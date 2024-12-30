use std::cell::{Cell, RefCell};
use std::time::Duration;

use niri_config::{Color, CornerRadius, GradientInterpolation, WindowRule};
use smithay::backend::renderer::element::surface::render_elements_from_surface_tree;
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::space::SpaceElement as _;
use smithay::desktop::{PopupManager, Window};
use smithay::output::{self, Output};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource as _;
use smithay::utils::{Logical, Point, Rectangle, Scale, Serial, Size, Transform};
use smithay::wayland::compositor::{remove_pre_commit_hook, with_states, HookId};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::xdg::{SurfaceCachedState, ToplevelSurface};
use wayland_backend::server::Credentials;

use super::{ResolvedWindowRules, WindowRef};
use crate::handlers::KdeDecorationsModeState;
use crate::layout::{
    ConfigureIntent, InteractiveResizeData, LayoutElement, LayoutElementRenderElement,
    LayoutElementRenderSnapshot,
};
use crate::niri::WindowOffscreenId;
use crate::niri_render_elements;
use crate::render_helpers::border::BorderRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::snapshot::RenderSnapshot;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::surface::render_snapshot_from_surface_tree;
use crate::render_helpers::{BakedBuffer, RenderTarget, SplitElements};
use crate::utils::id::IdCounter;
use crate::utils::transaction::Transaction;
use crate::utils::{
    get_credentials_for_surface, send_scale_transform, with_toplevel_role, ResizeEdge,
};

#[derive(Debug)]
pub struct Mapped {
    pub window: Window,

    /// Unique ID of this `Mapped`.
    id: MappedId,

    /// Credentials of the process that created the Wayland connection.
    credentials: Option<Credentials>,

    /// Pre-commit hook that we have on all mapped toplevel surfaces.
    pre_commit_hook: HookId,

    /// Up-to-date rules.
    rules: ResolvedWindowRules,

    /// Whether the window rules need to be recomputed.
    ///
    /// This is not used in all cases; for example, app ID and title changes recompute the rules
    /// immediately, rather than setting this flag.
    need_to_recompute_rules: bool,

    /// Whether this window has the keyboard focus.
    is_focused: bool,

    /// Whether this window is the active window in its column.
    is_active_in_column: bool,

    /// Whether this window is floating.
    is_floating: bool,

    /// Buffer to draw instead of the window when it should be blocked out.
    block_out_buffer: RefCell<SolidColorBuffer>,

    /// Whether the next configure should be animated, if the configured state changed.
    animate_next_configure: bool,

    /// Serials of commits that should be animated.
    animate_serials: Vec<Serial>,

    /// Snapshot right before an animated commit.
    animation_snapshot: Option<LayoutElementRenderSnapshot>,

    request_size_once: Option<RequestSizeOnce>,

    /// Transaction that the next configure should take part in, if any.
    transaction_for_next_configure: Option<Transaction>,

    /// Pending transactions that have not been added as blockers for this window yet.
    pending_transactions: Vec<(Serial, Transaction)>,

    /// State of an ongoing interactive resize.
    interactive_resize: Option<InteractiveResize>,

    /// Last time interactive resize was started.
    ///
    /// Used for double-resize-click tracking.
    last_interactive_resize_start: Cell<Option<(Duration, ResizeEdge)>>,
}

niri_render_elements! {
    WindowCastRenderElements<R> => {
        Layout = LayoutElementRenderElement<R>,
        // Blocked-out window with rounded corners.
        Border = BorderRenderElement,
    }
}

static MAPPED_ID_COUNTER: IdCounter = IdCounter::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MappedId(u64);

