use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{env, mem, thread};

use _server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDecorationsMode;
use anyhow::Context;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::{
    default_primary_scanout_output_compare, render_elements, AsRenderElements, Kind, RenderElement,
    RenderElementStates,
};
use smithay::backend::renderer::gles::{GlesMapping, GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{Bind, ExportMem, Frame, ImportAll, Offscreen, Renderer};
use smithay::desktop::utils::{
    bbox_from_surface_tree, output_update, send_dmabuf_feedback_surface_tree,
    send_frames_surface_tree, surface_presentation_feedback_flags_from_states,
    surface_primary_scanout_output, take_presentation_feedback_surface_tree,
    under_from_surface_tree, update_surface_primary_scanout_output, OutputPresentationFeedback,
};
use smithay::desktop::{layer_map_for_output, PopupManager, Space, Window, WindowSurfaceType};
use smithay::input::keyboard::{Layout as KeyboardLayout, XkbConfig, XkbContextHandler};
use smithay::input::pointer::{CursorIcon, CursorImageAttributes, CursorImageStatus, MotionEvent};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{
    self, Idle, Interest, LoopHandle, LoopSignal, Mode, PostAction, RegistrationToken,
};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities;
use smithay::reexports::wayland_protocols_misc::server_decoration as _server_decoration;
use smithay::reexports::wayland_server::backend::{
    ClientData, ClientId, DisconnectReason, GlobalId,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{
    ClockSource, Logical, Monotonic, Physical, Point, Rectangle, Scale, Size, Transform,
    SERIAL_COUNTER,
};
use smithay::wayland::compositor::{
    send_surface_state, with_states, with_surface_tree_downward, CompositorClientState,
    CompositorState, SurfaceData, TraversalAction,
};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::DmabufFeedback;
use smithay::wayland::input_method::InputMethodManagerState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_gestures::PointerGesturesState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::selection::data_device::{set_data_device_selection, DataDeviceState};
use smithay::wayland::selection::primary_selection::{
    set_primary_selection, PrimarySelectionState,
};
use smithay::wayland::selection::wlr_data_control::DataControlState;
use smithay::wayland::session_lock::{LockSurface, SessionLockManagerState, SessionLocker};
use smithay::wayland::shell::kde::decoration::KdeDecorationState;
use smithay::wayland::shell::wlr_layer::{Layer, WlrLayerShellState};
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::tablet_manager::TabletManagerState;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState;

use crate::backend::{Backend, RenderResult, Tty, Winit};
use crate::config::{Config, TrackLayout};
use crate::cursor::{CursorManager, CursorTextureCache, RenderCursor, XCursor};
#[cfg(feature = "dbus")]
use crate::dbus::gnome_shell_screenshot::{NiriToScreenshot, ScreenshotToNiri};
#[cfg(feature = "xdp-gnome-screencast")]
use crate::dbus::mutter_screen_cast::{self, ScreenCastToNiri};
use crate::frame_clock::FrameClock;
use crate::handlers::configure_lock_surface;
use crate::layout::{output_size, Layout, MonitorRenderElement};
use crate::pw_utils::{Cast, PipeWire};
use crate::screenshot_ui::{ScreenshotUi, ScreenshotUiRenderElement};
use crate::utils::{center, get_monotonic_time, make_screenshot_path, write_png_rgba8};

const CLEAR_COLOR: [f32; 4] = [0.2, 0.2, 0.2, 1.];
const CLEAR_COLOR_LOCKED: [f32; 4] = [0.3, 0.1, 0.1, 1.];

pub struct Niri {
    pub config: Rc<RefCell<Config>>,

    pub event_loop: LoopHandle<'static, State>,
    pub stop_signal: LoopSignal,
    pub display_handle: DisplayHandle,
    pub socket_name: OsString,

    pub start_time: Instant,

    // Each workspace corresponds to a Space. Each workspace generally has one Output mapped to it,
    // however it may have none (when there are no outputs connected) or mutiple (when mirroring).
    pub layout: Layout<Window>,

    // This space does not actually contain any windows, but all outputs are mapped into it
    // according to their global position.
    pub global_space: Space<Window>,

    // Windows which don't have a buffer attached yet.
    pub unmapped_windows: HashMap<WlSurface, Window>,

    pub output_state: HashMap<Output, OutputState>,
    pub output_by_name: HashMap<String, Output>,

    // When false, we're idling with monitors powered off.
    pub monitors_active: bool,

    // Smithay state.
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub kde_decoration_state: KdeDecorationState,
    pub layer_shell_state: WlrLayerShellState,
    pub session_lock_state: SessionLockManagerState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<State>,
    pub tablet_state: TabletManagerState,
    pub text_input_state: TextInputManagerState,
    pub input_method_state: InputMethodManagerState,
    pub virtual_keyboard_state: VirtualKeyboardManagerState,
    pub pointer_gestures_state: PointerGesturesState,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,
    pub popups: PopupManager,
    pub presentation_state: PresentationState,

    pub seat: Seat<State>,
    /// Scancodes of the keys to suppress.
    pub suppressed_keys: HashSet<u32>,

    pub cursor_manager: CursorManager,
    pub cursor_texture_cache: CursorTextureCache,
    pub cursor_shape_manager_state: CursorShapeManagerState,
    pub dnd_icon: Option<WlSurface>,
    pub pointer_focus: Option<PointerFocus>,

    pub lock_state: LockState,

    pub screenshot_ui: ScreenshotUi,

    #[cfg(feature = "dbus")]
    pub dbus: Option<crate::dbus::DBusServers>,
    #[cfg(feature = "dbus")]
    pub inhibit_power_key_fd: Option<zbus::zvariant::OwnedFd>,

    // Casts are dropped before PipeWire to prevent a double-free (yay).
    pub casts: Vec<Cast>,
    pub pipewire: Option<PipeWire>,
}

pub struct OutputState {
    pub global: GlobalId,
    pub frame_clock: FrameClock,
    pub redraw_state: RedrawState,
    // After the last redraw, some ongoing animations still remain.
    pub unfinished_animations_remain: bool,
    /// Estimated sequence currently displayed on this output.
    ///
    /// When a frame is presented on this output, this becomes the real sequence from the VBlank
    /// callback. Then, as long as there are no KMS submissions, but we keep getting commits, this
    /// sequence increases by 1 at estimated VBlank times.
    ///
    /// If there are no commits, then we won't have a timer running, so the estimated sequence will
    /// not increase.
    pub current_estimated_sequence: Option<u32>,
    /// Solid color buffer for the background that we use instead of clearing to avoid damage
    /// tracking issues and make screenshots easier.
    pub background_buffer: SolidColorBuffer,
    pub lock_render_state: LockRenderState,
    pub lock_surface: Option<LockSurface>,
    pub lock_color_buffer: SolidColorBuffer,
}

#[derive(Default)]
pub enum RedrawState {
    /// The compositor is idle.
    #[default]
    Idle,
    /// A redraw is queued.
    Queued(Idle<'static>),
    /// We submitted a frame to the KMS and waiting for it to be presented.
    WaitingForVBlank { redraw_needed: bool },
    /// We did not submit anything to KMS and made a timer to fire at the estimated VBlank.
    WaitingForEstimatedVBlank(RegistrationToken),
    /// A redraw is queued on top of the above.
    WaitingForEstimatedVBlankAndQueued((RegistrationToken, Idle<'static>)),
}

#[derive(Clone, PartialEq, Eq)]
pub struct PointerFocus {
    pub output: Output,
    pub surface: (WlSurface, Point<i32, Logical>),
}

#[derive(Default)]
pub enum LockState {
    #[default]
    Unlocked,
    Locking(SessionLocker),
    Locked,
}

#[derive(PartialEq, Eq)]
pub enum LockRenderState {
    /// The output displays a normal session frame.
    Unlocked,
    /// The output displays a locked frame.
    Locked,
}

// Not related to the one in Smithay.
//
// This state keeps track of when a surface last received a frame callback.
struct SurfaceFrameThrottlingState {
    /// Output and sequence that the frame callback was last sent at.
    last_sent_at: RefCell<Option<(Output, u32)>>,
}

impl Default for SurfaceFrameThrottlingState {
    fn default() -> Self {
        Self {
            last_sent_at: RefCell::new(None),
        }
    }
}

pub struct State {
    pub backend: Backend,
    pub niri: Niri,
}

impl State {
    pub fn new(
        config: Config,
        event_loop: LoopHandle<'static, State>,
        stop_signal: LoopSignal,
        display: Display<State>,
    ) -> Self {
        let _span = tracy_client::span!("State::new");

        let config = Rc::new(RefCell::new(config));

        let has_display =
            env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("DISPLAY").is_some();

        let mut backend = if has_display {
            Backend::Winit(Winit::new(config.clone(), event_loop.clone()))
        } else {
            Backend::Tty(Tty::new(config.clone(), event_loop.clone()))
        };

        let mut niri = Niri::new(config.clone(), event_loop, stop_signal, display, &backend);
        backend.init(&mut niri);

        Self { backend, niri }
    }

    pub fn refresh_and_flush_clients(&mut self) {
        let _span = tracy_client::span!("refresh_and_flush_clients");

        // These should be called periodically, before flushing the clients.
        self.niri.layout.refresh();
        self.niri.cursor_manager.check_cursor_image_surface_alive();
        self.niri.refresh_pointer_outputs();
        self.niri.popups.cleanup();
        self.update_focus();
        self.refresh_pointer_focus();

        {
            let _span = tracy_client::span!("flush_clients");
            self.niri.display_handle.flush_clients().unwrap();
        }
    }

    pub fn move_cursor(&mut self, location: Point<f64, Logical>) {
        let under = self.niri.surface_under_and_global_space(location);
        self.niri.pointer_focus = under.clone();
        let under = under.map(|u| u.surface);

        let pointer = &self.niri.seat.get_pointer().unwrap();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: get_monotonic_time().as_millis() as u32,
            },
        );
        pointer.frame(self);
        // FIXME: granular
        self.niri.queue_redraw_all();
    }

    pub fn refresh_pointer_focus(&mut self) {
        let _span = tracy_client::span!("Niri::refresh_pointer_focus");

        let pointer = &self.niri.seat.get_pointer().unwrap();
        let location = pointer.current_location();

        if !self.niri.is_locked() && !self.niri.screenshot_ui.is_open() {
            // Don't refresh cursor focus during transitions.
            if let Some((output, _)) = self.niri.output_under(location) {
                let monitor = self.niri.layout.monitor_for_output(output).unwrap();
                if monitor.are_transitions_ongoing() {
                    return;
                }
            }
        }

        if !self.update_pointer_focus() {
            return;
        }

        pointer.frame(self);
        // FIXME: granular
        self.niri.queue_redraw_all();
    }

    pub fn update_pointer_focus(&mut self) -> bool {
        let _span = tracy_client::span!("Niri::update_pointer_focus");

        let pointer = &self.niri.seat.get_pointer().unwrap();
        let location = pointer.current_location();
        let under = self.niri.surface_under_and_global_space(location);

        // We're not changing the global cursor location here, so if the focus did not change, then
        // nothing changed.
        if self.niri.pointer_focus == under {
            return false;
        }

        self.niri.pointer_focus = under.clone();
        let under = under.map(|u| u.surface);

        pointer.motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: get_monotonic_time().as_millis() as u32,
            },
        );

        true
    }

    pub fn move_cursor_to_output(&mut self, output: &Output) {
        let geo = self.niri.global_space.output_geometry(output).unwrap();
        self.move_cursor(center(geo).to_f64());
    }

    pub fn update_focus(&mut self) {
        let focus = if self.niri.is_locked() {
            self.niri.lock_surface_focus()
        } else if self.niri.screenshot_ui.is_open() {
            None
        } else {
            self.niri.layer_surface_focus().or_else(|| {
                self.niri
                    .layout
                    .focus()
                    .map(|win| win.toplevel().wl_surface().clone())
            })
        };

        let keyboard = self.niri.seat.get_keyboard().unwrap();
        let current_focus = keyboard.current_focus();
        if current_focus != focus {
            if self.niri.config.borrow().input.keyboard.track_layout == TrackLayout::Window {
                let current_layout =
                    keyboard.with_kkb_state(self, |context| context.active_layout());

                let mut new_layout = current_layout;
                // Store the currently active layout for the surface.
                if let Some(current_focus) = current_focus.as_ref() {
                    with_states(current_focus, |data| {
                        let cell = data
                            .data_map
                            .get_or_insert::<Cell<KeyboardLayout>, _>(Cell::default);
                        cell.set(current_layout);
                    });
                }

                if let Some(focus) = focus.as_ref() {
                    new_layout = with_states(focus, |data| {
                        let cell = data.data_map.get_or_insert::<Cell<KeyboardLayout>, _>(|| {
                            // The default layout is effectively the first layout in the
                            // keymap, so use it for new windows.
                            Cell::new(KeyboardLayout::default())
                        });
                        cell.get()
                    });
                }
                if new_layout != current_layout && focus.is_some() {
                    keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
                    keyboard.with_kkb_state(self, |mut context| {
                        context.set_layout(new_layout);
                    });
                }
            }

            keyboard.set_focus(self, focus, SERIAL_COUNTER.next_serial());

            // FIXME: can be more granular.
            self.niri.queue_redraw_all();
        }
    }

    pub fn reload_config(&mut self, path: PathBuf) {
        let _span = tracy_client::span!("State::reload_config");

        let config = match Config::load(Some(path)) {
            Ok((config, _)) => config,
            Err(err) => {
                warn!("{:?}", err.context("error loading config"));
                return;
            }
        };

        self.niri.layout.update_config(&config);

        let mut old_config = self.niri.config.borrow_mut();

        if config.cursor != old_config.cursor {
            self.niri
                .cursor_manager
                .reload(&config.cursor.xcursor_theme, config.cursor.xcursor_size);
            self.niri.cursor_texture_cache.clear();
        }

        *old_config = config;

        // Release the borrow.
        drop(old_config);

        self.niri.queue_redraw_all();
        // FIXME: apply output scale and whatnot.
        // FIXME: apply libinput device settings.
        // FIXME: apply xkb settings.
        // FIXME: apply xdg decoration settings.
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn on_screen_cast_msg(
        &mut self,
        to_niri: &calloop::channel::Sender<ScreenCastToNiri>,
        msg: ScreenCastToNiri,
    ) {
        match msg {
            ScreenCastToNiri::StartCast {
                session_id,
                output,
                cursor_mode,
                signal_ctx,
            } => {
                let _span = tracy_client::span!("StartCast");

                debug!(session_id, "StartCast");

                let gbm = match self.backend.gbm_device() {
                    Some(gbm) => gbm,
                    None => {
                        debug!("no GBM device available");
                        return;
                    }
                };

                let pw = self.niri.pipewire.as_ref().unwrap();
                match pw.start_cast(
                    to_niri.clone(),
                    gbm,
                    session_id,
                    output,
                    cursor_mode,
                    signal_ctx,
                ) {
                    Ok(cast) => {
                        self.niri.casts.push(cast);
                    }
                    Err(err) => {
                        warn!("error starting screencast: {err:?}");
                        self.niri.stop_cast(session_id);
                    }
                }
            }
            ScreenCastToNiri::StopCast { session_id } => self.niri.stop_cast(session_id),
        }
    }

    #[cfg(feature = "dbus")]
    pub fn on_screen_shot_msg(
        &mut self,
        to_screenshot: &async_channel::Sender<NiriToScreenshot>,
        msg: ScreenshotToNiri,
    ) {
        let ScreenshotToNiri::TakeScreenshot { include_cursor } = msg;
        let _span = tracy_client::span!("TakeScreenshot");

        let Some(renderer) = self.backend.renderer() else {
            let msg = NiriToScreenshot::ScreenshotResult(None);
            if let Err(err) = to_screenshot.send_blocking(msg) {
                warn!("error sending None to screenshot: {err:?}");
            }
            return;
        };

        let on_done = {
            let to_screenshot = to_screenshot.clone();
            move |path| {
                let msg = NiriToScreenshot::ScreenshotResult(Some(path));
                if let Err(err) = to_screenshot.send_blocking(msg) {
                    warn!("error sending path to screenshot: {err:?}");
                }
            }
        };

        let res = self
            .niri
            .screenshot_all_outputs(renderer, include_cursor, on_done);

        if let Err(err) = res {
            warn!("error taking a screenshot: {err:?}");

            let msg = NiriToScreenshot::ScreenshotResult(None);
            if let Err(err) = to_screenshot.send_blocking(msg) {
                warn!("error sending None to screenshot: {err:?}");
            }
        }
    }
}

impl Niri {
    pub fn new(
        config: Rc<RefCell<Config>>,
        event_loop: LoopHandle<'static, State>,
        stop_signal: LoopSignal,
        display: Display<State>,
        backend: &Backend,
    ) -> Self {
        let _span = tracy_client::span!("Niri::new");

        let display_handle = display.handle();
        let config_ = config.borrow();

        let layout = Layout::new(&config_);

        let compositor_state = CompositorState::new_v6::<State>(&display_handle);
        let xdg_shell_state = XdgShellState::new_with_capabilities::<State>(
            &display_handle,
            [WmCapabilities::Fullscreen],
        );
        let xdg_decoration_state = XdgDecorationState::new::<State>(&display_handle);
        let kde_decoration_state = KdeDecorationState::new::<State>(
            &display_handle,
            if config_.prefer_no_csd {
                KdeDecorationsMode::Server
            } else {
                KdeDecorationsMode::Client
            },
        );
        let layer_shell_state = WlrLayerShellState::new::<State>(&display_handle);
        let session_lock_state =
            SessionLockManagerState::new::<State, _>(&display_handle, |_| true);
        let shm_state = ShmState::new::<State>(&display_handle, vec![]);
        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<State>(&display_handle);
        let mut seat_state = SeatState::new();
        let tablet_state = TabletManagerState::new::<State>(&display_handle);
        let pointer_gestures_state = PointerGesturesState::new::<State>(&display_handle);
        let data_device_state = DataDeviceState::new::<State>(&display_handle);
        let primary_selection_state = PrimarySelectionState::new::<State>(&display_handle);
        let data_control_state = DataControlState::new::<State, _>(
            &display_handle,
            Some(&primary_selection_state),
            |_| true,
        );
        let presentation_state =
            PresentationState::new::<State>(&display_handle, Monotonic::ID as u32);

        let text_input_state = TextInputManagerState::new::<State>(&display_handle);
        let input_method_state =
            InputMethodManagerState::new::<State, _>(&display_handle, |_| true);
        let virtual_keyboard_state =
            VirtualKeyboardManagerState::new::<State, _>(&display_handle, |_| true);

        let mut seat: Seat<State> = seat_state.new_wl_seat(&display_handle, backend.seat_name());
        let xkb = XkbConfig {
            rules: &config_.input.keyboard.xkb.rules,
            model: &config_.input.keyboard.xkb.model,
            layout: config_.input.keyboard.xkb.layout.as_deref().unwrap_or("us"),
            variant: &config_.input.keyboard.xkb.variant,
            options: config_.input.keyboard.xkb.options.clone(),
        };
        seat.add_keyboard(
            xkb,
            config_.input.keyboard.repeat_delay as i32,
            config_.input.keyboard.repeat_rate as i32,
        )
        .unwrap();
        seat.add_pointer();

        let cursor_shape_manager_state = CursorShapeManagerState::new::<State>(&display_handle);
        let cursor_manager =
            CursorManager::new(&config_.cursor.xcursor_theme, config_.cursor.xcursor_size);

        let screenshot_ui = ScreenshotUi::new();

        let socket_source = ListeningSocketSource::new_auto().unwrap();
        let socket_name = socket_source.socket_name().to_os_string();
        event_loop
            .insert_source(socket_source, move |client, _, state| {
                let data = Arc::new(ClientState::default());
                if let Err(err) = state.niri.display_handle.insert_client(client, data) {
                    error!("error inserting client: {err}");
                }
            })
            .unwrap();

        let pipewire = match PipeWire::new(&event_loop) {
            Ok(pipewire) => Some(pipewire),
            Err(err) => {
                warn!("error starting PipeWire: {err:?}");
                None
            }
        };

        let display_source = Generic::new(display, Interest::READ, Mode::Level);
        event_loop
            .insert_source(display_source, |_, display, state| {
                // SAFETY: we don't drop the display.
                unsafe {
                    display.get_mut().dispatch_clients(state).unwrap();
                }
                Ok(PostAction::Continue)
            })
            .unwrap();

        drop(config_);
        Self {
            config,

            event_loop,
            stop_signal,
            socket_name,
            display_handle,
            start_time: Instant::now(),

            layout,
            global_space: Space::default(),
            output_state: HashMap::new(),
            output_by_name: HashMap::new(),
            unmapped_windows: HashMap::new(),
            monitors_active: true,

            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            kde_decoration_state,
            layer_shell_state,
            session_lock_state,
            text_input_state,
            input_method_state,
            virtual_keyboard_state,
            shm_state,
            output_manager_state,
            seat_state,
            tablet_state,
            pointer_gestures_state,
            data_device_state,
            primary_selection_state,
            data_control_state,
            popups: PopupManager::default(),
            suppressed_keys: HashSet::new(),
            presentation_state,

            seat,
            cursor_manager,
            cursor_texture_cache: Default::default(),
            cursor_shape_manager_state,
            dnd_icon: None,
            pointer_focus: None,

            lock_state: LockState::Unlocked,

            screenshot_ui,

            #[cfg(feature = "dbus")]
            dbus: None,
            #[cfg(feature = "dbus")]
            inhibit_power_key_fd: None,

            pipewire,
            casts: vec![],
        }
    }

    #[cfg(feature = "dbus")]
    pub fn inhibit_power_key(&mut self) -> anyhow::Result<()> {
        let conn = zbus::blocking::ConnectionBuilder::system()?.build()?;

        // logind-zbus has a wrong signature for this method, so do it manually.
        // https://gitlab.com/flukejones/logind-zbus/-/merge_requests/5
        let message = conn.call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &("handle-power-key", "niri", "Power key handling", "block"),
        )?;

        let fd = message.body()?;
        self.inhibit_power_key_fd = Some(fd);

        Ok(())
    }

    pub fn add_output(&mut self, output: Output, refresh_interval: Option<Duration>) {
        let global = output.create_global::<State>(&self.display_handle);

        let name = output.name();
        let config = self
            .config
            .borrow()
            .outputs
            .iter()
            .find(|o| o.name == name)
            .cloned()
            .unwrap_or_default();

        let size = output_size(&output);
        let position = config
            .position
            .map(|pos| Point::from((pos.x, pos.y)))
            .filter(|pos| {
                // Ensure that the requested position does not overlap any existing output.
                let target_geom = Rectangle::from_loc_and_size(*pos, size);

                let overlap = self
                    .global_space
                    .outputs()
                    .map(|output| self.global_space.output_geometry(output).unwrap())
                    .find(|geom| geom.overlaps(target_geom));

                if let Some(overlap) = overlap {
                    warn!(
                        "new output {name} at x={} y={} sized {}x{} \
                         overlaps an existing output at x={} y={} sized {}x{}, \
                         falling back to automatic placement",
                        pos.x,
                        pos.y,
                        size.w,
                        size.h,
                        overlap.loc.x,
                        overlap.loc.y,
                        overlap.size.w,
                        overlap.size.h,
                    );

                    false
                } else {
                    true
                }
            })
            .unwrap_or_else(|| {
                let x = self
                    .global_space
                    .outputs()
                    .map(|output| self.global_space.output_geometry(output).unwrap())
                    .map(|geom| geom.loc.x + geom.size.w)
                    .max()
                    .unwrap_or(0);

                Point::from((x, 0))
            });

        debug!(
            "putting new output {name} at x={} y={}",
            position.x, position.y
        );
        self.global_space.map_output(&output, position);
        self.layout.add_output(output.clone());
        output.change_current_state(None, None, None, Some(position));

        let lock_render_state = if self.is_locked() {
            // We haven't rendered anything yet so it's as good as locked.
            LockRenderState::Locked
        } else {
            LockRenderState::Unlocked
        };

        let state = OutputState {
            global,
            redraw_state: RedrawState::Idle,
            unfinished_animations_remain: false,
            frame_clock: FrameClock::new(refresh_interval),
            current_estimated_sequence: None,
            background_buffer: SolidColorBuffer::new(size, CLEAR_COLOR),
            lock_render_state,
            lock_surface: None,
            lock_color_buffer: SolidColorBuffer::new(size, CLEAR_COLOR_LOCKED),
        };
        let rv = self.output_state.insert(output.clone(), state);
        assert!(rv.is_none(), "output was already tracked");
        let rv = self.output_by_name.insert(name, output);
        assert!(rv.is_none(), "output was already tracked");
    }

    pub fn remove_output(&mut self, output: &Output) {
        self.layout.remove_output(output);
        self.global_space.unmap_output(output);
        // FIXME: reposition outputs so they are adjacent.

        let state = self.output_state.remove(output).unwrap();
        self.output_by_name.remove(&output.name()).unwrap();

        match state.redraw_state {
            RedrawState::Idle => (),
            RedrawState::Queued(idle) => idle.cancel(),
            RedrawState::WaitingForVBlank { .. } => (),
            RedrawState::WaitingForEstimatedVBlank(token) => self.event_loop.remove(token),
            RedrawState::WaitingForEstimatedVBlankAndQueued((token, idle)) => {
                self.event_loop.remove(token);
                idle.cancel();
            }
        }

        // Disable the output global and remove some time later to give the clients some time to
        // process it.
        let global = state.global;
        self.display_handle.disable_global::<State>(global.clone());
        self.event_loop
            .insert_source(
                Timer::from_duration(Duration::from_secs(10)),
                move |_, _, state| {
                    state
                        .niri
                        .display_handle
                        .remove_global::<State>(global.clone());
                    TimeoutAction::Drop
                },
            )
            .unwrap();

        match mem::take(&mut self.lock_state) {
            LockState::Locking(confirmation) => {
                // We're locking and an output was removed, check if the requirements are now met.
                let all_locked = self
                    .output_state
                    .values()
                    .all(|state| state.lock_render_state == LockRenderState::Locked);

                if all_locked {
                    confirmation.lock();
                    self.lock_state = LockState::Locked;
                } else {
                    // Still waiting.
                    self.lock_state = LockState::Locking(confirmation);
                }
            }
            lock_state => self.lock_state = lock_state,
        }

        if self.screenshot_ui.close() {
            self.cursor_manager
                .set_cursor_image(CursorImageStatus::default_named());
            self.queue_redraw_all();
        }
    }

    pub fn output_resized(&mut self, output: Output) {
        let output_size = output_size(&output);
        let is_locked = self.is_locked();

        layer_map_for_output(&output).arrange();
        self.layout.update_output_size(&output);

        if let Some(state) = self.output_state.get_mut(&output) {
            state.background_buffer.resize(output_size);

            state.lock_color_buffer.resize(output_size);
            if is_locked {
                if let Some(lock_surface) = &state.lock_surface {
                    configure_lock_surface(lock_surface, &output);
                }
            }
        }

        // If the output size changed with an open screenshot UI, close the screenshot UI.
        if let Some(old_size) = self.screenshot_ui.output_size(&output) {
            let output_transform = output.current_transform();
            let output_mode = output.current_mode().unwrap();
            let size = output_transform.transform_size(output_mode.size);
            if old_size != size {
                self.screenshot_ui.close();
                self.cursor_manager
                    .set_cursor_image(CursorImageStatus::default_named());
                self.queue_redraw_all();
                return;
            }
        }

        self.queue_redraw(output);
    }

    pub fn deactivate_monitors(&mut self, backend: &Backend) {
        if !self.monitors_active {
            return;
        }

        self.monitors_active = false;
        backend.set_monitors_active(false);
    }

    pub fn activate_monitors(&mut self, backend: &Backend) {
        if self.monitors_active {
            return;
        }

        self.monitors_active = true;
        backend.set_monitors_active(true);

        self.queue_redraw_all();
    }

    pub fn output_under(&self, pos: Point<f64, Logical>) -> Option<(&Output, Point<f64, Logical>)> {
        let output = self.global_space.output_under(pos).next()?;
        let pos_within_output = pos
            - self
                .global_space
                .output_geometry(output)
                .unwrap()
                .loc
                .to_f64();

        Some((output, pos_within_output))
    }

    pub fn window_under_cursor(&self) -> Option<&Window> {
        if self.is_locked() || self.screenshot_ui.is_open() {
            return None;
        }

        let pos = self.seat.get_pointer().unwrap().current_location();
        let (output, pos_within_output) = self.output_under(pos)?;
        let (window, _loc) = self.layout.window_under(output, pos_within_output)?;
        Some(window)
    }

    /// Returns the surface under cursor and its position in the global space.
    ///
    /// Pointer needs location in global space, and focused window location compatible with that
    /// global space. We don't have a global space for all windows, but this function converts the
    /// window location temporarily to the current global space.
    pub fn surface_under_and_global_space(
        &mut self,
        pos: Point<f64, Logical>,
    ) -> Option<PointerFocus> {
        let (output, pos_within_output) = self.output_under(pos)?;

        if self.is_locked() {
            let state = self.output_state.get(output)?;
            let surface = state.lock_surface.as_ref()?;
            // We put lock surfaces at (0, 0).
            let point = pos_within_output;
            let (surface, point) = under_from_surface_tree(
                surface.wl_surface(),
                point,
                (0, 0),
                WindowSurfaceType::ALL,
            )?;
            return Some(PointerFocus {
                output: output.clone(),
                surface: (surface, point),
            });
        }

        if self.screenshot_ui.is_open() {
            return None;
        }

        let layers = layer_map_for_output(output);
        let layer_surface_under = |layer| {
            layers
                .layer_under(layer, pos_within_output)
                .and_then(|layer| {
                    let layer_pos_within_output = layers.layer_geometry(layer).unwrap().loc;
                    layer
                        .surface_under(
                            pos_within_output - layer_pos_within_output.to_f64(),
                            WindowSurfaceType::ALL,
                        )
                        .map(|(surface, pos_within_layer)| {
                            (surface, pos_within_layer + layer_pos_within_output)
                        })
                })
        };

        let window_under = || {
            self.layout
                .window_under(output, pos_within_output)
                .and_then(|(window, win_pos_within_output)| {
                    window
                        .surface_under(
                            pos_within_output - win_pos_within_output.to_f64(),
                            WindowSurfaceType::ALL,
                        )
                        .map(|(s, pos_within_window)| {
                            (s, pos_within_window + win_pos_within_output)
                        })
                })
        };

        let mon = self.layout.monitor_for_output(output).unwrap();

        let mut under = layer_surface_under(Layer::Overlay);

        if mon.render_above_top_layer() {
            under = under
                .or_else(window_under)
                .or_else(|| layer_surface_under(Layer::Top));
        } else {
            under = under
                .or_else(|| layer_surface_under(Layer::Top))
                .or_else(window_under);
        }

        let (surface, surface_pos_within_output) = under
            .or_else(|| layer_surface_under(Layer::Bottom))
            .or_else(|| layer_surface_under(Layer::Background))?;

        let output_pos_in_global_space = self.global_space.output_geometry(output).unwrap().loc;
        let surface_loc_in_global_space = surface_pos_within_output + output_pos_in_global_space;

        Some(PointerFocus {
            output: output.clone(),
            surface: (surface, surface_loc_in_global_space),
        })
    }

    pub fn output_under_cursor(&self) -> Option<Output> {
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.global_space.output_under(pos).next().cloned()
    }

    pub fn output_left(&self) -> Option<Output> {
        let active = self.layout.active_output()?;
        let active_geo = self.global_space.output_geometry(active).unwrap();
        let extended_geo = Rectangle::from_loc_and_size(
            (i32::MIN / 2, active_geo.loc.y),
            (i32::MAX, active_geo.size.h),
        );

        self.global_space
            .outputs()
            .map(|output| (output, self.global_space.output_geometry(output).unwrap()))
            .filter(|(_, geo)| center(*geo).x < center(active_geo).x && geo.overlaps(extended_geo))
            .min_by_key(|(_, geo)| center(active_geo).x - center(*geo).x)
            .map(|(output, _)| output)
            .cloned()
    }

    pub fn output_right(&self) -> Option<Output> {
        let active = self.layout.active_output()?;
        let active_geo = self.global_space.output_geometry(active).unwrap();
        let extended_geo = Rectangle::from_loc_and_size(
            (i32::MIN / 2, active_geo.loc.y),
            (i32::MAX, active_geo.size.h),
        );

        self.global_space
            .outputs()
            .map(|output| (output, self.global_space.output_geometry(output).unwrap()))
            .filter(|(_, geo)| center(*geo).x > center(active_geo).x && geo.overlaps(extended_geo))
            .min_by_key(|(_, geo)| center(*geo).x - center(active_geo).x)
            .map(|(output, _)| output)
            .cloned()
    }

    pub fn output_up(&self) -> Option<Output> {
        let active = self.layout.active_output()?;
        let active_geo = self.global_space.output_geometry(active).unwrap();
        let extended_geo = Rectangle::from_loc_and_size(
            (active_geo.loc.x, i32::MIN / 2),
            (active_geo.size.w, i32::MAX),
        );

        self.global_space
            .outputs()
            .map(|output| (output, self.global_space.output_geometry(output).unwrap()))
            .filter(|(_, geo)| center(*geo).y < center(active_geo).y && geo.overlaps(extended_geo))
            .min_by_key(|(_, geo)| center(active_geo).y - center(*geo).y)
            .map(|(output, _)| output)
            .cloned()
    }

    pub fn output_down(&self) -> Option<Output> {
        let active = self.layout.active_output()?;
        let active_geo = self.global_space.output_geometry(active).unwrap();
        let extended_geo = Rectangle::from_loc_and_size(
            (active_geo.loc.x, i32::MIN / 2),
            (active_geo.size.w, i32::MAX),
        );

        self.global_space
            .outputs()
            .map(|output| (output, self.global_space.output_geometry(output).unwrap()))
            .filter(|(_, geo)| center(active_geo).y < center(*geo).y && geo.overlaps(extended_geo))
            .min_by_key(|(_, geo)| center(*geo).y - center(active_geo).y)
            .map(|(output, _)| output)
            .cloned()
    }

    pub fn output_for_tablet(&self) -> Option<&Output> {
        let config = self.config.borrow();
        let map_to_output = config.input.tablet.map_to_output.as_ref();
        map_to_output
            .and_then(|name| self.output_by_name.get(name))
            .or_else(|| self.global_space.outputs().next())
    }

    fn lock_surface_focus(&self) -> Option<WlSurface> {
        let output_under_cursor = self.output_under_cursor();
        let output = output_under_cursor
            .as_ref()
            .or_else(|| self.layout.active_output())
            .or_else(|| self.global_space.outputs().next())?;

        let state = self.output_state.get(output)?;
        state.lock_surface.as_ref().map(|s| s.wl_surface()).cloned()
    }

    fn layer_surface_focus(&self) -> Option<WlSurface> {
        let output = self.layout.active_output()?;
        let layers = layer_map_for_output(output);
        let surface = layers
            .layers_on(Layer::Overlay)
            .chain(layers.layers_on(Layer::Top))
            .find(|surface| surface.can_receive_keyboard_focus())?;

        Some(surface.wl_surface().clone())
    }

    /// Schedules an immediate redraw on all outputs if one is not already scheduled.
    pub fn queue_redraw_all(&mut self) {
        let outputs: Vec<_> = self.output_state.keys().cloned().collect();
        for output in outputs {
            self.queue_redraw(output);
        }
    }

    /// Schedules an immediate redraw if one is not already scheduled.
    pub fn queue_redraw(&mut self, output: Output) {
        let state = self.output_state.get_mut(&output).unwrap();
        let token = match mem::take(&mut state.redraw_state) {
            RedrawState::Idle => None,
            RedrawState::WaitingForEstimatedVBlank(token) => Some(token),

            // A redraw is already queued, put it back and do nothing.
            value @ (RedrawState::Queued(_)
            | RedrawState::WaitingForEstimatedVBlankAndQueued(_)) => {
                state.redraw_state = value;
                return;
            }

            // We're waiting for VBlank, request a redraw afterwards.
            RedrawState::WaitingForVBlank { .. } => {
                state.redraw_state = RedrawState::WaitingForVBlank {
                    redraw_needed: true,
                };
                return;
            }
        };

        let idle = self.event_loop.insert_idle(move |state| {
            state.niri.redraw(&mut state.backend, &output);
        });

        state.redraw_state = match token {
            Some(token) => RedrawState::WaitingForEstimatedVBlankAndQueued((token, idle)),
            None => RedrawState::Queued(idle),
        };
    }

    pub fn pointer_element(
        &self,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) -> Vec<OutputRenderElements<GlesRenderer>> {
        let _span = tracy_client::span!("Niri::pointer_element");
        let output_scale = output.current_scale();
        let output_pos = self.global_space.output_geometry(output).unwrap().loc;
        let pointer_pos = self.seat.get_pointer().unwrap().current_location() - output_pos.to_f64();

        // Get the render cursor to draw.
        let cursor_scale = output_scale.integer_scale();
        let render_cursor = self.cursor_manager.get_render_cursor(cursor_scale);

        let output_scale = Scale::from(output.current_scale().fractional_scale());

        let (mut pointer_elements, pointer_pos) = match render_cursor {
            RenderCursor::Hidden => (vec![], pointer_pos.to_physical_precise_round(output_scale)),
            RenderCursor::Surface { surface, hotspot } => {
                let pointer_pos =
                    (pointer_pos - hotspot.to_f64()).to_physical_precise_round(output_scale);

                let pointer_elements = render_elements_from_surface_tree(
                    renderer,
                    &surface,
                    pointer_pos,
                    output_scale,
                    1.,
                    Kind::Cursor,
                );

                (pointer_elements, pointer_pos)
            }
            RenderCursor::Named {
                icon,
                scale,
                cursor,
            } => {
                let (idx, frame) = cursor.frame(self.start_time.elapsed().as_millis() as u32);
                let hotspot = XCursor::hotspot(frame).to_logical(scale);
                let pointer_pos =
                    (pointer_pos - hotspot.to_f64()).to_physical_precise_round(output_scale);

                let texture = self
                    .cursor_texture_cache
                    .get(renderer, icon, scale, &cursor, idx);

                let pointer_elements = vec![OutputRenderElements::NamedPointer(
                    TextureRenderElement::from_texture_buffer(
                        pointer_pos.to_f64(),
                        &texture,
                        None,
                        None,
                        None,
                        Kind::Cursor,
                    ),
                )];

                (pointer_elements, pointer_pos)
            }
        };

        if let Some(dnd_icon) = &self.dnd_icon {
            pointer_elements.extend(render_elements_from_surface_tree(
                renderer,
                dnd_icon,
                pointer_pos,
                output_scale,
                1.,
                Kind::Unspecified,
            ));
        }

        pointer_elements
    }

    pub fn refresh_pointer_outputs(&mut self) {
        let _span = tracy_client::span!("Niri::refresh_pointer_outputs");

        match self.cursor_manager.cursor_image().clone() {
            CursorImageStatus::Surface(ref surface) => {
                let hotspot = with_states(surface, |states| {
                    states
                        .data_map
                        .get::<Mutex<CursorImageAttributes>>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .hotspot
                });

                let pointer_pos = self.seat.get_pointer().unwrap().current_location();
                let surface_pos = pointer_pos.to_i32_round() - hotspot;
                let bbox = bbox_from_surface_tree(surface, surface_pos);

                let dnd = self
                    .dnd_icon
                    .as_ref()
                    .map(|surface| (surface, bbox_from_surface_tree(surface, surface_pos)));

                // FIXME we basically need to pick the largest scale factor across the overlapping
                // outputs, this is how it's usually done in clients as well.
                let mut cursor_scale = 1;
                let mut dnd_scale = 1;
                for output in self.global_space.outputs() {
                    let geo = self.global_space.output_geometry(output).unwrap();

                    // Compute pointer surface overlap.
                    if let Some(mut overlap) = geo.intersection(bbox) {
                        overlap.loc -= surface_pos;
                        cursor_scale = cursor_scale.max(output.current_scale().integer_scale());
                        output_update(output, Some(overlap), surface);
                    } else {
                        output_update(output, None, surface);
                    }

                    // Compute DnD icon surface overlap.
                    if let Some((surface, bbox)) = dnd {
                        if let Some(mut overlap) = geo.intersection(bbox) {
                            overlap.loc -= surface_pos;
                            dnd_scale = dnd_scale.max(output.current_scale().integer_scale());
                            output_update(output, Some(overlap), surface);
                        } else {
                            output_update(output, None, surface);
                        }
                    }
                }

                with_states(surface, |data| {
                    send_surface_state(surface, data, cursor_scale, Transform::Normal);
                });
                if let Some((surface, _)) = dnd {
                    with_states(surface, |data| {
                        send_surface_state(surface, data, dnd_scale, Transform::Normal);
                    });
                }
            }
            cursor_image => {
                // There's no cursor surface, but there might be a DnD icon.
                let Some(surface) = &self.dnd_icon else {
                    return;
                };

                let icon = if let CursorImageStatus::Named(icon) = cursor_image {
                    icon
                } else {
                    Default::default()
                };

                let pointer_pos = self.seat.get_pointer().unwrap().current_location();

                let mut dnd_scale = 1;
                for output in self.global_space.outputs() {
                    let geo = self.global_space.output_geometry(output).unwrap();

                    // The default cursor is rendered at the right scale for each output, which
                    // means that it may have a different hotspot for each output.
                    let output_scale = output.current_scale().integer_scale();
                    let cursor = self
                        .cursor_manager
                        .get_cursor_with_name(icon, output_scale)
                        .unwrap_or_else(|| self.cursor_manager.get_default_cursor(output_scale));

                    // For simplicity, we always use frame 0 for this computation. Let's hope the
                    // hotspot doesn't change between frames.
                    let hotspot = XCursor::hotspot(&cursor.frames()[0]).to_logical(output_scale);

                    let surface_pos = pointer_pos.to_i32_round() - hotspot;
                    let bbox = bbox_from_surface_tree(surface, surface_pos);

                    if let Some(mut overlap) = geo.intersection(bbox) {
                        overlap.loc -= surface_pos;
                        dnd_scale = dnd_scale.max(output.current_scale().integer_scale());
                        output_update(output, Some(overlap), surface);
                    } else {
                        output_update(output, None, surface);
                    }

                    with_states(surface, |data| {
                        send_surface_state(surface, data, dnd_scale, Transform::Normal);
                    });
                }
            }
        }
    }

    fn render(
        &self,
        renderer: &mut GlesRenderer,
        output: &Output,
        include_pointer: bool,
    ) -> Vec<OutputRenderElements<GlesRenderer>> {
        let _span = tracy_client::span!("Niri::render");

        let output_scale = Scale::from(output.current_scale().fractional_scale());

        // The pointer goes on the top.
        let mut elements = vec![];
        if include_pointer {
            elements = self.pointer_element(renderer, output);
        }

        // If the session is locked, draw the lock surface.
        if self.is_locked() {
            let state = self.output_state.get(output).unwrap();
            if let Some(surface) = state.lock_surface.as_ref() {
                elements.extend(render_elements_from_surface_tree(
                    renderer,
                    surface.wl_surface(),
                    (0, 0),
                    output_scale,
                    1.,
                    Kind::Unspecified,
                ));
            }

            // Draw the solid color background.
            elements.push(
                SolidColorRenderElement::from_buffer(
                    &state.lock_color_buffer,
                    (0, 0),
                    output_scale,
                    1.,
                    Kind::Unspecified,
                )
                .into(),
            );

            return elements;
        }

        // Prepare the background element.
        let state = self.output_state.get(output).unwrap();
        let background = SolidColorRenderElement::from_buffer(
            &state.background_buffer,
            (0, 0),
            output_scale,
            1.,
            Kind::Unspecified,
        )
        .into();

        // If the screenshot UI is open, draw it.
        if self.screenshot_ui.is_open() {
            elements.extend(
                self.screenshot_ui
                    .render_output(output)
                    .into_iter()
                    .map(OutputRenderElements::from),
            );

            // Add the background for outputs that were connected while the screenshot UI was open.
            elements.push(background);

            return elements;
        }

        // Get monitor elements.
        let mon = self.layout.monitor_for_output(output).unwrap();
        let monitor_elements = mon.render_elements(renderer);

        // Get layer-shell elements.
        let layer_map = layer_map_for_output(output);
        let mut extend_from_layer = |elements: &mut Vec<OutputRenderElements<GlesRenderer>>,
                                     layer| {
            let iter = layer_map
                .layers_on(layer)
                .filter_map(|surface| {
                    layer_map
                        .layer_geometry(surface)
                        .map(|geo| (geo.loc, surface))
                })
                .flat_map(|(loc, surface)| {
                    surface
                        .render_elements(
                            renderer,
                            loc.to_physical_precise_round(output_scale),
                            output_scale,
                            1.,
                        )
                        .into_iter()
                        .map(OutputRenderElements::Wayland)
                });
            elements.extend(iter);
        };

        // The upper layer-shell elements go next.
        extend_from_layer(&mut elements, Layer::Overlay);

        // Then the regular monitor elements and the top layer in varying order.
        if mon.render_above_top_layer() {
            elements.extend(monitor_elements.into_iter().map(OutputRenderElements::from));
            extend_from_layer(&mut elements, Layer::Top);
        } else {
            extend_from_layer(&mut elements, Layer::Top);
            elements.extend(monitor_elements.into_iter().map(OutputRenderElements::from));
        }

        // Then the lower layer-shell elements.
        extend_from_layer(&mut elements, Layer::Bottom);
        extend_from_layer(&mut elements, Layer::Background);

        // Then the background.
        elements.push(background);

        elements
    }

    fn redraw(&mut self, backend: &mut Backend, output: &Output) {
        let _span = tracy_client::span!("Niri::redraw");

        let monitors_active = self.monitors_active;

        let state = self.output_state.get_mut(output).unwrap();
        assert!(matches!(
            state.redraw_state,
            RedrawState::Queued(_) | RedrawState::WaitingForEstimatedVBlankAndQueued(_)
        ));

        // FIXME: make this not cursed.
        let mut reset = || {
            let state = self.output_state.get_mut(output).unwrap();
            state.redraw_state =
                if let RedrawState::WaitingForEstimatedVBlankAndQueued((token, _)) =
                    state.redraw_state
                {
                    RedrawState::WaitingForEstimatedVBlank(token)
                } else {
                    RedrawState::Idle
                };

            if matches!(self.lock_state, LockState::Locking { .. })
                && state.lock_render_state == LockRenderState::Unlocked
            {
                // We needed to redraw this output for locking and failed.
                self.unlock();
            }
        };

        if !monitors_active {
            reset();
            return;
        }

        if !backend.is_active() {
            reset();
            return;
        }

        let Some(renderer) = backend.renderer() else {
            reset();
            return;
        };

        let state = self.output_state.get_mut(output).unwrap();
        let presentation_time = state.frame_clock.next_presentation_time();

        // Update from the config and advance the animations.
        self.layout.advance_animations(presentation_time);
        state.unfinished_animations_remain = self
            .layout
            .monitor_for_output(output)
            .unwrap()
            .are_animations_ongoing();

        // Also keep redrawing if the current cursor is animated.
        state.unfinished_animations_remain |= self
            .cursor_manager
            .is_current_cursor_animated(output.current_scale().integer_scale());

        // Render the elements.
        let elements = self.render(renderer, output, true);

        // Hand it over to the backend.
        let res = backend.render(self, output, &elements, presentation_time);

        // Update the lock render state on successful render.
        let is_locked = self.is_locked();
        let state = self.output_state.get_mut(output).unwrap();
        if res != RenderResult::Error {
            state.lock_render_state = if is_locked {
                LockRenderState::Locked
            } else {
                LockRenderState::Unlocked
            };
        }

        // If we're in process of locking the session, check if the requirements were met.
        match mem::take(&mut self.lock_state) {
            LockState::Locking(confirmation) => {
                if res == RenderResult::Error {
                    if state.lock_render_state == LockRenderState::Unlocked {
                        // We needed to render a locked frame on this output but failed.
                        self.unlock();
                    } else {
                        // Rendering failed but this output is already locked, so it's fine.
                        self.lock_state = LockState::Locking(confirmation);
                    }
                } else {
                    // Rendering succeeded, check if this was the last output.
                    let all_locked = self
                        .output_state
                        .values()
                        .all(|state| state.lock_render_state == LockRenderState::Locked);

                    if all_locked {
                        confirmation.lock();
                        self.lock_state = LockState::Locked;
                    } else {
                        // Still waiting.
                        self.lock_state = LockState::Locking(confirmation);
                    }
                }
            }
            lock_state => self.lock_state = lock_state,
        }

        // Send the frame callbacks.
        //
        // FIXME: The logic here could be a bit smarter. Currently, during an animation, the
        // surfaces that are visible for the very last frame (e.g. because the camera is moving
        // away) will receive frame callbacks, and the surfaces that are invisible but will become
        // visible next frame will not receive frame callbacks (so they will show stale contents for
        // one frame). We could advance the animations for the next frame and send frame callbacks
        // according to the expected new positions.
        //
        // However, this should probably be restricted to sending frame callbacks to more surfaces,
        // to err on the safe side.
        self.send_frame_callbacks(output);

        // Render and send to PipeWire screencast streams.
        #[cfg(feature = "xdp-gnome-screencast")]
        {
            let renderer = backend
                .renderer()
                .expect("renderer must not have disappeared");
            self.send_for_screen_cast(renderer, output, &elements, presentation_time);
        }
    }

    pub fn update_primary_scanout_output(
        &self,
        output: &Output,
        render_element_states: &RenderElementStates,
    ) {
        // FIXME: potentially tweak the compare function. The default one currently always prefers a
        // higher refresh-rate output, which is not always desirable (i.e. with a very small
        // overlap).
        //
        // While we only have cursors and DnD icons crossing output boundaries though, it doesn't
        // matter all that much.
        if let CursorImageStatus::Surface(surface) = &self.cursor_manager.cursor_image() {
            with_surface_tree_downward(
                surface,
                (),
                |_, _, _| TraversalAction::DoChildren(()),
                |surface, states, _| {
                    update_surface_primary_scanout_output(
                        surface,
                        output,
                        states,
                        render_element_states,
                        default_primary_scanout_output_compare,
                    );
                },
                |_, _, _| true,
            );
        }

        if let Some(surface) = &self.dnd_icon {
            with_surface_tree_downward(
                surface,
                (),
                |_, _, _| TraversalAction::DoChildren(()),
                |surface, states, _| {
                    update_surface_primary_scanout_output(
                        surface,
                        output,
                        states,
                        render_element_states,
                        default_primary_scanout_output_compare,
                    );
                },
                |_, _, _| true,
            );
        }

        // We're only updating the current output's windows and layer surfaces. This should be fine
        // as in niri they can only be rendered on a single output at a time.
        //
        // The reason to do this at all is that it keeps track of whether the surface is visible or
        // not in a unified way with the pointer surfaces, which makes the logic elsewhere simpler.

        for win in self.layout.windows_for_output(output) {
            win.with_surfaces(|surface, states| {
                update_surface_primary_scanout_output(
                    surface,
                    output,
                    states,
                    render_element_states,
                    // Windows are shown only on one output at a time.
                    |_, _, output, _| output,
                );
            });
        }

        for surface in layer_map_for_output(output).layers() {
            surface.with_surfaces(|surface, states| {
                update_surface_primary_scanout_output(
                    surface,
                    output,
                    states,
                    render_element_states,
                    // Layer surfaces are shown only on one output at a time.
                    |_, _, output, _| output,
                );
            });
        }

        if let Some(surface) = &self.output_state[output].lock_surface {
            with_surface_tree_downward(
                surface.wl_surface(),
                (),
                |_, _, _| TraversalAction::DoChildren(()),
                |surface, states, _| {
                    update_surface_primary_scanout_output(
                        surface,
                        output,
                        states,
                        render_element_states,
                        default_primary_scanout_output_compare,
                    );
                },
                |_, _, _| true,
            );
        }
    }

    pub fn send_dmabuf_feedbacks(&self, output: &Output, feedback: &DmabufFeedback) {
        let _span = tracy_client::span!("Niri::send_dmabuf_feedbacks");

        // We can unconditionally send the current output's feedback to regular and layer-shell
        // surfaces, as they can only be displayed on a single output at a time. Even if a surface
        // is currently invisible, this is the DMABUF feedback that it should know about.
        for win in self.layout.windows_for_output(output) {
            win.send_dmabuf_feedback(output, |_, _| Some(output.clone()), |_, _| feedback);
        }

        for surface in layer_map_for_output(output).layers() {
            surface.send_dmabuf_feedback(output, |_, _| Some(output.clone()), |_, _| feedback);
        }

        if let Some(surface) = &self.output_state[output].lock_surface {
            send_dmabuf_feedback_surface_tree(
                surface.wl_surface(),
                output,
                |_, _| Some(output.clone()),
                |_, _| feedback,
            );
        }

        if let Some(surface) = &self.dnd_icon {
            send_dmabuf_feedback_surface_tree(
                surface,
                output,
                surface_primary_scanout_output,
                |_, _| feedback,
            );
        }

        if let CursorImageStatus::Surface(surface) = &self.cursor_manager.cursor_image() {
            send_dmabuf_feedback_surface_tree(
                surface,
                output,
                surface_primary_scanout_output,
                |_, _| feedback,
            );
        }
    }

    pub fn send_frame_callbacks(&self, output: &Output) {
        let _span = tracy_client::span!("Niri::send_frame_callbacks");

        let state = self.output_state.get(output).unwrap();
        let sequence = state.current_estimated_sequence;

        let should_send = |surface: &WlSurface, states: &SurfaceData| {
            // Do the standard primary scanout output check. For pointer surfaces it deduplicates
            // the frame callbacks across potentially multiple outputs, and for regular windows and
            // layer-shell surfaces it avoids sending frame callbacks to invisible surfaces.
            let current_primary_output = surface_primary_scanout_output(surface, states);
            if current_primary_output.as_ref() != Some(output) {
                return None;
            }

            // Next, check the throttling status.
            let frame_throttling_state = states
                .data_map
                .get_or_insert(SurfaceFrameThrottlingState::default);
            let mut last_sent_at = frame_throttling_state.last_sent_at.borrow_mut();

            let mut send = true;

            // If we already sent a frame callback to this surface this output refresh
            // cycle, don't send one again to prevent empty-damage commit busy loops.
            if let Some((last_output, last_sequence)) = &*last_sent_at {
                if last_output == output && Some(*last_sequence) == sequence {
                    send = false;
                }
            }

            if send {
                if let Some(sequence) = sequence {
                    *last_sent_at = Some((output.clone(), sequence));
                }

                Some(output.clone())
            } else {
                None
            }
        };

        let frame_callback_time = get_monotonic_time();

        for win in self.layout.windows_for_output(output) {
            win.send_frame(output, frame_callback_time, None, should_send);
        }

        for surface in layer_map_for_output(output).layers() {
            surface.send_frame(output, frame_callback_time, None, should_send);
        }

        if let Some(surface) = &self.output_state[output].lock_surface {
            send_frames_surface_tree(
                surface.wl_surface(),
                output,
                frame_callback_time,
                None,
                should_send,
            );
        }

        if let Some(surface) = &self.dnd_icon {
            send_frames_surface_tree(surface, output, frame_callback_time, None, should_send);
        }

        if let CursorImageStatus::Surface(surface) = self.cursor_manager.cursor_image() {
            send_frames_surface_tree(surface, output, frame_callback_time, None, should_send);
        }
    }

    pub fn take_presentation_feedbacks(
        &mut self,
        output: &Output,
        render_element_states: &RenderElementStates,
    ) -> OutputPresentationFeedback {
        let mut feedback = OutputPresentationFeedback::new(output);

        if let CursorImageStatus::Surface(surface) = &self.cursor_manager.cursor_image() {
            take_presentation_feedback_surface_tree(
                surface,
                &mut feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        if let Some(surface) = &self.dnd_icon {
            take_presentation_feedback_surface_tree(
                surface,
                &mut feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        for win in self.layout.windows_for_output(output) {
            win.take_presentation_feedback(
                &mut feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            )
        }

        for surface in layer_map_for_output(output).layers() {
            surface.take_presentation_feedback(
                &mut feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        if let Some(surface) = &self.output_state[output].lock_surface {
            take_presentation_feedback_surface_tree(
                surface.wl_surface(),
                &mut feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        feedback
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    fn send_for_screen_cast(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
        presentation_time: Duration,
    ) {
        let _span = tracy_client::span!("Niri::send_for_screen_cast");

        let size = output.current_mode().unwrap().size;
        let scale = Scale::from(output.current_scale().fractional_scale());

        for cast in &mut self.casts {
            if !cast.is_active.get() {
                continue;
            }

            if &cast.output != output {
                continue;
            }

            let last = cast.last_frame_time;
            let min = cast.min_time_between_frames.get();
            if !last.is_zero() && presentation_time - last < min {
                trace!(
                    "skipping frame because it is too soon \
                     last={last:?} now={presentation_time:?} diff={:?} < min={min:?}",
                    presentation_time - last,
                );
                continue;
            }

            {
                let mut buffer = match cast.stream.dequeue_buffer() {
                    Some(buffer) => buffer,
                    None => {
                        warn!("no available buffer in pw stream, skipping frame");
                        continue;
                    }
                };

                let data = &mut buffer.datas_mut()[0];
                let fd = data.as_raw().fd as i32;
                let dmabuf = cast.dmabufs.borrow()[&fd].clone();

                // FIXME: Hidden / embedded / metadata cursor
                if let Err(err) = render_to_dmabuf(renderer, dmabuf, size, scale, elements) {
                    error!("error rendering to dmabuf: {err:?}");
                    continue;
                }

                let maxsize = data.as_raw().maxsize;
                let chunk = data.chunk_mut();
                *chunk.size_mut() = maxsize;
                *chunk.stride_mut() = maxsize as i32 / size.h;
            }

            cast.last_frame_time = presentation_time;
        }
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    fn stop_cast(&mut self, session_id: usize) {
        let _span = tracy_client::span!("Niri::stop_cast");

        debug!(session_id, "StopCast");

        for i in (0..self.casts.len()).rev() {
            let cast = &self.casts[i];
            if cast.session_id != session_id {
                continue;
            }

            let cast = self.casts.swap_remove(i);
            if let Err(err) = cast.stream.disconnect() {
                warn!("error disconnecting stream: {err:?}");
            }
        }

        let dbus = &self.dbus.as_ref().unwrap();
        let server = dbus.conn_screen_cast.as_ref().unwrap().object_server();
        let path = format!("/org/gnome/Mutter/ScreenCast/Session/u{}", session_id);
        if let Ok(iface) = server.interface::<_, mutter_screen_cast::Session>(path) {
            let _span = tracy_client::span!("invoking Session::stop");

            async_io::block_on(async move {
                iface
                    .get()
                    .stop(&server, iface.signal_context().clone())
                    .await
            });
        }
    }

    pub fn open_screenshot_ui(&mut self, renderer: &mut GlesRenderer) {
        if self.is_locked() || self.screenshot_ui.is_open() {
            return;
        }

        let Some(default_output) = self.output_under_cursor() else {
            return;
        };

        let screenshots = self
            .global_space
            .outputs()
            .cloned()
            .filter_map(|output| {
                let size = output.current_mode().unwrap().size;
                let scale = Scale::from(output.current_scale().fractional_scale());
                let elements = self.render(renderer, &output, true);

                let res = render_to_texture(renderer, size, scale, Fourcc::Abgr8888, &elements);
                let screenshot = match res {
                    Ok((texture, _)) => texture,
                    Err(err) => {
                        warn!("error rendering output {}: {err:?}", output.name());
                        return None;
                    }
                };

                Some((output, screenshot))
            })
            .collect();

        self.screenshot_ui
            .open(renderer, screenshots, default_output);
        self.cursor_manager
            .set_cursor_image(CursorImageStatus::Named(CursorIcon::Crosshair));
        self.queue_redraw_all();
    }

    pub fn screenshot(&self, renderer: &mut GlesRenderer, output: &Output) -> anyhow::Result<()> {
        let _span = tracy_client::span!("Niri::screenshot");

        let size = output.current_mode().unwrap().size;
        let scale = Scale::from(output.current_scale().fractional_scale());
        let elements = self.render(renderer, output, true);
        let pixels = render_to_vec(renderer, size, scale, Fourcc::Abgr8888, &elements)?;

        self.save_screenshot(size, pixels)
            .context("error saving screenshot")
    }

    pub fn screenshot_window(
        &self,
        renderer: &mut GlesRenderer,
        output: &Output,
        window: &Window,
    ) -> anyhow::Result<()> {
        let _span = tracy_client::span!("Niri::screenshot_window");

        let scale = Scale::from(output.current_scale().fractional_scale());
        let bbox = window.bbox_with_popups();
        let size = bbox.size.to_physical_precise_ceil(scale);
        let buf_pos = Point::from((0, 0)) - bbox.loc;
        // FIXME: pointer.
        let elements = window.render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
            renderer,
            buf_pos.to_physical_precise_ceil(scale),
            scale,
            1.,
        );
        let pixels = render_to_vec(renderer, size, scale, Fourcc::Abgr8888, &elements)?;

        self.save_screenshot(size, pixels)
            .context("error saving screenshot")
    }

    pub fn save_screenshot(
        &self,
        size: Size<i32, Physical>,
        pixels: Vec<u8>,
    ) -> anyhow::Result<()> {
        let path = match make_screenshot_path(&self.config.borrow()) {
            Ok(path) => path,
            Err(err) => {
                warn!("error making screenshot path: {err:?}");
                None
            }
        };

        // Prepare to set the encoded image as our clipboard selection. This must be done from the
        // main thread.
        let (tx, rx) = calloop::channel::sync_channel::<Arc<[u8]>>(1);
        self.event_loop
            .insert_source(rx, move |event, _, state| match event {
                calloop::channel::Event::Msg(buf) => {
                    set_data_device_selection(
                        &state.niri.display_handle,
                        &state.niri.seat,
                        vec![String::from("image/png")],
                        buf.clone(),
                    );
                    set_primary_selection(
                        &state.niri.display_handle,
                        &state.niri.seat,
                        vec![String::from("image/png")],
                        buf.clone(),
                    );
                }
                calloop::channel::Event::Closed => (),
            })
            .unwrap();

        // Encode and save the image in a thread as it's slow.
        thread::spawn(move || {
            let mut buf = vec![];

            let w = std::io::Cursor::new(&mut buf);
            if let Err(err) = write_png_rgba8(w, size.w as u32, size.h as u32, &pixels) {
                warn!("error encoding screenshot image: {err:?}");
                return;
            }

            let buf: Arc<[u8]> = Arc::from(buf.into_boxed_slice());
            let _ = tx.send(buf.clone());

            let mut image_path = None;

            if let Some(path) = path {
                debug!("saving screenshot to {path:?}");

                match std::fs::write(&path, buf) {
                    Ok(()) => image_path = Some(path),
                    Err(err) => {
                        warn!("error saving screenshot image: {err:?}");
                    }
                }
            } else {
                debug!("not saving screenshot to disk");
            }

            #[cfg(feature = "dbus")]
            crate::utils::show_screenshot_notification(image_path);
            #[cfg(not(feature = "dbus"))]
            drop(image_path);
        });

        Ok(())
    }

    #[cfg(feature = "dbus")]
    pub fn screenshot_all_outputs(
        &self,
        renderer: &mut GlesRenderer,
        include_pointer: bool,
        on_done: impl FnOnce(PathBuf) + Send + 'static,
    ) -> anyhow::Result<()> {
        use std::cmp::max;

        use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};

        let _span = tracy_client::span!("Niri::screenshot_all_outputs");

        let mut elements = vec![];
        let mut size = Size::from((0, 0));

        let outputs: Vec<_> = self.global_space.outputs().cloned().collect();
        for output in outputs {
            let geom = self.global_space.output_geometry(&output).unwrap();
            // FIXME: this does not work when outputs can have non-1 scale.
            let geom = geom.to_physical(1);

            size.w = max(size.w, geom.loc.x + geom.size.w);
            size.h = max(size.h, geom.loc.y + geom.size.h);

            let output_elements = self.render(renderer, &output, include_pointer);
            elements.extend(output_elements.into_iter().map(|elem| {
                RelocateRenderElement::from_element(elem, geom.loc, Relocate::Relative)
            }));
        }

        // FIXME: scale.
        let pixels = render_to_vec(renderer, size, Scale::from(1.), Fourcc::Abgr8888, &elements)?;

        let path = make_screenshot_path(&self.config.borrow())
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                let mut path = env::temp_dir();
                path.push("screenshot.png");
                path
            });
        debug!("saving screenshot to {path:?}");

        thread::spawn(move || {
            let file = match std::fs::File::create(&path) {
                Ok(file) => file,
                Err(err) => {
                    warn!("error creating file: {err:?}");
                    return;
                }
            };

            let w = std::io::BufWriter::new(file);
            if let Err(err) = write_png_rgba8(w, size.w as u32, size.h as u32, &pixels) {
                warn!("error encoding screenshot image: {err:?}");
                return;
            }

            on_done(path);
        });

        Ok(())
    }

    pub fn is_locked(&self) -> bool {
        !matches!(self.lock_state, LockState::Unlocked)
    }

    pub fn lock(&mut self, confirmation: SessionLocker) {
        info!("locking session");

        self.screenshot_ui.close();
        self.cursor_manager
            .set_cursor_image(CursorImageStatus::default_named());

        self.lock_state = LockState::Locking(confirmation);
        self.queue_redraw_all();
    }

    pub fn unlock(&mut self) {
        info!("unlocking session");

        self.lock_state = LockState::Unlocked;
        for output_state in self.output_state.values_mut() {
            output_state.lock_surface = None;
        }
        self.queue_redraw_all();
    }

    pub fn new_lock_surface(&mut self, surface: LockSurface, output: &Output) {
        if !self.is_locked() {
            error!("tried to add a lock surface on an unlocked session");
            return;
        }

        let Some(output_state) = self.output_state.get_mut(output) else {
            error!("missing output state");
            return;
        };

        output_state.lock_surface = Some(surface);
    }
}

render_elements! {
    #[derive(Debug)]
    pub OutputRenderElements<R> where R: ImportAll;
    Monitor = MonitorRenderElement<R>,
    Wayland = WaylandSurfaceRenderElement<R>,
    NamedPointer = TextureRenderElement<<R as Renderer>::TextureId>,
    SolidColor = SolidColorRenderElement,
    ScreenshotUi = ScreenshotUiRenderElement<R>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

fn render_to_texture(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    fourcc: Fourcc,
    elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<(GlesTexture, SyncPoint)> {
    let _span = tracy_client::span!("render_to_texture");

    let output_rect = Rectangle::from_loc_and_size((0, 0), size);
    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);

    let texture: GlesTexture = renderer
        .create_buffer(fourcc, buffer_size)
        .context("error creating texture")?;

    renderer
        .bind(texture.clone())
        .context("error binding texture")?;

    let mut frame = renderer
        .render(size, Transform::Normal)
        .context("error starting frame")?;

    for element in elements.iter().rev() {
        let src = element.src();
        let dst = element.geometry(scale);
        element
            .draw(&mut frame, src, dst, &[output_rect])
            .context("error drawing element")?;
    }

    let sync_point = frame.finish().context("error finishing frame")?;
    Ok((texture, sync_point))
}

fn render_and_download(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    fourcc: Fourcc,
    elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<GlesMapping> {
    let _span = tracy_client::span!("render_and_download");

    let (_, sync_point) = render_to_texture(renderer, size, scale, fourcc, elements)?;
    sync_point.wait();

    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);
    let mapping = renderer
        .copy_framebuffer(Rectangle::from_loc_and_size((0, 0), buffer_size), fourcc)
        .context("error copying framebuffer")?;
    Ok(mapping)
}

fn render_to_vec(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    fourcc: Fourcc,
    elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<Vec<u8>> {
    let _span = tracy_client::span!("render_to_vec");

    let mapping =
        render_and_download(renderer, size, scale, fourcc, elements).context("error rendering")?;
    let copy = renderer
        .map_texture(&mapping)
        .context("error mapping texture")?;
    Ok(copy.to_vec())
}

#[cfg(feature = "xdp-gnome-screencast")]
fn render_to_dmabuf(
    renderer: &mut GlesRenderer,
    dmabuf: smithay::backend::allocator::dmabuf::Dmabuf,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    elements: &[OutputRenderElements<GlesRenderer>],
) -> anyhow::Result<()> {
    use smithay::backend::renderer::element::Element;

    let _span = tracy_client::span!("render_to_dmabuf");

    let output_rect = Rectangle::from_loc_and_size((0, 0), size);

    renderer.bind(dmabuf).context("error binding texture")?;
    let mut frame = renderer
        .render(size, Transform::Normal)
        .context("error starting frame")?;

    for element in elements.iter().rev() {
        let src = element.src();
        let dst = element.geometry(scale);
        element
            .draw(&mut frame, src, dst, &[output_rect])
            .context("error drawing element")?;
    }

    let _sync_point = frame.finish().context("error finishing frame")?;

    Ok(())
}
