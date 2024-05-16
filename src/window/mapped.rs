use std::cell::{Cell, RefCell};
use std::cmp::{max, min};
use std::time::Duration;

use niri_config::WindowRule;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::render_elements_from_surface_tree;
use smithay::backend::renderer::element::{Id, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::space::SpaceElement as _;
use smithay::desktop::{PopupManager, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Scale, Serial, Size, Transform};
use smithay::wayland::compositor::{
    remove_pre_commit_hook, send_surface_state, with_states, HookId,
};
use smithay::wayland::shell::xdg::{SurfaceCachedState, ToplevelSurface};

use super::{ResolvedWindowRules, WindowRef};
use crate::layout::{
    InteractiveResizeData, LayoutElement, LayoutElementRenderElement, LayoutElementRenderSnapshot,
};
use crate::niri::WindowOffscreenId;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::snapshot::RenderSnapshot;
use crate::render_helpers::surface::render_snapshot_from_surface_tree;
use crate::render_helpers::{BakedBuffer, RenderTarget, SplitElements};
use crate::utils::ResizeEdge;

#[derive(Debug)]
pub struct Mapped {
    pub window: Window,

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
    pub is_active_in_column: bool,

    /// Buffer to draw instead of the window when it should be blocked out.
    block_out_buffer: RefCell<SolidColorBuffer>,

    /// Whether the next configure should be animated, if the configured state changed.
    animate_next_configure: bool,

    /// Serials of commits that should be animated.
    animate_serials: Vec<Serial>,

    /// Snapshot right before an animated commit.
    animation_snapshot: Option<LayoutElementRenderSnapshot>,

    /// State of an ongoing interactive resize.
    interactive_resize: Option<InteractiveResize>,

    /// Last time interactive resize was started.
    ///
    /// Used for double-resize-click tracking.
    last_interactive_resize_start: Cell<Option<(Duration, ResizeEdge)>>,
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

impl Mapped {
    pub fn new(window: Window, rules: ResolvedWindowRules, hook: HookId) -> Self {
        Self {
            window,
            pre_commit_hook: hook,
            rules,
            need_to_recompute_rules: false,
            is_focused: false,
            is_active_in_column: false,
            block_out_buffer: RefCell::new(SolidColorBuffer::new((0, 0), [0., 0., 0., 1.])),
            animate_next_configure: false,
            animate_serials: Vec::new(),
            animation_snapshot: None,
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

    pub fn is_focused(&self) -> bool {
        self.is_focused
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

        let size = self.size();

        let mut buffer = self.block_out_buffer.borrow_mut();
        buffer.resize(size);
        let blocked_out_contents = vec![BakedBuffer {
            buffer: buffer.clone(),
            location: Point::from((0, 0)),
            src: None,
            dst: None,
        }];

        let buf_pos = self.window.geometry().loc.upscale(-1);

        let mut contents = vec![];

        let surface = self.toplevel().wl_surface();
        for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
            let offset = self.window.geometry().loc + popup_offset - popup.geometry().loc;

            render_snapshot_from_surface_tree(
                renderer,
                popup.wl_surface(),
                buf_pos + offset,
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

    pub fn last_interactive_resize_start(&self) -> &Cell<Option<(Duration, ResizeEdge)>> {
        &self.last_interactive_resize_start
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
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> SplitElements<LayoutElementRenderElement<R>> {
        let mut rv = SplitElements::default();

        if target.should_block_out(self.rules.block_out_from) {
            let mut buffer = self.block_out_buffer.borrow_mut();
            buffer.resize(self.window.geometry().size);
            let elem = SolidColorRenderElement::from_buffer(
                &buffer,
                location.to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            );
            rv.normal.push(elem.into());
        } else {
            let buf_pos = location - self.window.geometry().loc;

            let surface = self.toplevel().wl_surface();
            for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
                let offset = self.window.geometry().loc + popup_offset - popup.geometry().loc;

                rv.popups.extend(render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    (buf_pos + offset).to_physical_precise_round(scale),
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
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> Vec<LayoutElementRenderElement<R>> {
        if target.should_block_out(self.rules.block_out_from) {
            let mut buffer = self.block_out_buffer.borrow_mut();
            buffer.resize(self.window.geometry().size);
            let elem = SolidColorRenderElement::from_buffer(
                &buffer,
                location.to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            );
            vec![elem.into()]
        } else {
            let buf_pos = location - self.window.geometry().loc;
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
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> Vec<LayoutElementRenderElement<R>> {
        if target.should_block_out(self.rules.block_out_from) {
            vec![]
        } else {
            let mut rv = vec![];

            let buf_pos = location - self.window.geometry().loc;
            let surface = self.toplevel().wl_surface();
            for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
                let offset = self.window.geometry().loc + popup_offset - popup.geometry().loc;

                rv.extend(render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    (buf_pos + offset).to_physical_precise_round(scale),
                    scale,
                    alpha,
                    Kind::Unspecified,
                ));
            }

            rv
        }
    }

    fn request_size(&mut self, size: Size<i32, Logical>, animate: bool) {
        let changed = self.toplevel().with_pending_state(|state| {
            let changed = state.size != Some(size);
            state.size = Some(size);
            state.states.unset(xdg_toplevel::State::Fullscreen);
            changed
        });

        if changed && animate {
            self.animate_next_configure = true;
        }
    }

    fn request_fullscreen(&self, size: Size<i32, Logical>) {
        self.toplevel().with_pending_state(|state| {
            state.size = Some(size);
            state.states.set(xdg_toplevel::State::Fullscreen);
        });
    }

    fn min_size(&self) -> Size<i32, Logical> {
        let mut size = with_states(self.toplevel().wl_surface(), |state| {
            let curr = state.cached_state.current::<SurfaceCachedState>();
            curr.min_size
        });

        if let Some(x) = self.rules.min_width {
            size.w = max(size.w, i32::from(x));
        }
        if let Some(x) = self.rules.min_height {
            size.h = max(size.h, i32::from(x));
        }

        size
    }

    fn max_size(&self) -> Size<i32, Logical> {
        let mut size = with_states(self.toplevel().wl_surface(), |state| {
            let curr = state.cached_state.current::<SurfaceCachedState>();
            curr.max_size
        });

        if let Some(x) = self.rules.max_width {
            if size.w == 0 {
                size.w = i32::from(x);
            } else if x > 0 {
                size.w = min(size.w, i32::from(x));
            }
        }
        if let Some(x) = self.rules.max_height {
            if size.h == 0 {
                size.h = i32::from(x);
            } else if x > 0 {
                size.h = min(size.h, i32::from(x));
            }
        }

        size
    }

    fn is_wl_surface(&self, wl_surface: &WlSurface) -> bool {
        self.toplevel().wl_surface() == wl_surface
    }

    fn set_preferred_scale_transform(&self, scale: i32, transform: Transform) {
        self.window.with_surfaces(|surface, data| {
            send_surface_state(surface, data, scale, transform);
        });
    }

    fn has_ssd(&self) -> bool {
        self.toplevel().current_state().decoration_mode
            == Some(zxdg_toplevel_decoration_v1::Mode::ServerSide)
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

    fn set_bounds(&self, bounds: Size<i32, Logical>) {
        self.toplevel().with_pending_state(|state| {
            state.bounds = Some(bounds);
        });
    }

    fn send_pending_configure(&mut self) {
        if let Some(serial) = self.toplevel().send_pending_configure() {
            if self.animate_next_configure {
                self.animate_serials.push(serial);
            }

            self.interactive_resize = match self.interactive_resize.take() {
                Some(InteractiveResize::WaitingForLastConfigure(data)) => {
                    Some(InteractiveResize::WaitingForLastCommit { data, serial })
                }
                x => x,
            }
        } else {
            self.interactive_resize = match self.interactive_resize.take() {
                // We probably started and stopped resizing in the same loop cycle without anything
                // changing.
                Some(InteractiveResize::WaitingForLastConfigure { .. }) => None,
                x => x,
            }
        }

        self.animate_next_configure = false;
    }

    fn is_fullscreen(&self) -> bool {
        self.toplevel()
            .current_state()
            .states
            .contains(xdg_toplevel::State::Fullscreen)
    }

    fn is_pending_fullscreen(&self) -> bool {
        self.toplevel()
            .with_pending_state(|state| state.states.contains(xdg_toplevel::State::Fullscreen))
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

    fn update_interactive_resize(&mut self, commit_serial: Serial) {
        if let Some(InteractiveResize::WaitingForLastCommit { serial, .. }) =
            &self.interactive_resize
        {
            if commit_serial.is_no_older_than(serial) {
                self.interactive_resize = None;
            }
        }
    }

    fn interactive_resize_data(&self) -> Option<InteractiveResizeData> {
        Some(self.interactive_resize.as_ref()?.data())
    }
}