impl MappedId {
    fn next() -> MappedId {
        MappedId(MAPPED_ID_COUNTER.next())
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Interactive resize state.
#[derive(Debug)]
enum InteractiveResize {
    /// The resize is ongoing.
    Ongoing(InteractiveResizeData),
    /// The resize has stopped and we're waiting to send the last configure.
    WaitingForLastConfigure(InteractiveResizeData),
    /// We had sent the last resize configure and are waiting for the corresponding commit.
    WaitingForLastCommit {
        data: InteractiveResizeData,
        serial: Serial,
    },
}

impl InteractiveResize {
    fn data(&self) -> InteractiveResizeData {
        match self {
            InteractiveResize::Ongoing(data) => *data,
            InteractiveResize::WaitingForLastConfigure(data) => *data,
            InteractiveResize::WaitingForLastCommit { data, .. } => *data,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RequestSizeOnce {
    WaitingForConfigure,
    WaitingForCommit(Serial),
    UseWindowSize,
}

impl Mapped {
    pub fn new(window: Window, rules: ResolvedWindowRules, hook: HookId) -> Self {
        let surface = window.wl_surface().expect("no X11 support");
        let credentials = get_credentials_for_surface(&surface);

        Self {
            window,
            id: MappedId::next(),
            credentials,
            pre_commit_hook: hook,
            rules,
            need_to_recompute_rules: false,
            is_focused: false,
            is_active_in_column: false,
            is_floating: false,
            block_out_buffer: RefCell::new(SolidColorBuffer::new((0., 0.), [0., 0., 0., 1.])),
            animate_next_configure: false,
            animate_serials: Vec::new(),
            animation_snapshot: None,
            request_size_once: None,
            transaction_for_next_configure: None,
            pending_transactions: Vec::new(),
            interactive_resize: None,
            last_interactive_resize_start: Cell::new(None),
        }
    }

    pub fn toplevel(&self) -> &ToplevelSurface {
        self.window.toplevel().expect("no X11 support")
    }

    /// Recomputes the resolved window rules and returns whether they changed.
    pub fn recompute_window_rules(&mut self, rules: &[WindowRule], is_at_startup: bool) -> bool {
        self.need_to_recompute_rules = false;

        let new_rules = ResolvedWindowRules::compute(rules, WindowRef::Mapped(self), is_at_startup);
        if new_rules == self.rules {
            return false;
        }

        self.rules = new_rules;
        true
    }

    pub fn recompute_window_rules_if_needed(
        &mut self,
        rules: &[WindowRule],
        is_at_startup: bool,
    ) -> bool {
        if !self.need_to_recompute_rules {
            return false;
        }

        self.recompute_window_rules(rules, is_at_startup)
    }

    pub fn id(&self) -> MappedId {
        self.id
    }

    pub fn credentials(&self) -> Option<&Credentials> {
        self.credentials.as_ref()
    }

    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    pub fn is_active_in_column(&self) -> bool {
        self.is_active_in_column
    }

    pub fn is_floating(&self) -> bool {
        self.is_floating
    }

    pub fn set_is_focused(&mut self, is_focused: bool) {
        if self.is_focused == is_focused {
            return;
        }

        self.is_focused = is_focused;
        self.need_to_recompute_rules = true;
    }

    fn render_snapshot(&self, renderer: &mut GlesRenderer) -> LayoutElementRenderSnapshot {
        let _span = tracy_client::span!("Mapped::render_snapshot");

        let size = self.size().to_f64();

        let mut buffer = self.block_out_buffer.borrow_mut();
        buffer.resize(size);
        let blocked_out_contents = vec![BakedBuffer {
            buffer: buffer.clone(),
            location: Point::from((0., 0.)),
            src: None,
            dst: None,
        }];

        let buf_pos = self.window.geometry().loc.upscale(-1).to_f64();

        let mut contents = vec![];

        let surface = self.toplevel().wl_surface();
        for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
            let offset = self.window.geometry().loc + popup_offset - popup.geometry().loc;

            render_snapshot_from_surface_tree(
                renderer,
                popup.wl_surface(),
                buf_pos + offset.to_f64(),
                &mut contents,
            );
        }

        render_snapshot_from_surface_tree(renderer, surface, buf_pos, &mut contents);

        RenderSnapshot {
            contents,
            blocked_out_contents,
            block_out_from: self.rules().block_out_from,
            size,
            texture: Default::default(),
            blocked_out_texture: Default::default(),
        }
    }

    pub fn should_animate_commit(&mut self, commit_serial: Serial) -> bool {
        let mut should_animate = false;
        self.animate_serials.retain_mut(|serial| {
            if commit_serial.is_no_older_than(serial) {
                should_animate = true;
                false
            } else {
                true
            }
        });
        should_animate
    }

    pub fn store_animation_snapshot(&mut self, renderer: &mut GlesRenderer) {
        self.animation_snapshot = Some(self.render_snapshot(renderer));
    }

    pub fn take_pending_transaction(&mut self, commit_serial: Serial) -> Option<Transaction> {
        let mut rv = None;

        // Pending transactions are appended in order by serial, so we can loop from the start
        // until we hit a serial that is too new.
        while let Some((serial, _)) = self.pending_transactions.first() {
            // In this loop, we will complete the transaction corresponding to the commit, as well
            // as all transactions corresponding to previous serials. This can happen when we
            // request resizes too quickly, and the surface only responds to the last one.
            //
            // Note that in this case, completing the previous transactions can result in an
            // inconsistent visual state, if another window is waiting for this window to assume a
            // specific size (in a previous transaction), which is now different (in this commit).
            //
            // However, there isn't really a good way to deal with that. We cannot cancel any
            // transactions because we need to keep sending frame callbacks, and cancelling a
            // transaction will make the corresponding frame callbacks get lost, and the window
            // will hang.
            //
            // This is why resize throttling (implemented separately) is important: it prevents
            // visually inconsistent states by way of never having more than one transaction in
            // flight.
            if commit_serial.is_no_older_than(serial) {
                let (_, transaction) = self.pending_transactions.remove(0);
                // Previous transaction is dropped here, signaling completion.
                rv = Some(transaction);
            } else {
                break;
            }
        }

        rv
    }

    pub fn last_interactive_resize_start(&self) -> &Cell<Option<(Duration, ResizeEdge)>> {
        &self.last_interactive_resize_start
    }

    pub fn render_for_screen_cast<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        scale: Scale<f64>,
    ) -> impl DoubleEndedIterator<Item = WindowCastRenderElements<R>> {
        let bbox = self.window.bbox_with_popups().to_physical_precise_up(scale);

        let has_border_shader = BorderRenderElement::has_shader(renderer);
        let rules = self.rules();
        let radius = rules.geometry_corner_radius.unwrap_or_default();
        let window_size = self
            .size()
            .to_f64()
            .to_physical_precise_round(scale)
            .to_logical(scale);
        let radius = radius.fit_to(window_size.w as f32, window_size.h as f32);

        let location = self.window.geometry().loc.to_f64() - bbox.loc.to_logical(scale);
        let elements = self.render(renderer, location, scale, 1., RenderTarget::Screencast);

        elements.into_iter().map(move |elem| {
            if let LayoutElementRenderElement::SolidColor(elem) = &elem {
                // In this branch we're rendering a blocked-out window with a solid color. We need
                // to render it with a rounded corner shader even if clip_to_geometry is false,
                // because in this case we're assuming that the unclipped window CSD already has
                // corners rounded to the user-provided radius, so our blocked-out rendering should
                // match that radius.
                if radius != CornerRadius::default() && has_border_shader {
                    let geo = elem.geo();
                    return BorderRenderElement::new(
                        geo.size,
                        Rectangle::from_loc_and_size((0., 0.), geo.size),
                        GradientInterpolation::default(),
                        Color::from_color32f(elem.color()),
                        Color::from_color32f(elem.color()),
                        0.,
                        Rectangle::from_loc_and_size((0., 0.), geo.size),
                        0.,
                        radius,
                        scale.x as f32,
                    )
                    .with_location(geo.loc)
                    .into();
                }
            }

            WindowCastRenderElements::from(elem)
        })
    }
}

impl Drop for Mapped {
    fn drop(&mut self) {
        remove_pre_commit_hook(self.toplevel().wl_surface(), self.pre_commit_hook);
    }
}

impl LayoutElement for Mapped {
    type Id = Window;

    fn id(&self) -> &Self::Id {
        &self.window
    }

    fn size(&self) -> Size<i32, Logical> {
        self.window.geometry().size
    }

    fn buf_loc(&self) -> Point<i32, Logical> {
        Point::from((0, 0)) - self.window.geometry().loc
    }

    fn is_in_input_region(&self, point: Point<f64, Logical>) -> bool {
        let surface_local = point + self.window.geometry().loc.to_f64();
        self.window.is_in_input_region(&surface_local)
    }

    fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> SplitElements<LayoutElementRenderElement<R>> {
        let mut rv = SplitElements::default();

        if target.should_block_out(self.rules.block_out_from) {
            let mut buffer = self.block_out_buffer.borrow_mut();
            buffer.resize(self.window.geometry().size.to_f64());
            let elem =
                SolidColorRenderElement::from_buffer(&buffer, location, alpha, Kind::Unspecified);
            rv.normal.push(elem.into());
        } else {
            let buf_pos = location - self.window.geometry().loc.to_f64();

            let surface = self.toplevel().wl_surface();
            for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
                let offset = self.window.geometry().loc + popup_offset - popup.geometry().loc;

                rv.popups.extend(render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    (buf_pos + offset.to_f64()).to_physical_precise_round(scale),
                    scale,
                    alpha,
                    Kind::Unspecified,
                ));
            }

            rv.normal = render_elements_from_surface_tree(
                renderer,
                surface,
                buf_pos.to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            );
        }

        rv
    }

    fn render_normal<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> Vec<LayoutElementRenderElement<R>> {
        if target.should_block_out(self.rules.block_out_from) {
            let mut buffer = self.block_out_buffer.borrow_mut();
            buffer.resize(self.window.geometry().size.to_f64());
            let elem =
                SolidColorRenderElement::from_buffer(&buffer, location, alpha, Kind::Unspecified);
            vec![elem.into()]
        } else {
            let buf_pos = location - self.window.geometry().loc.to_f64();
            let surface = self.toplevel().wl_surface();
            render_elements_from_surface_tree(
                renderer,
                surface,
                buf_pos.to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            )
        }
    }

    fn render_popups<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> Vec<LayoutElementRenderElement<R>> {
        if target.should_block_out(self.rules.block_out_from) {
            vec![]
        } else {
            let mut rv = vec![];

            let buf_pos = location - self.window.geometry().loc.to_f64();
            let surface = self.toplevel().wl_surface();
            for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
                let offset = self.window.geometry().loc + popup_offset - popup.geometry().loc;

                rv.extend(render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    (buf_pos + offset.to_f64()).to_physical_precise_round(scale),
                    scale,
                    alpha,
                    Kind::Unspecified,
                ));
            }

            rv
        }
    }

    fn request_size(
        &mut self,
        size: Size<i32, Logical>,
        animate: bool,
        transaction: Option<Transaction>,
    ) {
        let changed = self.toplevel().with_pending_state(|state| {
            let changed = state.size != Some(size);
            state.size = Some(size);
            state.states.unset(xdg_toplevel::State::Fullscreen);
            changed
        });

        if changed && animate {
            self.animate_next_configure = true;
        }

        self.request_size_once = None;

        // Store the transaction regardless of whether the size changed. This is because with 3+
        // windows in a column, the size may change among windows 1 and 2 and then right away among
        // windows 2 and 3, and we want all windows 1, 2 and 3 to use the last transaction, rather
        // than window 1 getting stuck with the previous transaction that is immediately released
        // by 2.
        if let Some(transaction) = transaction {
            self.transaction_for_next_configure = Some(transaction);
        }
    }

    fn request_size_once(&mut self, size: Size<i32, Logical>, animate: bool) {
        // Assume that when calling this function, the window is going floating, so it can no
        // longer participate in any transactions with other windows.
        self.transaction_for_next_configure = None;

        // If our last requested size already matches the size we want to request-once, clear the
        // size request right away. However, we must also check if we're unfullscreening, because
        // in that case the window itself will restore its previous size upon receiving a (0, 0)
        // configure, whereas what we potentially want is to unfullscreen the window into its
        // fullscreen size.
        let already_sent = with_toplevel_role(self.toplevel(), |role| {
            let (last_sent, last_serial) = if let Some(configure) = role.pending_configures().last()
            {
                // FIXME: it would be more optimal to find the *oldest* pending configure that
                // has the same size and fullscreen state to the last pending configure.
                (&configure.state, configure.serial)
            } else {
                (
                    role.last_acked.as_ref().unwrap(),
                    role.configure_serial.unwrap(),
                )
            };

            let same_size = last_sent.size.unwrap_or_default() == size;
            let has_fullscreen = last_sent.states.contains(xdg_toplevel::State::Fullscreen);
            (same_size && !has_fullscreen).then_some(last_serial)
        });

        if let Some(serial) = already_sent {
            if let Some(current_serial) =
                with_toplevel_role(self.toplevel(), |role| role.current_serial)
            {
                // God this triple negative...
                if !current_serial.is_no_older_than(&serial) {
                    // We have already sent a request for the new size, but the surface has not
                    // committed in response yet, so we will wait for that commit.
                    self.request_size_once = Some(RequestSizeOnce::WaitingForCommit(serial));
                } else {
                    // We have already sent a request for the new size, and the surface has
                    // committed in response, so we will start using the current size right away.
                    self.request_size_once = Some(RequestSizeOnce::UseWindowSize);
                }
            } else {
                warn!("no current serial; did the surface not ack the initial configure?");
                self.request_size_once = Some(RequestSizeOnce::UseWindowSize);
            };
            return;
        }

        let changed = self.toplevel().with_pending_state(|state| {
            let changed = state.size != Some(size);
            state.size = Some(size);
            state.states.unset(xdg_toplevel::State::Fullscreen);
            changed
        });

        if changed && animate {
            self.animate_next_configure = true;
        }

        self.request_size_once = Some(RequestSizeOnce::WaitingForConfigure);
    }

    fn request_fullscreen(&mut self, size: Size<i32, Logical>) {
        self.toplevel().with_pending_state(|state| {
            state.size = Some(size);
            state.states.set(xdg_toplevel::State::Fullscreen);
        });

        self.request_size_once = None;
    }

    fn min_size(&self) -> Size<i32, Logical> {
        let min_size = with_states(self.toplevel().wl_surface(), |state| {
            let mut guard = state.cached_state.get::<SurfaceCachedState>();
            guard.current().min_size
        });

        self.rules.apply_min_size(min_size)
    }

    fn max_size(&self) -> Size<i32, Logical> {
        let max_size = with_states(self.toplevel().wl_surface(), |state| {
            let mut guard = state.cached_state.get::<SurfaceCachedState>();
            guard.current().max_size
        });

        self.rules.apply_max_size(max_size)
    }

    fn is_wl_surface(&self, wl_surface: &WlSurface) -> bool {
        self.toplevel().wl_surface() == wl_surface
    }

    fn set_preferred_scale_transform(&self, scale: output::Scale, transform: Transform) {
        self.window.with_surfaces(|surface, data| {
            send_scale_transform(surface, data, scale, transform);
        });
    }

    fn has_ssd(&self) -> bool {
        let toplevel = self.toplevel();
        let mode = with_toplevel_role(self.toplevel(), |role| role.current.decoration_mode);

        match mode {
            Some(zxdg_toplevel_decoration_v1::Mode::ServerSide) => true,
            // Check KDE decorations when XDG are not in use.
            None => with_states(toplevel.wl_surface(), |states| {
                states
                    .data_map
                    .get::<KdeDecorationsModeState>()
                    .map(KdeDecorationsModeState::is_server)
                    == Some(true)
            }),
            _ => false,
        }
    }

    fn output_enter(&self, output: &Output) {
        let overlap = Rectangle::from_loc_and_size((0, 0), (i32::MAX, i32::MAX));
        self.window.output_enter(output, overlap)
    }

    fn output_leave(&self, output: &Output) {
        self.window.output_leave(output)
    }

    fn set_offscreen_element_id(&self, id: Option<Id>) {
        let data = self
            .window
            .user_data()
            .get_or_insert(WindowOffscreenId::default);
        data.0.replace(id);
    }

    fn set_activated(&mut self, active: bool) {
        let changed = self.toplevel().with_pending_state(|state| {
            if active {
                state.states.set(xdg_toplevel::State::Activated)
            } else {
                state.states.unset(xdg_toplevel::State::Activated)
            }
        });
        self.need_to_recompute_rules |= changed;
    }

    fn set_active_in_column(&mut self, active: bool) {
        let changed = self.is_active_in_column != active;
        self.is_active_in_column = active;
        self.need_to_recompute_rules |= changed;
    }

    fn set_floating(&mut self, floating: bool) {
        let changed = self.is_floating != floating;
        self.is_floating = floating;
        self.need_to_recompute_rules |= changed;
    }

    fn set_bounds(&self, bounds: Size<i32, Logical>) {
        self.toplevel().with_pending_state(|state| {
            state.bounds = Some(bounds);
        });
    }

    fn configure_intent(&self) -> ConfigureIntent {
        let _span =
            trace_span!("configure_intent", surface = ?self.toplevel().wl_surface().id()).entered();

        with_toplevel_role(self.toplevel(), |attributes| {
            if let Some(server_pending) = &attributes.server_pending {
                let current_server = attributes.current_server_state();
                if server_pending != current_server {
                    // Something changed. Check if the only difference is the size, and if the
                    // current server size matches the current committed size.
                    let mut current_server_same_size = current_server.clone();
                    current_server_same_size.size = server_pending.size;
                    if current_server_same_size == *server_pending {
                        // Only the size changed. Check if the window committed our previous size
                        // request.
                        if attributes.current.size == current_server.size {
                            // The window had committed for our previous size change, so we can
                            // change the size again.
                            trace!(
                                "current size matches server size: {:?}",
                                attributes.current.size
                            );
                            ConfigureIntent::CanSend
                        } else {
                            // The window had not committed for our previous size change yet. Since
                            // nothing else changed, do not send the new size request yet. This
                            // throttling is done because some clients do not batch size requests,
                            // leading to bad behavior with very fast input devices (i.e. a 1000 Hz
                            // mouse). This throttling also helps interactive resize transactions
                            // preserve visual consistency.
                            trace!("throttling resize");
                            ConfigureIntent::Throttled
                        }
                    } else {
                        // Something else changed other than the size; send it.
                        trace!("something changed other than the size");
                        ConfigureIntent::ShouldSend
                    }
                } else {
                    // Nothing changed since the last configure.
                    ConfigureIntent::NotNeeded
                }
            } else {
                // Nothing changed since the last configure.
                ConfigureIntent::NotNeeded
            }
        })
    }

    fn send_pending_configure(&mut self) {
        let toplevel = self.toplevel();
        let _span =
            trace_span!("send_pending_configure", surface = ?toplevel.wl_surface().id()).entered();

        // Check for pending changes manually to account fo RequestSizeOnce::UseWindowSize.
        let has_pending_changes = with_toplevel_role(self.toplevel(), |role| {
            if role.server_pending.is_none() {
                return false;
            }

            let current_server_size = role.current_server_state().size;
            let server_pending = role.server_pending.as_mut().unwrap();

            // With UseWindowSize, we do not consider size-only changes, because we will
            // request the current window size and do not expect it to actually change.
            if let Some(RequestSizeOnce::UseWindowSize) = self.request_size_once {
                server_pending.size = current_server_size;
            }

            let server_pending = role.server_pending.as_ref().unwrap();
            server_pending != role.current_server_state()
        });

        if has_pending_changes {
            // If needed, replace the pending size with the current window size.
            if let Some(RequestSizeOnce::UseWindowSize) = self.request_size_once {
                let size = self.window.geometry().size;
                toplevel.with_pending_state(|state| {
                    state.size = Some(size);
                });
            }

            let serial = toplevel.send_configure();
            trace!(?serial, "sending configure");

            if self.animate_next_configure {
                self.animate_serials.push(serial);
            }

            if let Some(transaction) = self.transaction_for_next_configure.take() {
                self.pending_transactions.push((serial, transaction));
            }

            self.interactive_resize = match self.interactive_resize.take() {
                Some(InteractiveResize::WaitingForLastConfigure(data)) => {
                    Some(InteractiveResize::WaitingForLastCommit { data, serial })
                }
                x => x,
            };

            if let Some(RequestSizeOnce::WaitingForConfigure) = self.request_size_once {
                self.request_size_once = Some(RequestSizeOnce::WaitingForCommit(serial));
            }
        } else {
            self.interactive_resize = match self.interactive_resize.take() {
                // We probably started and stopped resizing in the same loop cycle without anything
                // changing.
                Some(InteractiveResize::WaitingForLastConfigure { .. }) => None,
                x => x,
            };
        }

        self.animate_next_configure = false;
        self.transaction_for_next_configure = None;
    }

    fn is_fullscreen(&self) -> bool {
        with_toplevel_role(self.toplevel(), |role| {
            role.current
                .states
                .contains(xdg_toplevel::State::Fullscreen)
        })
    }

    fn is_pending_fullscreen(&self) -> bool {
        self.toplevel()
            .with_pending_state(|state| state.states.contains(xdg_toplevel::State::Fullscreen))
    }

    fn requested_size(&self) -> Option<Size<i32, Logical>> {
        self.toplevel().with_pending_state(|state| state.size)
    }

    fn expected_size(&self) -> Option<Size<i32, Logical>> {
        // We can only use current size if it's not fullscreen.
        let current_size = (!self.is_fullscreen()).then(|| self.window.geometry().size);

        // Check if we should be using the current window size.
        //
        // This branch can be useful (give different result than the logic below) in this example
        // case:
        //
        // 1. We request_size_once a size change.
        // 2. We send a second configure requesting a state change.
        // 3. The window acks and commits-to the first configure but not the second, with a
        //    different size.
        //
        // In this case self.request_size_once will already flip to UseWindowSize and this branch
        // will return the window's own new size, but the logic below would see an uncommitted size
        // change and return our size.
        if let Some(RequestSizeOnce::UseWindowSize) = self.request_size_once {
            return current_size;
        }

        let pending = with_toplevel_role(self.toplevel(), |role| {
            // If we have a server-pending size change that we haven't sent yet, use that size.
            if let Some(server_pending) = &role.server_pending {
                let current_server = role.current_server_state();
                if server_pending.size != current_server.size {
                    return Some((
                        server_pending.size.unwrap_or_default(),
                        server_pending
                            .states
                            .contains(xdg_toplevel::State::Fullscreen),
                    ));
                }
            }

            // If we have a sent-but-not-committed-to size, use that.
            let (last_sent, last_serial) = if let Some(configure) = role.pending_configures().last()
            {
                (&configure.state, configure.serial)
            } else {
                (
                    role.last_acked.as_ref().unwrap(),
                    role.configure_serial.unwrap(),
                )
            };

            if let Some(current_serial) = role.current_serial {
                if !current_serial.is_no_older_than(&last_serial) {
                    return Some((
                        last_sent.size.unwrap_or_default(),
                        last_sent.states.contains(xdg_toplevel::State::Fullscreen),
                    ));
                }
            }

            None
        });

        if let Some((mut size, fullscreen)) = pending {
            // If the pending change is fullscreen, we can't use that size.
            if fullscreen {
                return None;
            }

            // If some component of the pending size is zero, substitute it with the current window
            // size. But only if the current size is not fullscreen.
            if size.w == 0 {
                size.w = current_size?.w;
            }
            if size.h == 0 {
                size.h = current_size?.h;
            }

            Some(size)
        } else {
            // No pending size, return the current size if it's non-fullscreen.
            current_size
        }
    }

    fn is_child_of(&self, parent: &Self) -> bool {
        self.toplevel().parent().as_ref() == Some(parent.toplevel().wl_surface())
    }

    fn refresh(&self) {
        self.window.refresh();
    }

    fn rules(&self) -> &ResolvedWindowRules {
        &self.rules
    }

    fn animation_snapshot(&self) -> Option<&LayoutElementRenderSnapshot> {
        self.animation_snapshot.as_ref()
    }

    fn take_animation_snapshot(&mut self) -> Option<LayoutElementRenderSnapshot> {
        self.animation_snapshot.take()
    }

    fn set_interactive_resize(&mut self, data: Option<InteractiveResizeData>) {
        self.toplevel().with_pending_state(|state| {
            if data.is_some() {
                state.states.set(xdg_toplevel::State::Resizing);
            } else {
                state.states.unset(xdg_toplevel::State::Resizing);
            }
        });

        if let Some(data) = data {
            self.interactive_resize = Some(InteractiveResize::Ongoing(data));
        } else {
            self.interactive_resize = match self.interactive_resize.take() {
                Some(InteractiveResize::Ongoing(data)) => {
                    Some(InteractiveResize::WaitingForLastConfigure(data))
                }
                x => x,
            }
        }
    }

    fn cancel_interactive_resize(&mut self) {
        self.set_interactive_resize(None);
        self.interactive_resize = None;
    }

    fn interactive_resize_data(&self) -> Option<InteractiveResizeData> {
        Some(self.interactive_resize.as_ref()?.data())
    }

    fn on_commit(&mut self, commit_serial: Serial) {
        if let Some(InteractiveResize::WaitingForLastCommit { serial, .. }) =
            &self.interactive_resize
        {
            if commit_serial.is_no_older_than(serial) {
                self.interactive_resize = None;
            }
        }

        if let Some(RequestSizeOnce::WaitingForCommit(serial)) = &self.request_size_once {
            if commit_serial.is_no_older_than(serial) {
                self.request_size_once = Some(RequestSizeOnce::UseWindowSize);
            }
        }
    }
}
