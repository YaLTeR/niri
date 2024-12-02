use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{env, mem, thread};

use _server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDecorationsMode;
use anyhow::{bail, ensure, Context};
use calloop::futures::Scheduler;
use niri_config::{
    Config, FloatOrInt, Key, Modifiers, OutputName, PreviewRender, TrackLayout, WorkspaceReference,
    DEFAULT_BACKGROUND_COLOR,
};
use smithay::backend::allocator::Fourcc;
use smithay::backend::input::Keycode;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::utils::{
    select_dmabuf_feedback, Relocate, RelocateRenderElement,
};
use smithay::backend::renderer::element::{
    default_primary_scanout_output_compare, Element as _, Id, Kind, PrimaryScanoutOutput,
    RenderElementStates,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{Color32F, Unbind};
use smithay::desktop::utils::{
    bbox_from_surface_tree, output_update, send_dmabuf_feedback_surface_tree,
    send_frames_surface_tree, surface_presentation_feedback_flags_from_states,
    surface_primary_scanout_output, take_presentation_feedback_surface_tree,
    under_from_surface_tree, update_surface_primary_scanout_output, OutputPresentationFeedback,
};
use smithay::desktop::{
    layer_map_for_output, LayerMap, LayerSurface, PopupGrab, PopupManager, PopupUngrabStrategy,
    Space, Window, WindowSurfaceType,
};
use smithay::input::keyboard::Layout as KeyboardLayout;
use smithay::input::pointer::{CursorIcon, CursorImageAttributes, CursorImageStatus, MotionEvent};
use smithay::input::{Seat, SeatState};
use smithay::output::{self, Output, OutputModeSource, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{
    Interest, LoopHandle, LoopSignal, Mode, PostAction, RegistrationToken,
};
use smithay::reexports::wayland_protocols::ext::session_lock::v1::server::ext_session_lock_v1::ExtSessionLockV1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities;
use smithay::reexports::wayland_protocols_misc::server_decoration as _server_decoration;
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;
use smithay::reexports::wayland_server::backend::{
    ClientData, ClientId, DisconnectReason, GlobalId,
};
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, Display, DisplayHandle, Resource};
use smithay::utils::{
    ClockSource, IsAlive as _, Logical, Monotonic, Physical, Point, Rectangle, Scale, Size,
    Transform, SERIAL_COUNTER,
};
use smithay::wayland::compositor::{
    with_states, with_surface_tree_downward, CompositorClientState, CompositorHandler,
    CompositorState, HookId, SurfaceData, TraversalAction,
};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::DmabufState;
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::idle_inhibit::IdleInhibitManagerState;
use smithay::wayland::idle_notify::IdleNotifierState;
use smithay::wayland::input_method::{InputMethodManagerState, InputMethodSeat};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_constraints::{with_pointer_constraint, PointerConstraintsState};
use smithay::wayland::pointer_gestures::PointerGesturesState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::security_context::SecurityContextState;
use smithay::wayland::selection::data_device::{set_data_device_selection, DataDeviceState};
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState;
use smithay::wayland::session_lock::{LockSurface, SessionLockManagerState, SessionLocker};
use smithay::wayland::shell::kde::decoration::KdeDecorationState;
use smithay::wayland::shell::wlr_layer::{self, Layer, WlrLayerShellState};
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::tablet_manager::TabletManagerState;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::wayland::xdg_foreign::XdgForeignState;

use crate::animation::Clock;
use crate::backend::tty::SurfaceDmabufFeedback;
use crate::backend::{Backend, RenderResult, Tty, Winit};
use crate::cursor::{CursorManager, CursorTextureCache, RenderCursor, XCursor};
#[cfg(feature = "dbus")]
use crate::dbus::gnome_shell_introspect::{self, IntrospectToNiri, NiriToIntrospect};
#[cfg(feature = "dbus")]
use crate::dbus::gnome_shell_screenshot::{NiriToScreenshot, ScreenshotToNiri};
#[cfg(feature = "xdp-gnome-screencast")]
use crate::dbus::mutter_screen_cast::{self, ScreenCastToNiri};
use crate::frame_clock::FrameClock;
use crate::handlers::{configure_lock_surface, XDG_ACTIVATION_TOKEN_TIMEOUT};
use crate::input::scroll_tracker::ScrollTracker;
use crate::input::{
    apply_libinput_settings, mods_with_finger_scroll_binds, mods_with_wheel_binds, TabletData,
};
use crate::ipc::server::IpcServer;
use crate::layer::mapped::LayerSurfaceRenderElement;
use crate::layer::MappedLayer;
use crate::layout::tile::TileRenderElement;
use crate::layout::workspace::WorkspaceId;
use crate::layout::{Layout, LayoutElement as _, MonitorRenderElement};
use crate::niri_render_elements;
use crate::protocols::foreign_toplevel::{self, ForeignToplevelManagerState};
use crate::protocols::gamma_control::GammaControlManagerState;
use crate::protocols::mutter_x11_interop::MutterX11InteropManagerState;
use crate::protocols::output_management::OutputManagementManagerState;
use crate::protocols::screencopy::{Screencopy, ScreencopyBuffer, ScreencopyManagerState};
use crate::pw_utils::{Cast, PipeWire};
#[cfg(feature = "xdp-gnome-screencast")]
use crate::pw_utils::{CastSizeChange, CastTarget, PwToNiri};
use crate::render_helpers::debug::draw_opaque_regions;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::texture::TextureBuffer;
use crate::render_helpers::{
    render_to_dmabuf, render_to_encompassing_texture, render_to_shm, render_to_texture,
    render_to_vec, shaders, RenderTarget,
};
use crate::ui::config_error_notification::ConfigErrorNotification;
use crate::ui::exit_confirm_dialog::ExitConfirmDialog;
use crate::ui::hotkey_overlay::HotkeyOverlay;
use crate::ui::screen_transition::{self, ScreenTransition};
use crate::ui::screenshot_ui::{OutputScreenshot, ScreenshotUi, ScreenshotUiRenderElement};
use crate::utils::scale::{closest_representable_scale, guess_monitor_scale};
use crate::utils::spawning::CHILD_ENV;
use crate::utils::{
    center, center_f64, get_monotonic_time, ipc_transform_to_smithay, logical_output,
    make_screenshot_path, output_matches_name, output_size, send_scale_transform, write_png_rgba8,
};
use crate::window::{InitialConfigureState, Mapped, ResolvedWindowRules, Unmapped, WindowRef};

const CLEAR_COLOR_LOCKED: [f32; 4] = [0., 0., 0., 1.];

// We'll try to send frame callbacks at least once a second. We'll make a timer that fires once a
// second, so with the worst timing the maximum interval between two frame callbacks for a surface
// should be ~1.995 seconds.
const FRAME_CALLBACK_THROTTLE: Option<Duration> = Some(Duration::from_millis(995));

pub struct Niri {
    pub config: Rc<RefCell<Config>>,

    /// Output config from the config file.
    ///
    /// This does not include transient output config changes done via IPC. It is only used when
    /// reloading the config from disk to determine if the output configuration should be reloaded
    /// (and transient changes dropped).
    pub config_file_output_config: niri_config::Outputs,

    pub event_loop: LoopHandle<'static, State>,
    pub scheduler: Scheduler<()>,
    pub stop_signal: LoopSignal,
    pub display_handle: DisplayHandle,
    pub socket_name: OsString,

    pub start_time: Instant,

    /// Whether the at-startup=true window rules are active.
    pub is_at_startup: bool,

    /// Clock for driving animations.
    pub clock: Clock,

    // Each workspace corresponds to a Space. Each workspace generally has one Output mapped to it,
    // however it may have none (when there are no outputs connected) or multiple (when mirroring).
    pub layout: Layout<Mapped>,

    // This space does not actually contain any windows, but all outputs are mapped into it
    // according to their global position.
    pub global_space: Space<Window>,

    // Windows which don't have a buffer attached yet.
    pub unmapped_windows: HashMap<WlSurface, Unmapped>,

    /// Layer surfaces which don't have a buffer attached yet.
    pub unmapped_layer_surfaces: HashSet<WlSurface>,

    /// Extra data for mapped layer surfaces.
    pub mapped_layer_surfaces: HashMap<LayerSurface, MappedLayer>,

    // Cached root surface for every surface, so that we can access it in destroyed() where the
    // normal get_parent() is cleared out.
    pub root_surface: HashMap<WlSurface, WlSurface>,

    // Dmabuf readiness pre-commit hook for a surface.
    pub dmabuf_pre_commit_hook: HashMap<WlSurface, HookId>,

    /// Clients to notify about their blockers being cleared.
    pub blocker_cleared_tx: Sender<Client>,
    pub blocker_cleared_rx: Receiver<Client>,

    pub output_state: HashMap<Output, OutputState>,

    // When false, we're idling with monitors powered off.
    pub monitors_active: bool,

    /// Whether the laptop lid is closed.
    ///
    /// Libinput guarantees that the lid switch starts in open state, and if it was closed during
    /// startup, libinput will immediately send a closed event.
    pub is_lid_closed: bool,

    pub devices: HashSet<input::Device>,
    pub tablets: HashMap<input::Device, TabletData>,
    pub touch: HashSet<input::Device>,

    // Smithay state.
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub kde_decoration_state: KdeDecorationState,
    pub layer_shell_state: WlrLayerShellState,
    pub session_lock_state: SessionLockManagerState,
    pub foreign_toplevel_state: ForeignToplevelManagerState,
    pub screencopy_state: ScreencopyManagerState,
    pub output_management_state: OutputManagementManagerState,
    pub viewporter_state: ViewporterState,
    pub xdg_foreign_state: XdgForeignState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub dmabuf_state: DmabufState,
    pub fractional_scale_manager_state: FractionalScaleManagerState,
    pub seat_state: SeatState<State>,
    pub tablet_state: TabletManagerState,
    pub text_input_state: TextInputManagerState,
    pub input_method_state: InputMethodManagerState,
    pub virtual_keyboard_state: VirtualKeyboardManagerState,
    pub pointer_gestures_state: PointerGesturesState,
    pub relative_pointer_state: RelativePointerManagerState,
    pub pointer_constraints_state: PointerConstraintsState,
    pub idle_notifier_state: IdleNotifierState<State>,
    pub idle_inhibit_manager_state: IdleInhibitManagerState,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub data_control_state: DataControlState,
    pub popups: PopupManager,
    pub popup_grab: Option<PopupGrabState>,
    pub presentation_state: PresentationState,
    pub security_context_state: SecurityContextState,
    pub gamma_control_manager_state: GammaControlManagerState,
    pub activation_state: XdgActivationState,
    pub mutter_x11_interop_state: MutterX11InteropManagerState,

    pub seat: Seat<State>,
    /// Scancodes of the keys to suppress.
    pub suppressed_keys: HashSet<Keycode>,
    pub bind_cooldown_timers: HashMap<Key, RegistrationToken>,
    pub bind_repeat_timer: Option<RegistrationToken>,
    pub keyboard_focus: KeyboardFocus,
    pub layer_shell_on_demand_focus: Option<LayerSurface>,
    pub previously_focused_window: Option<Window>,
    pub idle_inhibiting_surfaces: HashSet<WlSurface>,
    pub is_fdo_idle_inhibited: Arc<AtomicBool>,

    pub cursor_manager: CursorManager,
    pub cursor_texture_cache: CursorTextureCache,
    pub cursor_shape_manager_state: CursorShapeManagerState,
    pub dnd_icon: Option<DndIcon>,
    /// Contents under pointer.
    ///
    /// Periodically updated: on motion and other events and in the loop callback. If you require
    /// the real up-to-date contents somewhere, it's better to recompute on the spot.
    ///
    /// This is not pointer focus. I.e. during a click grab, the pointer focus remains on the
    /// client with the grab, but this field will keep updating to the latest contents as if no
    /// grab was active.
    ///
    /// This is primarily useful for emitting pointer motion events for surfaces that move
    /// underneath the cursor on their own (i.e. when the tiling layout moves). In this case, not
    /// taking grabs into account is expected, because we pass the information to pointer.motion()
    /// which passes it down through grabs, which decide what to do with it as they see fit.
    pub pointer_contents: PointContents,
    /// Whether the pointer is hidden, for example due to a previous touch input.
    ///
    /// When this happens, the pointer also loses any focus. This is so that touch can prevent
    /// various tooltips from sticking around.
    pub pointer_hidden: bool,
    pub pointer_inactivity_timer: Option<RegistrationToken>,
    pub tablet_cursor_location: Option<Point<f64, Logical>>,
    pub gesture_swipe_3f_cumulative: Option<(f64, f64)>,
    pub vertical_wheel_tracker: ScrollTracker,
    pub horizontal_wheel_tracker: ScrollTracker,
    pub mods_with_wheel_binds: HashSet<Modifiers>,
    pub vertical_finger_scroll_tracker: ScrollTracker,
    pub horizontal_finger_scroll_tracker: ScrollTracker,
    pub mods_with_finger_scroll_binds: HashSet<Modifiers>,

    pub lock_state: LockState,

    pub screenshot_ui: ScreenshotUi,
    pub config_error_notification: ConfigErrorNotification,
    pub hotkey_overlay: HotkeyOverlay,
    pub exit_confirm_dialog: Option<ExitConfirmDialog>,

    pub debug_draw_opaque_regions: bool,
    pub debug_draw_damage: bool,

    #[cfg(feature = "dbus")]
    pub dbus: Option<crate::dbus::DBusServers>,
    #[cfg(feature = "dbus")]
    pub inhibit_power_key_fd: Option<zbus::zvariant::OwnedFd>,

    pub ipc_server: Option<IpcServer>,
    pub ipc_outputs_changed: bool,

    // Casts are dropped before PipeWire to prevent a double-free (yay).
    pub casts: Vec<Cast>,
    pub pipewire: Option<PipeWire>,

    // Screencast output for each mapped window.
    #[cfg(feature = "xdp-gnome-screencast")]
    pub mapped_cast_output: HashMap<Window, Output>,
}

#[derive(Debug)]
pub struct DndIcon {
    pub surface: WlSurface,
    pub offset: Point<i32, Logical>,
}

pub struct OutputState {
    pub global: GlobalId,
    pub frame_clock: FrameClock,
    pub redraw_state: RedrawState,
    pub on_demand_vrr_enabled: bool,
    // After the last redraw, some ongoing animations still remain.
    pub unfinished_animations_remain: bool,
    /// Last sequence received in a vblank event.
    pub last_drm_sequence: Option<u32>,
    /// Sequence for frame callback throttling.
    ///
    /// We want to send frame callbacks for each surface at most once per monitor refresh cycle.
    ///
    /// Even if a surface commit resulted in empty damage to the monitor, we want to delay the next
    /// frame callback until roughly when a VBlank would occur, had the monitor been damaged. This
    /// is necessary to prevent clients busy-looping with frame callbacks that result in empty
    /// damage.
    ///
    /// This counter wrapping-increments by 1 every time we move into the next refresh cycle, as
    /// far as frame callback throttling is concerned. Specifically, it happens:
    ///
    /// 1. Upon a successful DRM frame submission. Notably, we don't wait for the VBlank here,
    ///    because the client buffers are already "latched" at the point of submission. Even if a
    ///    client submits a new buffer right away, we will wait for a VBlank to draw it, which
    ///    means that busy looping is avoided.
    /// 2. If a frame resulted in empty damage, a timer is queued to fire roughly when a VBlank
    ///    would occur, based on the last presentation time and output refresh interval. Sequence
    ///    is incremented in that timer, before attempting a redraw or sending frame callbacks.
    pub frame_callback_sequence: u32,
    /// Solid color buffer for the background that we use instead of clearing to avoid damage
    /// tracking issues and make screenshots easier.
    pub background_buffer: SolidColorBuffer,
    pub lock_render_state: LockRenderState,
    pub lock_surface: Option<LockSurface>,
    pub lock_color_buffer: SolidColorBuffer,
    screen_transition: Option<ScreenTransition>,
    /// Damage tracker used for the debug damage visualization.
    pub debug_damage_tracker: OutputDamageTracker,
}

#[derive(Debug, Default)]
pub enum RedrawState {
    /// The compositor is idle.
    #[default]
    Idle,
    /// A redraw is queued.
    Queued,
    /// We submitted a frame to the KMS and waiting for it to be presented.
    WaitingForVBlank { redraw_needed: bool },
    /// We did not submit anything to KMS and made a timer to fire at the estimated VBlank.
    WaitingForEstimatedVBlank(RegistrationToken),
    /// A redraw is queued on top of the above.
    WaitingForEstimatedVBlankAndQueued(RegistrationToken),
}

pub struct PopupGrabState {
    pub root: WlSurface,
    pub grab: PopupGrab<State>,
}

// The surfaces here are always toplevel surfaces focused as far as niri's logic is concerned, even
// when popup grabs are active (which means the real keyboard focus is on a popup descending from
// that toplevel surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardFocus {
    // Layout is focused by default if there's nothing else to focus.
    Layout { surface: Option<WlSurface> },
    LayerShell { surface: WlSurface },
    LockScreen { surface: Option<WlSurface> },
    ScreenshotUi,
}

#[derive(Default, Clone, PartialEq)]
pub struct PointContents {
    // Output under point.
    pub output: Option<Output>,
    // Surface under point and its location in the global coordinate space.
    pub surface: Option<(WlSurface, Point<f64, Logical>)>,
    // If surface belongs to a window, this is that window.
    pub window: Option<Window>,
    // If surface belongs to a layer surface, this is that layer surface.
    pub layer: Option<LayerSurface>,
}

#[derive(Default)]
pub enum LockState {
    #[default]
    Unlocked,
    Locking(SessionLocker),
    Locked(ExtSessionLockV1),
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

pub enum CenterCoords {
    Separately,
    Both,
}

#[derive(Default)]
pub struct WindowOffscreenId(pub RefCell<Option<Id>>);

impl RedrawState {
    fn queue_redraw(self) -> Self {
        match self {
            RedrawState::Idle => RedrawState::Queued,
            RedrawState::WaitingForEstimatedVBlank(token) => {
                RedrawState::WaitingForEstimatedVBlankAndQueued(token)
            }

            // A redraw is already queued.
            value @ (RedrawState::Queued | RedrawState::WaitingForEstimatedVBlankAndQueued(_)) => {
                value
            }

            // We're waiting for VBlank, request a redraw afterwards.
            RedrawState::WaitingForVBlank { .. } => RedrawState::WaitingForVBlank {
                redraw_needed: true,
            },
        }
    }
}

impl Default for SurfaceFrameThrottlingState {
    fn default() -> Self {
        Self {
            last_sent_at: RefCell::new(None),
        }
    }
}

impl KeyboardFocus {
    pub fn surface(&self) -> Option<&WlSurface> {
        match self {
            KeyboardFocus::Layout { surface } => surface.as_ref(),
            KeyboardFocus::LayerShell { surface } => Some(surface),
            KeyboardFocus::LockScreen { surface } => surface.as_ref(),
            KeyboardFocus::ScreenshotUi => None,
        }
    }

    pub fn into_surface(self) -> Option<WlSurface> {
        match self {
            KeyboardFocus::Layout { surface } => surface,
            KeyboardFocus::LayerShell { surface } => Some(surface),
            KeyboardFocus::LockScreen { surface } => surface,
            KeyboardFocus::ScreenshotUi => None,
        }
    }

    pub fn is_layout(&self) -> bool {
        matches!(self, KeyboardFocus::Layout { .. })
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
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let _span = tracy_client::span!("State::new");

        let config = Rc::new(RefCell::new(config));

        let has_display =
            env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("DISPLAY").is_some();

        let mut backend = if has_display {
            let winit = Winit::new(config.clone(), event_loop.clone())?;
            Backend::Winit(winit)
        } else {
            let tty = Tty::new(config.clone(), event_loop.clone())
                .context("error initializing the TTY backend")?;
            Backend::Tty(tty)
        };

        let mut niri = Niri::new(config.clone(), event_loop, stop_signal, display, &backend);
        backend.init(&mut niri);

        let mut state = Self { backend, niri };

        // Initialize some IPC server state.
        state.ipc_keyboard_layouts_changed();

        Ok(state)
    }

    pub fn refresh_and_flush_clients(&mut self) {
        let _span = tracy_client::span!("State::refresh_and_flush_clients");

        self.refresh();

        // Advance animations to the current time (not target render time) before rendering outputs
        // in order to clear completed animations and render elements. Even if we're not rendering,
        // it's good to advance every now and then so the workspace clean-up and animations don't
        // build up (the 1 second frame callback timer will call this line).
        self.niri.advance_animations();

        self.niri.redraw_queued_outputs(&mut self.backend);

        {
            let _span = tracy_client::span!("flush_clients");
            self.niri.display_handle.flush_clients().unwrap();
        }

        // Clear the time so it's fetched afresh next iteration.
        self.niri.clock.clear();
    }

    fn refresh(&mut self) {
        let _span = tracy_client::span!("State::refresh");

        // Handle commits for surfaces whose blockers cleared this cycle. This should happen before
        // layout.refresh() since this is where these surfaces handle commits.
        self.notify_blocker_cleared();

        // These should be called periodically, before flushing the clients.
        self.niri.popups.cleanup();
        self.refresh_popup_grab();
        self.update_keyboard_focus();

        // Needs to be called after updating the keyboard focus.
        self.niri.refresh_layout();

        self.niri.cursor_manager.check_cursor_image_surface_alive();
        self.niri.refresh_pointer_outputs();
        self.niri.global_space.refresh();
        self.niri.refresh_idle_inhibit();
        self.refresh_pointer_contents();
        foreign_toplevel::refresh(self);
        self.niri.refresh_window_rules();
        self.refresh_ipc_outputs();
        self.ipc_refresh_layout();
        self.ipc_refresh_keyboard_layout_index();

        #[cfg(feature = "xdp-gnome-screencast")]
        self.niri.refresh_mapped_cast_outputs();
    }

    fn notify_blocker_cleared(&mut self) {
        let dh = self.niri.display_handle.clone();
        while let Ok(client) = self.niri.blocker_cleared_rx.try_recv() {
            trace!("calling blocker_cleared");
            self.client_compositor_state(&client)
                .blocker_cleared(self, &dh);
        }
    }

    pub fn move_cursor(&mut self, location: Point<f64, Logical>) {
        let under = self.niri.contents_under(location);
        self.niri.pointer_contents.clone_from(&under);

        let pointer = &self.niri.seat.get_pointer().unwrap();
        pointer.motion(
            self,
            under.surface,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: get_monotonic_time().as_millis() as u32,
            },
        );
        pointer.frame(self);

        self.niri.maybe_activate_pointer_constraint();

        // We do not show the pointer on programmatic or keyboard movement.

        // FIXME: granular
        self.niri.queue_redraw_all();
    }

    /// Moves cursor within the specified rectangle, only adjusting coordinates if needed.
    fn move_cursor_to_rect(&mut self, rect: Rectangle<f64, Logical>, mode: CenterCoords) -> bool {
        let pointer = &self.niri.seat.get_pointer().unwrap();
        let cur_loc = pointer.current_location();
        let x_in_bound = cur_loc.x >= rect.loc.x && cur_loc.x <= rect.loc.x + rect.size.w;
        let y_in_bound = cur_loc.y >= rect.loc.y && cur_loc.y <= rect.loc.y + rect.size.h;

        let p = match mode {
            CenterCoords::Separately => {
                if x_in_bound && y_in_bound {
                    return false;
                } else if y_in_bound {
                    // adjust x
                    Point::from((rect.loc.x + rect.size.w / 2.0, cur_loc.y))
                } else if x_in_bound {
                    // adjust y
                    Point::from((cur_loc.x, rect.loc.y + rect.size.h / 2.0))
                } else {
                    // adjust x and y
                    center_f64(rect)
                }
            }
            CenterCoords::Both => {
                if x_in_bound && y_in_bound {
                    return false;
                } else {
                    // adjust x and y
                    center_f64(rect)
                }
            }
        };

        self.move_cursor(p);
        true
    }

    pub fn move_cursor_to_focused_tile(&mut self, mode: CenterCoords) -> bool {
        if !self.niri.keyboard_focus.is_layout() {
            return false;
        }

        if self.niri.tablet_cursor_location.is_some() {
            return false;
        }

        let Some(output) = self.niri.layout.active_output() else {
            return false;
        };
        let monitor = self.niri.layout.monitor_for_output(output).unwrap();

        let mut rv = false;
        let rect = monitor.active_tile_visual_rectangle();

        if let Some(rect) = rect {
            let output_geo = self.niri.global_space.output_geometry(output).unwrap();
            let mut rect = rect;
            rect.loc += output_geo.loc.to_f64();
            rv = self.move_cursor_to_rect(rect, mode);
        }

        rv
    }

    /// Focus a specific window, taking care of a potential active output change and cursor
    /// warp.
    pub fn focus_window(&mut self, window: &Window) {
        let active_output = self.niri.layout.active_output().cloned();

        self.niri.layout.activate_window(window);

        let new_active = self.niri.layout.active_output().cloned();
        #[allow(clippy::collapsible_if)]
        if new_active != active_output {
            if !self.maybe_warp_cursor_to_focus_centered() {
                self.move_cursor_to_output(&new_active.unwrap());
            }
        } else {
            self.maybe_warp_cursor_to_focus();
        }

        // FIXME: granular
        self.niri.queue_redraw_all();
    }

    pub fn maybe_warp_cursor_to_focus(&mut self) -> bool {
        if !self.niri.config.borrow().input.warp_mouse_to_focus {
            return false;
        }

        self.move_cursor_to_focused_tile(CenterCoords::Separately)
    }

    pub fn maybe_warp_cursor_to_focus_centered(&mut self) -> bool {
        if !self.niri.config.borrow().input.warp_mouse_to_focus {
            return false;
        }

        self.move_cursor_to_focused_tile(CenterCoords::Both)
    }

    pub fn refresh_pointer_contents(&mut self) {
        let _span = tracy_client::span!("Niri::refresh_pointer_contents");

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

        if !self.update_pointer_contents() {
            return;
        }

        pointer.frame(self);

        // Pointer motion from a surface to nothing triggers a cursor change to default, which
        // means we may need to redraw.

        // FIXME: granular
        self.niri.queue_redraw_all();
    }

    pub fn update_pointer_contents(&mut self) -> bool {
        let _span = tracy_client::span!("Niri::update_pointer_contents");

        let pointer = &self.niri.seat.get_pointer().unwrap();
        let location = pointer.current_location();
        let under = if self.niri.pointer_hidden {
            PointContents::default()
        } else {
            self.niri.contents_under(location)
        };

        // We're not changing the global cursor location here, so if the contents did not change,
        // then nothing changed.
        if self.niri.pointer_contents == under {
            return false;
        }

        self.niri.pointer_contents.clone_from(&under);

        pointer.motion(
            self,
            under.surface,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: get_monotonic_time().as_millis() as u32,
            },
        );

        self.niri.maybe_activate_pointer_constraint();

        true
    }

    pub fn move_cursor_to_output(&mut self, output: &Output) {
        let geo = self.niri.global_space.output_geometry(output).unwrap();
        self.move_cursor(center(geo).to_f64());
    }

    pub fn refresh_popup_grab(&mut self) {
        let keyboard_grabbed = self.niri.seat.input_method().keyboard_grabbed();

        if let Some(grab) = &mut self.niri.popup_grab {
            if grab.grab.has_ended() {
                self.niri.popup_grab = None;
            } else if keyboard_grabbed {
                // HACK: remove popup grab if IME grabbed the keyboard, because we can't yet do
                // popup grabs together with an IME grab.
                // FIXME: do this properly.
                grab.grab.ungrab(PopupUngrabStrategy::All);
                self.niri.seat.get_pointer().unwrap().unset_grab(
                    self,
                    SERIAL_COUNTER.next_serial(),
                    get_monotonic_time().as_millis() as u32,
                );
                self.niri.popup_grab = None;
            }
        }
    }

    pub fn update_keyboard_focus(&mut self) {
        // Clean up on-demand layer surface focus if necessary.
        if let Some(surface) = &self.niri.layer_shell_on_demand_focus {
            // Still alive and has on-demand interactivity.
            let good = surface.alive()
                && surface.cached_state().keyboard_interactivity
                    == wlr_layer::KeyboardInteractivity::OnDemand;
            if !good {
                self.niri.layer_shell_on_demand_focus = None;
            }
        }

        // Compute the current focus.
        let focus = if self.niri.is_locked() {
            KeyboardFocus::LockScreen {
                surface: self.niri.lock_surface_focus(),
            }
        } else if self.niri.screenshot_ui.is_open() {
            KeyboardFocus::ScreenshotUi
        } else if let Some(output) = self.niri.layout.active_output() {
            let mon = self.niri.layout.monitor_for_output(output).unwrap();
            let layers = layer_map_for_output(output);

            // Explicitly check for layer-shell popup grabs here, our keyboard focus will stay on
            // the root layer surface while it has grabs.
            let layer_grab = self.niri.popup_grab.as_ref().and_then(|g| {
                layers
                    .layer_for_surface(&g.root, WindowSurfaceType::TOPLEVEL)
                    .map(|l| (&g.root, l.layer()))
            });
            let grab_on_layer = |layer: Layer| {
                layer_grab
                    .and_then(move |(s, l)| if l == layer { Some(s.clone()) } else { None })
                    .map(|surface| KeyboardFocus::LayerShell { surface })
            };

            let layout_focus = || {
                self.niri
                    .layout
                    .focus()
                    .map(|win| win.toplevel().wl_surface().clone())
                    .map(|surface| KeyboardFocus::Layout {
                        surface: Some(surface),
                    })
            };

            let excl_focus_on_layer = |layer| {
                layers.layers_on(layer).find_map(|surface| {
                    let can_receive_exclusive_focus = surface.cached_state().keyboard_interactivity
                        == wlr_layer::KeyboardInteractivity::Exclusive;
                    can_receive_exclusive_focus
                        .then(|| surface.wl_surface().clone())
                        .map(|surface| KeyboardFocus::LayerShell { surface })
                })
            };

            let on_d_focus_on_layer = |layer| {
                layers.layers_on(layer).find_map(|surface| {
                    let is_on_demand_surface =
                        Some(surface) == self.niri.layer_shell_on_demand_focus.as_ref();
                    is_on_demand_surface
                        .then(|| surface.wl_surface().clone())
                        .map(|surface| KeyboardFocus::LayerShell { surface })
                })
            };

            // Prefer exclusive focus on a layer, then check on-demand focus.
            let focus_on_layer =
                |layer| excl_focus_on_layer(layer).or_else(|| on_d_focus_on_layer(layer));

            let mut surface = grab_on_layer(Layer::Overlay);
            // FIXME: we shouldn't prioritize the top layer grabs over regular overlay input or a
            // fullscreen layout window. This will need tracking in grab() to avoid handing it out
            // in the first place. Or a better way to structure this code.
            surface = surface.or_else(|| grab_on_layer(Layer::Top));

            surface = surface.or_else(|| focus_on_layer(Layer::Overlay));

            if mon.render_above_top_layer() {
                surface = surface.or_else(layout_focus);
                surface = surface.or_else(|| focus_on_layer(Layer::Top));
            } else {
                surface = surface.or_else(|| focus_on_layer(Layer::Top));
                surface = surface.or_else(layout_focus);
            }

            // Bottom and background layers can receive on-demand focus only.
            surface = surface.or_else(|| on_d_focus_on_layer(Layer::Bottom));
            surface = surface.or_else(|| on_d_focus_on_layer(Layer::Background));

            surface.unwrap_or(KeyboardFocus::Layout { surface: None })
        } else {
            KeyboardFocus::Layout { surface: None }
        };

        let keyboard = self.niri.seat.get_keyboard().unwrap();
        if self.niri.keyboard_focus != focus {
            trace!(
                "keyboard focus changed from {:?} to {:?}",
                self.niri.keyboard_focus,
                focus
            );

            // Tell the windows their new focus state for window rule purposes.
            let mut previous_focus = None;
            if let KeyboardFocus::Layout {
                surface: Some(surface),
            } = &self.niri.keyboard_focus
            {
                if let Some((mapped, _)) = self.niri.layout.find_window_and_output_mut(surface) {
                    mapped.set_is_focused(false);
                    previous_focus = Some(mapped.window.clone());
                }
            }
            if let KeyboardFocus::Layout {
                surface: Some(surface),
            } = &focus
            {
                if let Some((mapped, _)) = self.niri.layout.find_window_and_output_mut(surface) {
                    mapped.set_is_focused(true);
                }
            }

            // Update the previous focus but only when staying focused on the layout.
            //
            // Case 1: opening and closing exclusive-keyboard layer-shell (e.g. app launcher). This
            // involves going from Layout to LayerShell, then from LayerShell to Layout. The
            // previously focused window should stay unchanged.
            //
            //     Case 1.5: opening layer-shell, in the background switching layout focus, closing
            //     layer-shell. With the current logic, this won't update the previously focused
            //     window, which is incorrect. But this case should be rare.
            //
            // Case 2: switching to an empty workspace, then hitting FocusWindowPrevious. The focus
            // should go to the window that was just focused. The keyboard focus goes from Layout
            // (with Some surface) to Layout (with None surface), so we update the previously
            // focused window.
            //
            // FIXME: Ideally this should happen inside Layout itself, then there wouldn't be any
            // problems with layer-shell, etc.
            if matches!(self.niri.keyboard_focus, KeyboardFocus::Layout { .. })
                && matches!(focus, KeyboardFocus::Layout { .. })
            {
                self.niri.previously_focused_window = previous_focus;
            }

            if let Some(grab) = self.niri.popup_grab.as_mut() {
                if Some(&grab.root) != focus.surface() {
                    trace!(
                        "grab root {:?} is not the new focus {:?}, ungrabbing",
                        grab.root,
                        focus
                    );

                    grab.grab.ungrab(PopupUngrabStrategy::All);
                    keyboard.unset_grab(self);
                    self.niri.seat.get_pointer().unwrap().unset_grab(
                        self,
                        SERIAL_COUNTER.next_serial(),
                        get_monotonic_time().as_millis() as u32,
                    );
                    self.niri.popup_grab = None;
                }
            }

            if self.niri.config.borrow().input.keyboard.track_layout == TrackLayout::Window {
                let current_layout = keyboard.with_xkb_state(self, |context| {
                    let xkb = context.xkb().lock().unwrap();
                    xkb.active_layout()
                });

                let mut new_layout = current_layout;
                // Store the currently active layout for the surface.
                if let Some(current_focus) = self.niri.keyboard_focus.surface() {
                    with_states(current_focus, |data| {
                        let cell = data
                            .data_map
                            .get_or_insert::<Cell<KeyboardLayout>, _>(Cell::default);
                        cell.set(current_layout);
                    });
                }

                if let Some(focus) = focus.surface() {
                    new_layout = with_states(focus, |data| {
                        let cell = data.data_map.get_or_insert::<Cell<KeyboardLayout>, _>(|| {
                            // The default layout is effectively the first layout in the
                            // keymap, so use it for new windows.
                            Cell::new(KeyboardLayout::default())
                        });
                        cell.get()
                    });
                }
                if new_layout != current_layout && focus.surface().is_some() {
                    keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
                    keyboard.with_xkb_state(self, |mut context| {
                        context.set_layout(new_layout);
                    });
                }
            }

            self.niri.keyboard_focus.clone_from(&focus);
            keyboard.set_focus(self, focus.into_surface(), SERIAL_COUNTER.next_serial());

            // FIXME: can be more granular.
            self.niri.queue_redraw_all();
        }
    }

    pub fn reload_config(&mut self, path: PathBuf) {
        let _span = tracy_client::span!("State::reload_config");

        let mut config = match Config::load(&path) {
            Ok(config) => config,
            Err(err) => {
                warn!("{:?}", err.context("error loading config"));
                self.niri.config_error_notification.show();
                self.niri.queue_redraw_all();
                return;
            }
        };

        self.niri.config_error_notification.hide();

        // Find & orphan removed named workspaces.
        let mut removed_workspaces: Vec<String> = vec![];
        for ws in &self.niri.config.borrow().workspaces {
            if !config.workspaces.iter().any(|w| w.name == ws.name) {
                removed_workspaces.push(ws.name.0.clone());
            }
        }
        for name in removed_workspaces {
            self.niri.layout.unname_workspace(&name);
        }

        self.niri.layout.update_config(&config);

        // Create new named workspaces.
        for ws_config in &config.workspaces {
            self.niri.layout.ensure_named_workspace(ws_config);
        }

        let rate = 1.0 / config.animations.slowdown.max(0.001);
        self.niri.clock.set_rate(rate);
        self.niri
            .clock
            .set_complete_instantly(config.animations.off);

        *CHILD_ENV.write().unwrap() = mem::take(&mut config.environment);

        let mut reload_xkb = None;
        let mut libinput_config_changed = false;
        let mut output_config_changed = false;
        let mut preserved_output_config = None;
        let mut window_rules_changed = false;
        let mut layer_rules_changed = false;
        let mut debug_config_changed = false;
        let mut shaders_changed = false;
        let mut cursor_inactivity_timeout_changed = false;
        let mut old_config = self.niri.config.borrow_mut();

        // Reload the cursor.
        if config.cursor != old_config.cursor {
            self.niri
                .cursor_manager
                .reload(&config.cursor.xcursor_theme, config.cursor.xcursor_size);
            self.niri.cursor_texture_cache.clear();
        }

        // We need &mut self to reload the xkb config, so just store it here.
        if config.input.keyboard.xkb != old_config.input.keyboard.xkb {
            reload_xkb = Some(config.input.keyboard.xkb.clone());
        }

        // Reload the repeat info.
        if config.input.keyboard.repeat_rate != old_config.input.keyboard.repeat_rate
            || config.input.keyboard.repeat_delay != old_config.input.keyboard.repeat_delay
        {
            let keyboard = self.niri.seat.get_keyboard().unwrap();
            keyboard.change_repeat_info(
                config.input.keyboard.repeat_rate.into(),
                config.input.keyboard.repeat_delay.into(),
            );
        }

        if config.input.touchpad != old_config.input.touchpad
            || config.input.mouse != old_config.input.mouse
            || config.input.trackpoint != old_config.input.trackpoint
        {
            libinput_config_changed = true;
        }

        if config.outputs != self.niri.config_file_output_config {
            output_config_changed = true;
            self.niri
                .config_file_output_config
                .clone_from(&config.outputs);
        } else {
            // Output config did not change from the last disk load, so we need to preserve the
            // transient changes.
            preserved_output_config = Some(mem::take(&mut old_config.outputs));
        }

        if config.binds != old_config.binds {
            self.niri.hotkey_overlay.on_hotkey_config_updated();
            self.niri.mods_with_wheel_binds =
                mods_with_wheel_binds(self.backend.mod_key(), &config.binds);
            self.niri.mods_with_finger_scroll_binds =
                mods_with_finger_scroll_binds(self.backend.mod_key(), &config.binds);
        }

        if config.window_rules != old_config.window_rules {
            window_rules_changed = true;
        }

        if config.layer_rules != old_config.layer_rules {
            layer_rules_changed = true;
        }

        if config.animations.window_resize.custom_shader
            != old_config.animations.window_resize.custom_shader
        {
            let src = config.animations.window_resize.custom_shader.as_deref();
            self.backend.with_primary_renderer(|renderer| {
                shaders::set_custom_resize_program(renderer, src);
            });
            shaders_changed = true;
        }

        if config.animations.window_close.custom_shader
            != old_config.animations.window_close.custom_shader
        {
            let src = config.animations.window_close.custom_shader.as_deref();
            self.backend.with_primary_renderer(|renderer| {
                shaders::set_custom_close_program(renderer, src);
            });
            shaders_changed = true;
        }

        if config.animations.window_open.custom_shader
            != old_config.animations.window_open.custom_shader
        {
            let src = config.animations.window_open.custom_shader.as_deref();
            self.backend.with_primary_renderer(|renderer| {
                shaders::set_custom_open_program(renderer, src);
            });
            shaders_changed = true;
        }

        if config.cursor.hide_after_inactive_ms != old_config.cursor.hide_after_inactive_ms {
            cursor_inactivity_timeout_changed = true;
        }

        if config.debug != old_config.debug {
            debug_config_changed = true;

            if config.debug.keep_laptop_panel_on_when_lid_is_closed
                != old_config.debug.keep_laptop_panel_on_when_lid_is_closed
            {
                output_config_changed = true;
            }
        }

        *old_config = config;

        if let Some(outputs) = preserved_output_config {
            old_config.outputs = outputs;
        }

        // Release the borrow.
        drop(old_config);

        // Now with a &mut self we can reload the xkb config.
        if let Some(xkb) = reload_xkb {
            let keyboard = self.niri.seat.get_keyboard().unwrap();
            if let Err(err) = keyboard.set_xkb_config(self, xkb.to_xkb_config()) {
                warn!("error updating xkb config: {err:?}");
            }

            self.ipc_keyboard_layouts_changed();
        }

        if libinput_config_changed {
            let config = self.niri.config.borrow();
            for mut device in self.niri.devices.iter().cloned() {
                apply_libinput_settings(&config.input, &mut device);
            }
        }

        if output_config_changed {
            self.reload_output_config();
        }

        if debug_config_changed {
            self.backend.on_debug_config_changed();
        }

        if window_rules_changed {
            self.niri.recompute_window_rules();
        }

        if layer_rules_changed {
            self.niri.recompute_layer_rules();
        }

        if shaders_changed {
            self.niri.layout.update_shaders();
        }

        if cursor_inactivity_timeout_changed {
            self.niri.reset_pointer_inactivity_timer();
        }

        // Can't really update xdg-decoration settings since we have to hide the globals for CSD
        // due to the SDL2 bug... I don't imagine clients are prepared for the xdg-decoration
        // global suddenly appearing? Either way, right now it's live-reloaded in a sense that new
        // clients will use the new xdg-decoration setting.

        self.niri.queue_redraw_all();
    }

    pub fn reload_output_config(&mut self) {
        let mut resized_outputs = vec![];
        let mut recolored_outputs = vec![];

        for output in self.niri.global_space.outputs() {
            let name = output.user_data().get::<OutputName>().unwrap();
            let config = self.niri.config.borrow_mut();
            let config = config.outputs.find(name);

            let scale = config
                .and_then(|c| c.scale)
                .map(|s| s.0)
                .unwrap_or_else(|| {
                    let size_mm = output.physical_properties().size;
                    let resolution = output.current_mode().unwrap().size;
                    guess_monitor_scale(size_mm, resolution)
                });
            let scale = closest_representable_scale(scale.clamp(0.1, 10.));

            let mut transform = config
                .map(|c| ipc_transform_to_smithay(c.transform))
                .unwrap_or(Transform::Normal);
            // FIXME: fix winit damage on other transforms.
            if name.connector == "winit" {
                transform = Transform::Flipped180;
            }

            if output.current_scale().fractional_scale() != scale
                || output.current_transform() != transform
            {
                output.change_current_state(
                    None,
                    Some(transform),
                    Some(output::Scale::Fractional(scale)),
                    None,
                );
                self.niri.ipc_outputs_changed = true;
                resized_outputs.push(output.clone());
            }

            let mut background_color = config
                .map(|c| c.background_color)
                .unwrap_or(DEFAULT_BACKGROUND_COLOR)
                .to_array_unpremul();
            background_color[3] = 1.;
            let background_color = Color32F::from(background_color);

            if let Some(state) = self.niri.output_state.get_mut(output) {
                if state.background_buffer.color() != background_color {
                    state.background_buffer.set_color(background_color);
                    recolored_outputs.push(output.clone());
                }
            }
        }

        for output in resized_outputs {
            self.niri.output_resized(&output);
        }

        for output in recolored_outputs {
            self.niri.queue_redraw(&output);
        }

        self.backend.on_output_config_changed(&mut self.niri);

        self.niri.reposition_outputs(None);

        if let Some(touch) = self.niri.seat.get_touch() {
            touch.cancel(self);
        }

        let config = self.niri.config.borrow().outputs.clone();
        self.niri.output_management_state.on_config_changed(config);
    }

    pub fn apply_transient_output_config(&mut self, name: &str, action: niri_ipc::OutputAction) {
        {
            // Try hard to find the output config section corresponding to the output set by the
            // user. Since if we add a new section and some existing section also matches the
            // output, then our new section won't do anything.
            let temp;
            let match_name = if let Some(output) = self.niri.output_by_name_match(name) {
                output.user_data().get::<OutputName>().unwrap()
            } else if let Some(output_name) = self
                .backend
                .tty_checked()
                .and_then(|tty| tty.disconnected_connector_name_by_name_match(name))
            {
                temp = output_name;
                &temp
            } else {
                // Even if name is "make model serial", matching will work fine this way.
                temp = OutputName {
                    connector: name.to_owned(),
                    make: None,
                    model: None,
                    serial: None,
                };
                &temp
            };

            let mut config = self.niri.config.borrow_mut();
            let config = if let Some(config) = config.outputs.find_mut(match_name) {
                config
            } else {
                config.outputs.0.push(niri_config::Output {
                    // Save name as set by the user.
                    name: String::from(name),
                    ..Default::default()
                });
                config.outputs.0.last_mut().unwrap()
            };

            match action {
                niri_ipc::OutputAction::Off => config.off = true,
                niri_ipc::OutputAction::On => config.off = false,
                niri_ipc::OutputAction::Mode { mode } => {
                    config.mode = match mode {
                        niri_ipc::ModeToSet::Automatic => None,
                        niri_ipc::ModeToSet::Specific(mode) => Some(mode),
                    }
                }
                niri_ipc::OutputAction::Scale { scale } => {
                    config.scale = match scale {
                        niri_ipc::ScaleToSet::Automatic => None,
                        niri_ipc::ScaleToSet::Specific(scale) => Some(FloatOrInt(scale)),
                    }
                }
                niri_ipc::OutputAction::Transform { transform } => config.transform = transform,
                niri_ipc::OutputAction::Position { position } => {
                    config.position = match position {
                        niri_ipc::PositionToSet::Automatic => None,
                        niri_ipc::PositionToSet::Specific(position) => {
                            Some(niri_config::Position {
                                x: position.x,
                                y: position.y,
                            })
                        }
                    }
                }
                niri_ipc::OutputAction::Vrr { vrr } => {
                    config.variable_refresh_rate = if vrr.vrr {
                        Some(niri_config::Vrr {
                            on_demand: vrr.on_demand,
                        })
                    } else {
                        None
                    }
                }
            }
        }

        self.reload_output_config();
    }

    pub fn refresh_ipc_outputs(&mut self) {
        if !self.niri.ipc_outputs_changed {
            return;
        }
        self.niri.ipc_outputs_changed = false;

        let _span = tracy_client::span!("State::refresh_ipc_outputs");

        for ipc_output in self.backend.ipc_outputs().lock().unwrap().values_mut() {
            let logical = self
                .niri
                .global_space
                .outputs()
                .find(|output| output.name() == ipc_output.name)
                .map(logical_output);
            ipc_output.logical = logical;
        }

        #[cfg(feature = "dbus")]
        self.niri.on_ipc_outputs_changed();

        let new_config = self.backend.ipc_outputs().lock().unwrap().clone();
        self.niri.output_management_state.notify_changes(new_config);
    }

    pub fn open_screenshot_ui(&mut self) {
        if self.niri.is_locked() || self.niri.screenshot_ui.is_open() {
            return;
        }

        let default_output = self
            .niri
            .output_under_cursor()
            .or_else(|| self.niri.layout.active_output().cloned());
        let Some(default_output) = default_output else {
            return;
        };

        self.niri.update_render_elements(None);

        let Some(screenshots) = self
            .backend
            .with_primary_renderer(|renderer| self.niri.capture_screenshots(renderer).collect())
        else {
            return;
        };

        // Now that we captured the screenshots, clear grabs like drag-and-drop, etc.
        self.niri.seat.get_pointer().unwrap().unset_grab(
            self,
            SERIAL_COUNTER.next_serial(),
            get_monotonic_time().as_millis() as u32,
        );

        self.backend.with_primary_renderer(|renderer| {
            self.niri
                .screenshot_ui
                .open(renderer, screenshots, default_output)
        });

        self.niri
            .cursor_manager
            .set_cursor_image(CursorImageStatus::Named(CursorIcon::Crosshair));
        self.niri.queue_redraw_all();
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn on_pw_msg(&mut self, msg: PwToNiri) {
        match msg {
            PwToNiri::StopCast { session_id } => self.niri.stop_cast(session_id),
            PwToNiri::Redraw(target) => match target {
                CastTarget::Output(weak) => {
                    if let Some(output) = weak.upgrade() {
                        self.niri.queue_redraw(&output);
                    }
                }
                CastTarget::Window { id } => {
                    self.backend.with_primary_renderer(|renderer| {
                        // FIXME: target presentation time at the time of window commit?
                        self.niri
                            .render_window_for_screen_cast(renderer, id, get_monotonic_time());
                    });
                }
            },
        }
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn on_screen_cast_msg(&mut self, msg: ScreenCastToNiri) {
        use crate::dbus::mutter_screen_cast::StreamTargetId;

        match msg {
            ScreenCastToNiri::StartCast {
                session_id,
                target,
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

                let Some(pw) = &self.niri.pipewire else {
                    error!("screencasting must be disabled if PipeWire is missing");
                    return;
                };

                let (target, size, refresh, alpha) = match target {
                    StreamTargetId::Output { name } => {
                        let global_space = &self.niri.global_space;
                        let output = global_space.outputs().find(|out| out.name() == name);
                        let Some(output) = output else {
                            warn!("error starting screencast: requested output is missing");
                            self.niri.stop_cast(session_id);
                            return;
                        };

                        let mode = output.current_mode().unwrap();
                        let transform = output.current_transform();
                        let size = transform.transform_size(mode.size);
                        let refresh = mode.refresh as u32;
                        (CastTarget::Output(output.downgrade()), size, refresh, false)
                    }
                    StreamTargetId::Window { id } => {
                        let mut window = None;
                        self.niri.layout.with_windows(|mapped, _, _| {
                            if mapped.id().get() != id {
                                return;
                            }

                            window = Some(mapped.window.clone());
                        });

                        let Some(window) = window else {
                            warn!("error starting screencast: requested window is missing");
                            self.niri.stop_cast(session_id);
                            return;
                        };

                        // Use the cached output since it will be present even if the output was
                        // currently disconnected.
                        let Some(output) = self.niri.mapped_cast_output.get(&window) else {
                            warn!("error starting screencast: requested window is missing");
                            self.niri.stop_cast(session_id);
                            return;
                        };

                        let scale = Scale::from(output.current_scale().fractional_scale());
                        let bbox = window.bbox_with_popups().to_physical_precise_up(scale);
                        let refresh = output.current_mode().unwrap().refresh as u32;

                        (CastTarget::Window { id }, bbox.size, refresh, true)
                    }
                };

                let render_formats = self
                    .backend
                    .with_primary_renderer(|renderer| {
                        renderer.egl_context().dmabuf_render_formats().clone()
                    })
                    .unwrap_or_default();

                let res = pw.start_cast(
                    gbm,
                    render_formats,
                    session_id,
                    target,
                    size,
                    refresh,
                    alpha,
                    cursor_mode,
                    signal_ctx,
                );
                match res {
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

        let rv = self.backend.with_primary_renderer(|renderer| {
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
        });

        if rv.is_none() {
            let msg = NiriToScreenshot::ScreenshotResult(None);
            if let Err(err) = to_screenshot.send_blocking(msg) {
                warn!("error sending None to screenshot: {err:?}");
            }
        }
    }

    #[cfg(feature = "dbus")]
    pub fn on_introspect_msg(
        &mut self,
        to_introspect: &async_channel::Sender<NiriToIntrospect>,
        msg: IntrospectToNiri,
    ) {
        use crate::utils::with_toplevel_role;

        let IntrospectToNiri::GetWindows = msg;
        let _span = tracy_client::span!("GetWindows");

        let mut windows = HashMap::new();

        self.niri.layout.with_windows(|mapped, _, _| {
            let id = mapped.id().get();
            let props = with_toplevel_role(mapped.toplevel(), |role| {
                gnome_shell_introspect::WindowProperties {
                    title: role.title.clone().unwrap_or_default(),
                    app_id: role.app_id.clone().unwrap_or_default(),
                }
            });

            windows.insert(id, props);
        });

        let msg = NiriToIntrospect::Windows(windows);
        if let Err(err) = to_introspect.send_blocking(msg) {
            warn!("error sending windows to introspect: {err:?}");
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

        let (executor, scheduler) = calloop::futures::executor().unwrap();
        event_loop.insert_source(executor, |_, _, _| ()).unwrap();

        let display_handle = display.handle();
        let config_ = config.borrow();
        let config_file_output_config = config_.outputs.clone();

        let mut animation_clock = Clock::default();

        let rate = 1.0 / config_.animations.slowdown.max(0.001);
        animation_clock.set_rate(rate);
        animation_clock.set_complete_instantly(config_.animations.off);

        let layout = Layout::new(animation_clock.clone(), &config_);

        let (blocker_cleared_tx, blocker_cleared_rx) = mpsc::channel();

        let compositor_state = CompositorState::new_v6::<State>(&display_handle);
        let xdg_shell_state = XdgShellState::new_with_capabilities::<State>(
            &display_handle,
            [WmCapabilities::Fullscreen],
        );
        let xdg_decoration_state =
            XdgDecorationState::new_with_filter::<State, _>(&display_handle, |client| {
                client
                    .get_data::<ClientState>()
                    .unwrap()
                    .can_view_decoration_globals
            });
        let kde_decoration_state = KdeDecorationState::new_with_filter::<State, _>(
            &display_handle,
            // If we want CSD we will hide the global.
            KdeDecorationsMode::Server,
            |client| {
                client
                    .get_data::<ClientState>()
                    .unwrap()
                    .can_view_decoration_globals
            },
        );
        let layer_shell_state =
            WlrLayerShellState::new_with_filter::<State, _>(&display_handle, |client| {
                !client.get_data::<ClientState>().unwrap().restricted
            });
        let session_lock_state =
            SessionLockManagerState::new::<State, _>(&display_handle, |client| {
                !client.get_data::<ClientState>().unwrap().restricted
            });
        let shm_state = ShmState::new::<State>(
            &display_handle,
            vec![wl_shm::Format::Xbgr8888, wl_shm::Format::Abgr8888],
        );
        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<State>(&display_handle);
        let dmabuf_state = DmabufState::new();
        let fractional_scale_manager_state =
            FractionalScaleManagerState::new::<State>(&display_handle);
        let mut seat_state = SeatState::new();
        let tablet_state = TabletManagerState::new::<State>(&display_handle);
        let pointer_gestures_state = PointerGesturesState::new::<State>(&display_handle);
        let relative_pointer_state = RelativePointerManagerState::new::<State>(&display_handle);
        let pointer_constraints_state = PointerConstraintsState::new::<State>(&display_handle);
        let idle_notifier_state = IdleNotifierState::new(&display_handle, event_loop.clone());
        let idle_inhibit_manager_state = IdleInhibitManagerState::new::<State>(&display_handle);
        let data_device_state = DataDeviceState::new::<State>(&display_handle);
        let primary_selection_state = PrimarySelectionState::new::<State>(&display_handle);
        let data_control_state = DataControlState::new::<State, _>(
            &display_handle,
            Some(&primary_selection_state),
            |client| !client.get_data::<ClientState>().unwrap().restricted,
        );
        let presentation_state =
            PresentationState::new::<State>(&display_handle, Monotonic::ID as u32);
        let security_context_state =
            SecurityContextState::new::<State, _>(&display_handle, |client| {
                !client.get_data::<ClientState>().unwrap().restricted
            });

        let text_input_state = TextInputManagerState::new::<State>(&display_handle);
        let input_method_state =
            InputMethodManagerState::new::<State, _>(&display_handle, |client| {
                !client.get_data::<ClientState>().unwrap().restricted
            });
        let virtual_keyboard_state =
            VirtualKeyboardManagerState::new::<State, _>(&display_handle, |client| {
                !client.get_data::<ClientState>().unwrap().restricted
            });

        let foreign_toplevel_state =
            ForeignToplevelManagerState::new::<State, _>(&display_handle, |client| {
                !client.get_data::<ClientState>().unwrap().restricted
            });
        let mut output_management_state =
            OutputManagementManagerState::new::<State, _>(&display_handle, |client| {
                !client.get_data::<ClientState>().unwrap().restricted
            });
        output_management_state.on_config_changed(config_.outputs.clone());
        let screencopy_state = ScreencopyManagerState::new::<State, _>(&display_handle, |client| {
            !client.get_data::<ClientState>().unwrap().restricted
        });
        let viewporter_state = ViewporterState::new::<State>(&display_handle);
        let xdg_foreign_state = XdgForeignState::new::<State>(&display_handle);

        let is_tty = matches!(backend, Backend::Tty(_));
        let gamma_control_manager_state =
            GammaControlManagerState::new::<State, _>(&display_handle, move |client| {
                is_tty && !client.get_data::<ClientState>().unwrap().restricted
            });
        let activation_state = XdgActivationState::new::<State>(&display_handle);
        event_loop
            .insert_source(
                Timer::from_duration(XDG_ACTIVATION_TOKEN_TIMEOUT),
                |_, _, state| {
                    state.niri.activation_state.retain_tokens(|_, token_data| {
                        token_data.timestamp.elapsed() < XDG_ACTIVATION_TOKEN_TIMEOUT
                    });
                    TimeoutAction::ToDuration(XDG_ACTIVATION_TOKEN_TIMEOUT)
                },
            )
            .unwrap();

        let mutter_x11_interop_state =
            MutterX11InteropManagerState::new::<State, _>(&display_handle, move |_| true);

        let mut seat: Seat<State> = seat_state.new_wl_seat(&display_handle, backend.seat_name());
        seat.add_keyboard(
            config_.input.keyboard.xkb.to_xkb_config(),
            config_.input.keyboard.repeat_delay.into(),
            config_.input.keyboard.repeat_rate.into(),
        )
        .unwrap();
        seat.add_pointer();

        let cursor_shape_manager_state = CursorShapeManagerState::new::<State>(&display_handle);
        let cursor_manager =
            CursorManager::new(&config_.cursor.xcursor_theme, config_.cursor.xcursor_size);

        let mods_with_wheel_binds = mods_with_wheel_binds(backend.mod_key(), &config_.binds);
        let mods_with_finger_scroll_binds =
            mods_with_finger_scroll_binds(backend.mod_key(), &config_.binds);

        let screenshot_ui = ScreenshotUi::new(animation_clock.clone(), config.clone());
        let config_error_notification =
            ConfigErrorNotification::new(animation_clock.clone(), config.clone());

        let mut hotkey_overlay = HotkeyOverlay::new(config.clone(), backend.mod_key());
        if !config_.hotkey_overlay.skip_at_startup {
            hotkey_overlay.show();
        }

        let exit_confirm_dialog = match ExitConfirmDialog::new() {
            Ok(x) => Some(x),
            Err(err) => {
                warn!("error creating the exit confirm dialog: {err:?}");
                None
            }
        };

        event_loop
            .insert_source(
                Timer::from_duration(Duration::from_secs(1)),
                |_, _, state| {
                    state.niri.send_frame_callbacks_on_fallback_timer();
                    TimeoutAction::ToDuration(Duration::from_secs(1))
                },
            )
            .unwrap();

        let socket_source = ListeningSocketSource::new_auto().unwrap();
        let socket_name = socket_source.socket_name().to_os_string();
        event_loop
            .insert_source(socket_source, move |client, _, state| {
                let config = state.niri.config.borrow();
                let data = Arc::new(ClientState {
                    compositor_state: Default::default(),
                    can_view_decoration_globals: config.prefer_no_csd,
                    restricted: false,
                    credentials_unknown: false,
                });

                if let Err(err) = state.niri.display_handle.insert_client(client, data) {
                    warn!("error inserting client: {err}");
                }
            })
            .unwrap();

        let ipc_server = match IpcServer::start(&event_loop, &socket_name.to_string_lossy()) {
            Ok(server) => Some(server),
            Err(err) => {
                warn!("error starting IPC server: {err:?}");
                None
            }
        };

        let pipewire = match PipeWire::new(&event_loop) {
            Ok(pipewire) => Some(pipewire),
            Err(err) => {
                warn!("error connecting to PipeWire, screencasting will not work: {err:?}");
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

        event_loop
            .insert_source(
                Timer::from_duration(Duration::from_secs(60)),
                |_, _, state| {
                    let _span = tracy_client::span!("startup timeout");
                    state.niri.is_at_startup = false;
                    state.niri.recompute_window_rules();
                    state.niri.recompute_layer_rules();
                    TimeoutAction::Drop
                },
            )
            .unwrap();

        drop(config_);
        let mut niri = Self {
            config,
            config_file_output_config,

            event_loop,
            scheduler,
            stop_signal,
            socket_name,
            display_handle,
            start_time: Instant::now(),
            is_at_startup: true,
            clock: animation_clock,

            layout,
            global_space: Space::default(),
            output_state: HashMap::new(),
            unmapped_windows: HashMap::new(),
            unmapped_layer_surfaces: HashSet::new(),
            mapped_layer_surfaces: HashMap::new(),
            root_surface: HashMap::new(),
            dmabuf_pre_commit_hook: HashMap::new(),
            blocker_cleared_tx,
            blocker_cleared_rx,
            monitors_active: true,
            is_lid_closed: false,

            devices: HashSet::new(),
            tablets: HashMap::new(),
            touch: HashSet::new(),

            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            kde_decoration_state,
            layer_shell_state,
            session_lock_state,
            foreign_toplevel_state,
            output_management_state,
            screencopy_state,
            viewporter_state,
            xdg_foreign_state,
            text_input_state,
            input_method_state,
            virtual_keyboard_state,
            shm_state,
            output_manager_state,
            dmabuf_state,
            fractional_scale_manager_state,
            seat_state,
            tablet_state,
            pointer_gestures_state,
            relative_pointer_state,
            pointer_constraints_state,
            idle_notifier_state,
            idle_inhibit_manager_state,
            data_device_state,
            primary_selection_state,
            data_control_state,
            popups: PopupManager::default(),
            popup_grab: None,
            suppressed_keys: HashSet::new(),
            bind_cooldown_timers: HashMap::new(),
            bind_repeat_timer: Option::default(),
            presentation_state,
            security_context_state,
            gamma_control_manager_state,
            activation_state,
            mutter_x11_interop_state,

            seat,
            keyboard_focus: KeyboardFocus::Layout { surface: None },
            layer_shell_on_demand_focus: None,
            previously_focused_window: None,
            idle_inhibiting_surfaces: HashSet::new(),
            is_fdo_idle_inhibited: Arc::new(AtomicBool::new(false)),
            cursor_manager,
            cursor_texture_cache: Default::default(),
            cursor_shape_manager_state,
            dnd_icon: None,
            pointer_contents: PointContents::default(),
            pointer_hidden: false,
            pointer_inactivity_timer: None,
            tablet_cursor_location: None,
            gesture_swipe_3f_cumulative: None,
            vertical_wheel_tracker: ScrollTracker::new(120),
            horizontal_wheel_tracker: ScrollTracker::new(120),
            mods_with_wheel_binds,

            // 10 is copied from Clutter: DISCRETE_SCROLL_STEP.
            vertical_finger_scroll_tracker: ScrollTracker::new(10),
            horizontal_finger_scroll_tracker: ScrollTracker::new(10),
            mods_with_finger_scroll_binds,

            lock_state: LockState::Unlocked,

            screenshot_ui,
            config_error_notification,
            hotkey_overlay,
            exit_confirm_dialog,

            debug_draw_opaque_regions: false,
            debug_draw_damage: false,

            #[cfg(feature = "dbus")]
            dbus: None,
            #[cfg(feature = "dbus")]
            inhibit_power_key_fd: None,

            ipc_server,
            ipc_outputs_changed: false,

            pipewire,
            casts: vec![],

            #[cfg(feature = "xdp-gnome-screencast")]
            mapped_cast_output: HashMap::new(),
        };

        niri.reset_pointer_inactivity_timer();

        niri
    }

    #[cfg(feature = "dbus")]
    pub fn inhibit_power_key(&mut self) -> anyhow::Result<()> {
        use std::os::fd::{AsRawFd, BorrowedFd};

        use smithay::reexports::rustix::io::{fcntl_setfd, FdFlags};

        let conn = zbus::blocking::ConnectionBuilder::system()?.build()?;

        let message = conn.call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &("handle-power-key", "niri", "Power key handling", "block"),
        )?;

        let fd: zbus::zvariant::OwnedFd = message.body()?;

        // Don't leak the fd to child processes.
        let borrowed = unsafe { BorrowedFd::borrow_raw(fd.as_raw_fd()) };
        if let Err(err) = fcntl_setfd(borrowed, FdFlags::CLOEXEC) {
            warn!("error setting CLOEXEC on inhibit fd: {err:?}");
        };

        self.inhibit_power_key_fd = Some(fd);

        Ok(())
    }

    /// Repositions all outputs, optionally adding a new output.
    pub fn reposition_outputs(&mut self, new_output: Option<&Output>) {
        let _span = tracy_client::span!("Niri::reposition_outputs");

        #[derive(Debug)]
        struct Data {
            output: Output,
            name: OutputName,
            position: Option<Point<i32, Logical>>,
            config: Option<niri_config::Position>,
        }

        let config = self.config.borrow();
        let mut outputs = vec![];
        for output in self.global_space.outputs().chain(new_output) {
            let name = output.user_data().get::<OutputName>().unwrap();
            let position = self.global_space.output_geometry(output).map(|geo| geo.loc);
            let config = config.outputs.find(name).and_then(|c| c.position);

            outputs.push(Data {
                output: output.clone(),
                name: name.clone(),
                position,
                config,
            });
        }
        drop(config);

        for Data { output, .. } in &outputs {
            self.global_space.unmap_output(output);
        }

        // Connectors can appear in udev in any order. If we sort by name then we get output
        // positioning that does not depend on the order they appeared.
        //
        // This sorting first compares by make/model/serial so that it is stable regardless of the
        // connector name. However, if make/model/serial is equal or unknown, then it does fall
        // back to comparing the connector name, which should always be unique.
        outputs.sort_unstable_by(|a, b| a.name.compare(&b.name));

        // Place all outputs with explicitly configured position first, then the unconfigured ones.
        outputs.sort_by_key(|d| d.config.is_none());

        trace!(
            "placing outputs in order: {:?}",
            outputs.iter().map(|d| &d.name.connector)
        );

        for data in outputs.into_iter() {
            let Data {
                output,
                name,
                position,
                config,
            } = data;

            let size = output_size(&output).to_i32_round();

            let new_position = config
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
                            "output {} at x={} y={} sized {}x{} \
                             overlaps an existing output at x={} y={} sized {}x{}, \
                             falling back to automatic placement",
                            name.connector,
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

            self.global_space.map_output(&output, new_position);

            // By passing new_output as an Option, rather than mapping it into a bogus location
            // in global_space, we ensure that this branch always runs for it.
            if Some(new_position) != position {
                debug!(
                    "putting output {} at x={} y={}",
                    name.connector, new_position.x, new_position.y
                );
                output.change_current_state(None, None, None, Some(new_position));
                self.ipc_outputs_changed = true;
                self.queue_redraw(&output);
            }
        }
    }

    pub fn add_output(&mut self, output: Output, refresh_interval: Option<Duration>, vrr: bool) {
        let global = output.create_global::<State>(&self.display_handle);

        let name = output.user_data().get::<OutputName>().unwrap();

        let config = self.config.borrow();
        let c = config.outputs.find(name);
        let scale = c.and_then(|c| c.scale).map(|s| s.0).unwrap_or_else(|| {
            let size_mm = output.physical_properties().size;
            let resolution = output.current_mode().unwrap().size;
            guess_monitor_scale(size_mm, resolution)
        });
        let scale = closest_representable_scale(scale.clamp(0.1, 10.));

        let mut transform = c
            .map(|c| ipc_transform_to_smithay(c.transform))
            .unwrap_or(Transform::Normal);

        let mut background_color = c
            .map(|c| c.background_color)
            .unwrap_or(DEFAULT_BACKGROUND_COLOR)
            .to_array_unpremul();
        background_color[3] = 1.;

        // FIXME: fix winit damage on other transforms.
        if name.connector == "winit" {
            transform = Transform::Flipped180;
        }
        drop(config);

        // Set scale and transform before adding to the layout since that will read the output size.
        output.change_current_state(
            None,
            Some(transform),
            Some(output::Scale::Fractional(scale)),
            None,
        );

        self.layout.add_output(output.clone());

        let lock_render_state = if self.is_locked() {
            // We haven't rendered anything yet so it's as good as locked.
            LockRenderState::Locked
        } else {
            LockRenderState::Unlocked
        };

        let size = output_size(&output).to_i32_round();
        let state = OutputState {
            global,
            redraw_state: RedrawState::Idle,
            on_demand_vrr_enabled: false,
            unfinished_animations_remain: false,
            frame_clock: FrameClock::new(refresh_interval, vrr),
            last_drm_sequence: None,
            frame_callback_sequence: 0,
            background_buffer: SolidColorBuffer::new(size, background_color),
            lock_render_state,
            lock_surface: None,
            lock_color_buffer: SolidColorBuffer::new(size, CLEAR_COLOR_LOCKED),
            screen_transition: None,
            debug_damage_tracker: OutputDamageTracker::from_output(&output),
        };
        let rv = self.output_state.insert(output.clone(), state);
        assert!(rv.is_none(), "output was already tracked");

        // Must be last since it will call queue_redraw(output) which needs things to be filled-in.
        self.reposition_outputs(Some(&output));
    }

    pub fn remove_output(&mut self, output: &Output) {
        for layer in layer_map_for_output(output).layers() {
            layer.layer_surface().send_close();
        }

        self.layout.remove_output(output);
        self.global_space.unmap_output(output);
        self.reposition_outputs(None);
        self.gamma_control_manager_state.output_removed(output);

        let state = self.output_state.remove(output).unwrap();

        match state.redraw_state {
            RedrawState::Idle => (),
            RedrawState::Queued => (),
            RedrawState::WaitingForVBlank { .. } => (),
            RedrawState::WaitingForEstimatedVBlank(token) => self.event_loop.remove(token),
            RedrawState::WaitingForEstimatedVBlankAndQueued(token) => self.event_loop.remove(token),
        }

        #[cfg(feature = "xdp-gnome-screencast")]
        self.stop_casts_for_target(CastTarget::Output(output.downgrade()));

        self.remove_screencopy_output(output);

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
                    let lock = confirmation.ext_session_lock().clone();
                    confirmation.lock();
                    self.lock_state = LockState::Locked(lock);
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

    pub fn output_resized(&mut self, output: &Output) {
        let output_size = output_size(output).to_i32_round();
        let scale = output.current_scale();
        let transform = output.current_transform();
        let is_locked = self.is_locked();

        {
            let mut layer_map = layer_map_for_output(output);
            for layer in layer_map.layers() {
                layer.with_surfaces(|surface, data| {
                    send_scale_transform(surface, data, scale, transform);
                });
            }
            layer_map.arrange();
        }

        self.layout.update_output_size(output);

        if let Some(state) = self.output_state.get_mut(output) {
            state.background_buffer.resize(output_size);

            state.lock_color_buffer.resize(output_size);
            if is_locked {
                if let Some(lock_surface) = &state.lock_surface {
                    configure_lock_surface(lock_surface, output);
                }
            }
        }

        // If the output size changed with an open screenshot UI, close the screenshot UI.
        if let Some((old_size, old_scale, old_transform)) = self.screenshot_ui.output_size(output) {
            let output_mode = output.current_mode().unwrap();
            let size = transform.transform_size(output_mode.size);
            let scale = output.current_scale().fractional_scale();
            // FIXME: scale changes and transform flips shouldn't matter but they currently do since
            // I haven't quite figured out how to draw the screenshot textures in
            // physical coordinates.
            if old_size != size || old_scale != scale || old_transform != transform {
                self.screenshot_ui.close();
                self.cursor_manager
                    .set_cursor_image(CursorImageStatus::default_named());
                self.queue_redraw_all();
                return;
            }
        }

        self.queue_redraw(output);
    }

    pub fn deactivate_monitors(&mut self, backend: &mut Backend) {
        if !self.monitors_active {
            return;
        }

        self.monitors_active = false;
        backend.set_monitors_active(false);
    }

    pub fn activate_monitors(&mut self, backend: &mut Backend) {
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

    /// Returns the window under the position to be activated.
    ///
    /// The cursor may be inside the window's activation region, but not within the window's input
    /// region.
    pub fn window_under(&self, pos: Point<f64, Logical>) -> Option<&Mapped> {
        if self.is_locked() || self.screenshot_ui.is_open() {
            return None;
        }

        let (output, pos_within_output) = self.output_under(pos)?;

        // Check if some layer-shell surface is on top.
        let layers = layer_map_for_output(output);
        let layer_under = |layer| layers.layer_under(layer, pos_within_output).is_some();
        if layer_under(Layer::Overlay) {
            return None;
        }

        let mon = self.layout.monitor_for_output(output).unwrap();
        if !mon.render_above_top_layer() && layer_under(Layer::Top) {
            return None;
        }

        let (window, _loc) = self.layout.window_under(output, pos_within_output)?;
        Some(window)
    }

    /// Returns the window under the cursor to be activated.
    ///
    /// The cursor may be inside the window's activation region, but not within the window's input
    /// region.
    pub fn window_under_cursor(&self) -> Option<&Mapped> {
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.window_under(pos)
    }

    /// Returns contents under the given point.
    ///
    /// We don't have a proper global space for all windows, so this function converts window
    /// locations to global space according to where they are rendered.
    ///
    /// This function does not take pointer or touch grabs into account.
    pub fn contents_under(&mut self, pos: Point<f64, Logical>) -> PointContents {
        let mut rv = PointContents::default();

        let Some((output, pos_within_output)) = self.output_under(pos) else {
            return rv;
        };
        rv.output = Some(output.clone());
        let output_pos_in_global_space = self.global_space.output_geometry(output).unwrap().loc;

        if self.is_locked() {
            let Some(state) = self.output_state.get(output) else {
                return rv;
            };
            let Some(surface) = state.lock_surface.as_ref() else {
                return rv;
            };

            rv.surface = under_from_surface_tree(
                surface.wl_surface(),
                pos_within_output,
                // We put lock surfaces at (0, 0).
                (0, 0),
                WindowSurfaceType::ALL,
            )
            .map(|(surface, pos_within_output)| {
                (
                    surface,
                    (pos_within_output + output_pos_in_global_space).to_f64(),
                )
            });

            return rv;
        }

        if self.screenshot_ui.is_open() {
            return rv;
        }

        let layers = layer_map_for_output(output);
        let layer_surface_under = |layer| {
            layers
                .layer_under(layer, pos_within_output)
                .and_then(|layer| {
                    let layer_pos_within_output =
                        layers.layer_geometry(layer).unwrap().loc.to_f64();
                    layer
                        .surface_under(
                            pos_within_output - layer_pos_within_output,
                            WindowSurfaceType::ALL,
                        )
                        .map(|(surface, pos_within_layer)| {
                            (
                                (surface, pos_within_layer.to_f64() + layer_pos_within_output),
                                layer,
                            )
                        })
                })
                .map(|(s, l)| (s, (None, Some(l.clone()))))
        };

        let window_under = || {
            self.layout
                .window_under(output, pos_within_output)
                .and_then(|(mapped, win_pos_within_output)| {
                    let win_pos_within_output = win_pos_within_output?;
                    let window = &mapped.window;
                    window
                        .surface_under(
                            pos_within_output - win_pos_within_output,
                            WindowSurfaceType::ALL,
                        )
                        .map(|(s, pos_within_window)| {
                            (s, pos_within_window.to_f64() + win_pos_within_output)
                        })
                        .map(|s| (s, (Some(window.clone()), None)))
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

        let Some(((surface, surface_pos_within_output), (window, layer))) = under
            .or_else(|| layer_surface_under(Layer::Bottom))
            .or_else(|| layer_surface_under(Layer::Background))
        else {
            return rv;
        };

        let surface_loc_in_global_space =
            surface_pos_within_output + output_pos_in_global_space.to_f64();

        rv.surface = Some((surface, surface_loc_in_global_space));
        rv.window = window;
        rv.layer = layer;
        rv
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

    pub fn find_output_and_workspace_index(
        &self,
        workspace_reference: WorkspaceReference,
    ) -> Option<(Option<Output>, usize)> {
        let (target_workspace_index, target_workspace) = match workspace_reference {
            WorkspaceReference::Index(index) => {
                return Some((None, index.saturating_sub(1) as usize));
            }
            WorkspaceReference::Name(name) => self.layout.find_workspace_by_name(&name)?,
            WorkspaceReference::Id(id) => {
                let id = WorkspaceId::specific(id);
                self.layout.find_workspace_by_id(id)?
            }
        };

        let target_output = target_workspace.current_output();
        Some((target_output.cloned(), target_workspace_index))
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
        map_to_output.and_then(|name| self.output_by_name_match(name))
    }

    pub fn output_for_touch(&self) -> Option<&Output> {
        let config = self.config.borrow();
        let map_to_output = config.input.touch.map_to_output.as_ref();
        map_to_output
            .and_then(|name| self.output_by_name_match(name))
            .or_else(|| self.global_space.outputs().next())
    }

    pub fn output_by_name_match(&self, target: &str) -> Option<&Output> {
        self.global_space
            .outputs()
            .find(|output| output_matches_name(output, target))
    }

    pub fn output_for_root(&self, root: &WlSurface) -> Option<&Output> {
        // Check the main layout.
        let win_out = self.layout.find_window_and_output(root);
        let layout_output = win_out.map(|(_, output)| output);

        // Check layer-shell.
        let has_layer_surface = |o: &&Output| {
            layer_map_for_output(o)
                .layer_for_surface(root, WindowSurfaceType::TOPLEVEL)
                .is_some()
        };
        let layer_shell_output = || self.layout.outputs().find(has_layer_surface);

        layout_output.or_else(layer_shell_output)
    }

    pub fn lock_surface_focus(&self) -> Option<WlSurface> {
        let output_under_cursor = self.output_under_cursor();
        let output = output_under_cursor
            .as_ref()
            .or_else(|| self.layout.active_output())
            .or_else(|| self.global_space.outputs().next())?;

        let state = self.output_state.get(output)?;
        state.lock_surface.as_ref().map(|s| s.wl_surface()).cloned()
    }

    /// Schedules an immediate redraw on all outputs if one is not already scheduled.
    pub fn queue_redraw_all(&mut self) {
        for state in self.output_state.values_mut() {
            state.redraw_state = mem::take(&mut state.redraw_state).queue_redraw();
        }
    }

    /// Schedules an immediate redraw if one is not already scheduled.
    pub fn queue_redraw(&mut self, output: &Output) {
        let state = self.output_state.get_mut(output).unwrap();
        state.redraw_state = mem::take(&mut state.redraw_state).queue_redraw();
    }

    pub fn redraw_queued_outputs(&mut self, backend: &mut Backend) {
        let _span = tracy_client::span!("Niri::redraw_queued_outputs");

        while let Some((output, _)) = self.output_state.iter().find(|(_, state)| {
            matches!(
                state.redraw_state,
                RedrawState::Queued | RedrawState::WaitingForEstimatedVBlankAndQueued(_)
            )
        }) {
            trace!("redrawing output");
            let output = output.clone();
            self.redraw(backend, &output);
        }
    }

    pub fn pointer_element<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
    ) -> Vec<OutputRenderElements<R>> {
        if self.pointer_hidden {
            return vec![];
        }

        let _span = tracy_client::span!("Niri::pointer_element");
        let output_scale = output.current_scale();
        let output_pos = self.global_space.output_geometry(output).unwrap().loc;

        // Check whether we need to draw the tablet cursor or the regular cursor.
        let pointer_pos = self
            .tablet_cursor_location
            .unwrap_or_else(|| self.seat.get_pointer().unwrap().current_location());
        let pointer_pos = pointer_pos - output_pos.to_f64();

        // Get the render cursor to draw.
        let cursor_scale = output_scale.integer_scale();
        let render_cursor = self.cursor_manager.get_render_cursor(cursor_scale);

        let output_scale = Scale::from(output.current_scale().fractional_scale());

        let mut pointer_elements = match render_cursor {
            RenderCursor::Hidden => vec![],
            RenderCursor::Surface { surface, hotspot } => {
                let pointer_pos =
                    (pointer_pos - hotspot.to_f64()).to_physical_precise_round(output_scale);

                render_elements_from_surface_tree(
                    renderer,
                    &surface,
                    pointer_pos,
                    output_scale,
                    1.,
                    Kind::Cursor,
                )
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

                let texture = self.cursor_texture_cache.get(icon, scale, &cursor, idx);
                let mut pointer_elements = vec![];
                let pointer_element = match MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    pointer_pos,
                    &texture,
                    None,
                    None,
                    None,
                    Kind::Cursor,
                ) {
                    Ok(element) => Some(element),
                    Err(err) => {
                        warn!("error importing a cursor texture: {err:?}");
                        None
                    }
                };
                if let Some(element) = pointer_element {
                    pointer_elements.push(OutputRenderElements::NamedPointer(element));
                }

                pointer_elements
            }
        };

        if let Some(dnd_icon) = self.dnd_icon.as_ref() {
            let pointer_pos =
                (pointer_pos + dnd_icon.offset.to_f64()).to_physical_precise_round(output_scale);
            pointer_elements.extend(render_elements_from_surface_tree(
                renderer,
                &dnd_icon.surface,
                pointer_pos,
                output_scale,
                1.,
                Kind::Unspecified,
            ));
        }

        pointer_elements
    }

    pub fn refresh_pointer_outputs(&mut self) {
        if self.pointer_hidden {
            return;
        }

        let _span = tracy_client::span!("Niri::refresh_pointer_outputs");

        // Check whether we need to draw the tablet cursor or the regular cursor.
        let pointer_pos = self
            .tablet_cursor_location
            .unwrap_or_else(|| self.seat.get_pointer().unwrap().current_location());

        match self.cursor_manager.cursor_image() {
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

                let surface_pos = pointer_pos.to_i32_round() - hotspot;
                let bbox = bbox_from_surface_tree(surface, surface_pos);

                let dnd = self
                    .dnd_icon
                    .as_ref()
                    .map(|icon| &icon.surface)
                    .map(|surface| (surface, bbox_from_surface_tree(surface, surface_pos)));

                // FIXME we basically need to pick the largest scale factor across the overlapping
                // outputs, this is how it's usually done in clients as well.
                let mut cursor_scale = 1.;
                let mut cursor_transform = Transform::Normal;
                let mut dnd_scale = 1.;
                let mut dnd_transform = Transform::Normal;
                for output in self.global_space.outputs() {
                    let geo = self.global_space.output_geometry(output).unwrap();

                    // Compute pointer surface overlap.
                    if let Some(mut overlap) = geo.intersection(bbox) {
                        overlap.loc -= surface_pos;
                        cursor_scale =
                            f64::max(cursor_scale, output.current_scale().fractional_scale());
                        // FIXME: using the largest overlapping or "primary" output transform would
                        // make more sense here.
                        cursor_transform = output.current_transform();
                        output_update(output, Some(overlap), surface);
                    } else {
                        output_update(output, None, surface);
                    }

                    // Compute DnD icon surface overlap.
                    if let Some((surface, bbox)) = dnd {
                        if let Some(mut overlap) = geo.intersection(bbox) {
                            overlap.loc -= surface_pos;
                            dnd_scale =
                                f64::max(dnd_scale, output.current_scale().fractional_scale());
                            // FIXME: using the largest overlapping or "primary" output transform
                            // would make more sense here.
                            dnd_transform = output.current_transform();
                            output_update(output, Some(overlap), surface);
                        } else {
                            output_update(output, None, surface);
                        }
                    }
                }

                with_states(surface, |data| {
                    send_scale_transform(
                        surface,
                        data,
                        output::Scale::Fractional(cursor_scale),
                        cursor_transform,
                    )
                });
                if let Some((surface, _)) = dnd {
                    with_states(surface, |data| {
                        send_scale_transform(
                            surface,
                            data,
                            output::Scale::Fractional(dnd_scale),
                            dnd_transform,
                        );
                    });
                }
            }
            cursor_image => {
                // There's no cursor surface, but there might be a DnD icon.
                let Some(surface) = self.dnd_icon.as_ref().map(|icon| &icon.surface) else {
                    return;
                };

                let icon = if let CursorImageStatus::Named(icon) = cursor_image {
                    *icon
                } else {
                    Default::default()
                };

                let mut dnd_scale = 1.;
                let mut dnd_transform = Transform::Normal;
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
                        dnd_scale = f64::max(dnd_scale, output.current_scale().fractional_scale());
                        // FIXME: using the largest overlapping or "primary" output transform would
                        // make more sense here.
                        dnd_transform = output.current_transform();
                        output_update(output, Some(overlap), surface);
                    } else {
                        output_update(output, None, surface);
                    }
                }

                with_states(surface, |data| {
                    send_scale_transform(
                        surface,
                        data,
                        output::Scale::Fractional(dnd_scale),
                        dnd_transform,
                    );
                });
            }
        }
    }

    pub fn refresh_layout(&mut self) {
        let layout_is_active = match &self.keyboard_focus {
            KeyboardFocus::Layout { .. } => true,
            KeyboardFocus::LayerShell { .. } => false,

            // Draw layout as active in these cases to reduce unnecessary window animations.
            // There's no confusion because these are both fullscreen modes.
            //
            // FIXME: when going into the screenshot UI from a layer-shell focus, and then back to
            // layer-shell, the layout will briefly draw as active, despite never having focus.
            KeyboardFocus::LockScreen { .. } => true,
            KeyboardFocus::ScreenshotUi => true,
        };

        self.layout.refresh(layout_is_active);
    }

    pub fn refresh_idle_inhibit(&mut self) {
        let _span = tracy_client::span!("Niri::refresh_idle_inhibit");

        self.idle_inhibiting_surfaces.retain(|s| s.is_alive());

        let is_inhibited = self.is_fdo_idle_inhibited.load(Ordering::SeqCst)
            || self.idle_inhibiting_surfaces.iter().any(|surface| {
                with_states(surface, |states| {
                    surface_primary_scanout_output(surface, states).is_some()
                })
            });
        self.idle_notifier_state.set_is_inhibited(is_inhibited);
    }

    pub fn refresh_window_rules(&mut self) {
        let _span = tracy_client::span!("Niri::refresh_window_rules");

        let config = self.config.borrow();
        let window_rules = &config.window_rules;

        let mut windows = vec![];
        let mut outputs = HashSet::new();
        self.layout.with_windows_mut(|mapped, output| {
            if mapped.recompute_window_rules_if_needed(window_rules, self.is_at_startup) {
                windows.push(mapped.window.clone());

                if let Some(output) = output {
                    outputs.insert(output.clone());
                }
            }
        });
        drop(config);

        for win in windows {
            self.layout.update_window(&win, None);
            win.toplevel()
                .expect("no X11 support")
                .send_pending_configure();
        }
        for output in outputs {
            self.queue_redraw(&output);
        }
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn refresh_mapped_cast_outputs(&mut self) {
        use std::collections::hash_map::Entry;

        let mut seen = HashSet::new();
        let mut output_changed = vec![];

        self.layout.with_windows(|mapped, output, _| {
            seen.insert(mapped.window.clone());

            let Some(output) = output else {
                return;
            };

            match self.mapped_cast_output.entry(mapped.window.clone()) {
                Entry::Occupied(mut entry) => {
                    if entry.get() != output {
                        entry.insert(output.clone());
                        output_changed.push((mapped.id(), output.clone()));
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(output.clone());
                }
            }
        });

        self.mapped_cast_output.retain(|win, _| seen.contains(win));

        let mut to_stop = vec![];
        for (id, out) in output_changed {
            let refresh = out.current_mode().unwrap().refresh as u32;
            let target = CastTarget::Window { id: id.get() };
            for cast in self.casts.iter_mut().filter(|cast| cast.target == target) {
                if let Err(err) = cast.set_refresh(refresh) {
                    warn!("error changing cast FPS: {err:?}");
                    to_stop.push(cast.session_id);
                };
            }
        }

        for session_id in to_stop {
            self.stop_cast(session_id);
        }
    }

    pub fn advance_animations(&mut self) {
        let _span = tracy_client::span!("Niri::advance_animations");

        self.layout.advance_animations();
        self.config_error_notification.advance_animations();
        self.screenshot_ui.advance_animations();

        for state in self.output_state.values_mut() {
            if let Some(transition) = &mut state.screen_transition {
                if transition.is_done() {
                    state.screen_transition = None;
                }
            }
        }
    }

    pub fn update_render_elements(&mut self, output: Option<&Output>) {
        self.layout.update_render_elements(output);

        for (out, state) in self.output_state.iter_mut() {
            if output.map_or(true, |output| out == output) {
                let scale = Scale::from(out.current_scale().fractional_scale());
                let transform = out.current_transform();

                if let Some(transition) = &mut state.screen_transition {
                    transition.update_render_elements(scale, transform);
                }
            }
        }
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
        include_pointer: bool,
        mut target: RenderTarget,
    ) -> Vec<OutputRenderElements<R>> {
        let _span = tracy_client::span!("Niri::render");

        if target == RenderTarget::Output {
            if let Some(preview) = self.config.borrow().debug.preview_render {
                target = match preview {
                    PreviewRender::Screencast => RenderTarget::Screencast,
                    PreviewRender::ScreenCapture => RenderTarget::ScreenCapture,
                };
            }
        }

        let output_scale = Scale::from(output.current_scale().fractional_scale());

        // The pointer goes on the top.
        let mut elements = vec![];
        if include_pointer {
            elements = self.pointer_element(renderer, output);
        }

        // Next, the screen transition texture.
        {
            let state = self.output_state.get(output).unwrap();
            if let Some(transition) = &state.screen_transition {
                elements.push(transition.render(target).into());
            }
        }

        // Next, the exit confirm dialog.
        if let Some(dialog) = &self.exit_confirm_dialog {
            if let Some(element) = dialog.render(renderer, output) {
                elements.push(element.into());
            }
        }

        // Next, the config error notification too.
        if let Some(element) = self.config_error_notification.render(renderer, output) {
            elements.push(element.into());
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

            if self.debug_draw_opaque_regions {
                draw_opaque_regions(&mut elements, output_scale);
            }
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
                    .render_output(output, target)
                    .into_iter()
                    .map(OutputRenderElements::from),
            );

            // Add the background for outputs that were connected while the screenshot UI was open.
            elements.push(background);

            if self.debug_draw_opaque_regions {
                draw_opaque_regions(&mut elements, output_scale);
            }
            return elements;
        }

        // Draw the hotkey overlay on top.
        if let Some(element) = self.hotkey_overlay.render(renderer, output) {
            elements.push(element.into());
        }

        // Get monitor elements.
        let mon = self.layout.monitor_for_output(output).unwrap();
        let monitor_elements: Vec<_> = mon.render_elements(renderer, target).collect();
        let float_elements: Vec<_> = self
            .layout
            .render_floating_for_output(renderer, output, target)
            .collect();

        // Get layer-shell elements.
        let layer_map = layer_map_for_output(output);
        let mut extend_from_layer = |elements: &mut Vec<OutputRenderElements<R>>, layer| {
            self.render_layer(renderer, target, output_scale, &layer_map, layer, elements);
        };

        // The upper layer-shell elements go next.
        extend_from_layer(&mut elements, Layer::Overlay);

        // Then the regular monitor elements and the top layer in varying order.
        if mon.render_above_top_layer() {
            elements.extend(float_elements.into_iter().map(OutputRenderElements::from));
            elements.extend(monitor_elements.into_iter().map(OutputRenderElements::from));
            extend_from_layer(&mut elements, Layer::Top);
        } else {
            extend_from_layer(&mut elements, Layer::Top);
            elements.extend(float_elements.into_iter().map(OutputRenderElements::from));
            elements.extend(monitor_elements.into_iter().map(OutputRenderElements::from));
        }

        // Then the lower layer-shell elements.
        extend_from_layer(&mut elements, Layer::Bottom);
        extend_from_layer(&mut elements, Layer::Background);

        // Then the background.
        elements.push(background);

        if self.debug_draw_opaque_regions {
            draw_opaque_regions(&mut elements, output_scale);
        }

        elements
    }

    fn render_layer<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        target: RenderTarget,
        scale: Scale<f64>,
        layer_map: &LayerMap,
        layer: Layer,
        elements: &mut Vec<OutputRenderElements<R>>,
    ) {
        let iter = layer_map
            .layers_on(layer)
            .filter_map(|surface| {
                let mapped = self.mapped_layer_surfaces.get(surface)?;
                let geo = layer_map.layer_geometry(surface)?;
                Some((mapped, geo))
            })
            .flat_map(|(mapped, geo)| {
                let elements = mapped.render(renderer, geo, scale, target);
                elements.into_iter().map(OutputRenderElements::LayerSurface)
            });
        elements.extend(iter);
    }

    fn redraw(&mut self, backend: &mut Backend, output: &Output) {
        let _span = tracy_client::span!("Niri::redraw");

        // Verify our invariant.
        let state = self.output_state.get_mut(output).unwrap();
        assert!(matches!(
            state.redraw_state,
            RedrawState::Queued | RedrawState::WaitingForEstimatedVBlankAndQueued(_)
        ));

        let target_presentation_time = state.frame_clock.next_presentation_time();

        // Freeze the clock at the target time.
        self.clock.set_unadjusted(target_presentation_time);

        self.update_render_elements(Some(output));

        let mut res = RenderResult::Skipped;
        if self.monitors_active {
            let state = self.output_state.get_mut(output).unwrap();
            state.unfinished_animations_remain = self.layout.are_animations_ongoing(Some(output));
            state.unfinished_animations_remain |=
                self.config_error_notification.are_animations_ongoing();
            state.unfinished_animations_remain |= self.screenshot_ui.are_animations_ongoing();
            state.unfinished_animations_remain |= state.screen_transition.is_some();

            // Also keep redrawing if the current cursor is animated.
            state.unfinished_animations_remain |= self
                .cursor_manager
                .is_current_cursor_animated(output.current_scale().integer_scale());

            // Render.
            res = backend.render(self, output, target_presentation_time);
        }

        let is_locked = self.is_locked();
        let state = self.output_state.get_mut(output).unwrap();

        if res == RenderResult::Skipped {
            // Update the redraw state on failed render.
            state.redraw_state = if let RedrawState::WaitingForEstimatedVBlank(token)
            | RedrawState::WaitingForEstimatedVBlankAndQueued(token) =
                state.redraw_state
            {
                RedrawState::WaitingForEstimatedVBlank(token)
            } else {
                RedrawState::Idle
            };
        }

        // Update the lock render state on successful render, or if monitors are inactive. When
        // monitors are inactive on a TTY, they have no framebuffer attached, so no sensitive data
        // from a last render will be visible.
        if res != RenderResult::Skipped || !self.monitors_active {
            state.lock_render_state = if is_locked {
                LockRenderState::Locked
            } else {
                LockRenderState::Unlocked
            };
        }

        // If we're in process of locking the session, check if the requirements were met.
        match mem::take(&mut self.lock_state) {
            LockState::Locking(confirmation) => {
                if state.lock_render_state == LockRenderState::Unlocked {
                    // We needed to render a locked frame on this output but failed.
                    self.unlock();
                } else {
                    // Check if all outputs are now locked.
                    let all_locked = self
                        .output_state
                        .values()
                        .all(|state| state.lock_render_state == LockRenderState::Locked);

                    if all_locked {
                        // All outputs are locked, report success.
                        let lock = confirmation.ext_session_lock().clone();
                        confirmation.lock();
                        self.lock_state = LockState::Locked(lock);
                    } else {
                        // Still waiting for other outputs.
                        self.lock_state = LockState::Locking(confirmation);
                    }
                }
            }
            lock_state => self.lock_state = lock_state,
        }

        self.refresh_on_demand_vrr(backend, output);

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
        backend.with_primary_renderer(|renderer| {
            #[cfg(feature = "xdp-gnome-screencast")]
            {
                // Render and send to PipeWire screencast streams.
                self.render_for_screen_cast(renderer, output, target_presentation_time);

                // FIXME: when a window is hidden, it should probably still receive frame callbacks
                // and get rendered for screen cast. This is currently
                // unimplemented, but happens to work by chance, since output
                // redrawing is more eager than it should be.
                self.render_windows_for_screen_cast(renderer, output, target_presentation_time);
            }

            self.render_for_screencopy_with_damage(renderer, output);
        });
    }

    pub fn refresh_on_demand_vrr(&mut self, backend: &mut Backend, output: &Output) {
        let _span = tracy_client::span!("Niri::refresh_on_demand_vrr");

        let name = output.user_data().get::<OutputName>().unwrap();
        let on_demand = self
            .config
            .borrow()
            .outputs
            .find(name)
            .map_or(false, |output| output.is_vrr_on_demand());
        if !on_demand {
            return;
        }

        let current = self.layout.windows_for_output(output).any(|mapped| {
            mapped.rules().variable_refresh_rate == Some(true) && {
                let mut visible = false;
                mapped.window.with_surfaces(|surface, states| {
                    if !visible
                        && surface_primary_scanout_output(surface, states).as_ref() == Some(output)
                    {
                        visible = true;
                    }
                });
                visible
            }
        });

        backend.set_output_on_demand_vrr(self, output, current);
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

        if let Some(surface) = self.dnd_icon.as_ref().map(|icon| &icon.surface) {
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

        for mapped in self.layout.windows_for_output(output) {
            let win = &mapped.window;
            let offscreen_id = win
                .user_data()
                .get_or_insert(WindowOffscreenId::default)
                .0
                .borrow();
            let offscreen_id = offscreen_id.as_ref();

            win.with_surfaces(|surface, states| {
                let surface_primary_scanout_output = states
                    .data_map
                    .get_or_insert_threadsafe(Mutex::<PrimaryScanoutOutput>::default);
                surface_primary_scanout_output
                    .lock()
                    .unwrap()
                    .update_from_render_element_states(
                        offscreen_id.cloned().unwrap_or_else(|| surface.into()),
                        output,
                        render_element_states,
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

    pub fn send_dmabuf_feedbacks(
        &self,
        output: &Output,
        feedback: &SurfaceDmabufFeedback,
        render_element_states: &RenderElementStates,
    ) {
        let _span = tracy_client::span!("Niri::send_dmabuf_feedbacks");

        // We can unconditionally send the current output's feedback to regular and layer-shell
        // surfaces, as they can only be displayed on a single output at a time. Even if a surface
        // is currently invisible, this is the DMABUF feedback that it should know about.
        for mapped in self.layout.windows_for_output(output) {
            mapped.window.send_dmabuf_feedback(
                output,
                |_, _| Some(output.clone()),
                |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_element_states,
                        &feedback.render,
                        &feedback.scanout,
                    )
                },
            );
        }

        for surface in layer_map_for_output(output).layers() {
            surface.send_dmabuf_feedback(
                output,
                |_, _| Some(output.clone()),
                |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_element_states,
                        &feedback.render,
                        &feedback.scanout,
                    )
                },
            );
        }

        if let Some(surface) = &self.output_state[output].lock_surface {
            send_dmabuf_feedback_surface_tree(
                surface.wl_surface(),
                output,
                |_, _| Some(output.clone()),
                |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_element_states,
                        &feedback.render,
                        &feedback.scanout,
                    )
                },
            );
        }

        if let Some(surface) = self.dnd_icon.as_ref().map(|icon| &icon.surface) {
            send_dmabuf_feedback_surface_tree(
                surface,
                output,
                surface_primary_scanout_output,
                |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_element_states,
                        &feedback.render,
                        &feedback.scanout,
                    )
                },
            );
        }

        if let CursorImageStatus::Surface(surface) = &self.cursor_manager.cursor_image() {
            send_dmabuf_feedback_surface_tree(
                surface,
                output,
                surface_primary_scanout_output,
                |surface, _| {
                    select_dmabuf_feedback(
                        surface,
                        render_element_states,
                        &feedback.render,
                        &feedback.scanout,
                    )
                },
            );
        }
    }

    pub fn send_frame_callbacks(&self, output: &Output) {
        let _span = tracy_client::span!("Niri::send_frame_callbacks");

        let state = self.output_state.get(output).unwrap();
        let sequence = state.frame_callback_sequence;

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
                if last_output == output && *last_sequence == sequence {
                    send = false;
                }
            }

            if send {
                *last_sent_at = Some((output.clone(), sequence));
                Some(output.clone())
            } else {
                None
            }
        };

        let frame_callback_time = get_monotonic_time();

        for mapped in self.layout.windows_for_output(output) {
            mapped.window.send_frame(
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                should_send,
            );
        }

        for surface in layer_map_for_output(output).layers() {
            surface.send_frame(
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                should_send,
            );
        }

        if let Some(surface) = &self.output_state[output].lock_surface {
            send_frames_surface_tree(
                surface.wl_surface(),
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                should_send,
            );
        }

        if let Some(surface) = self.dnd_icon.as_ref().map(|icon| &icon.surface) {
            send_frames_surface_tree(
                surface,
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                should_send,
            );
        }

        if let CursorImageStatus::Surface(surface) = self.cursor_manager.cursor_image() {
            send_frames_surface_tree(
                surface,
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                should_send,
            );
        }
    }

    pub fn send_frame_callbacks_on_fallback_timer(&self) {
        let _span = tracy_client::span!("Niri::send_frame_callbacks_on_fallback_timer");

        // Make up a bogus output; we don't care about it here anyway, just the throttling timer.
        let output = Output::new(
            String::new(),
            PhysicalProperties {
                size: Size::from((0, 0)),
                subpixel: Subpixel::Unknown,
                make: String::new(),
                model: String::new(),
            },
        );
        let output = &output;

        let frame_callback_time = get_monotonic_time();

        self.layout.with_windows(|mapped, _, _| {
            mapped.window.send_frame(
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                |_, _| None,
            );
        });

        for (output, state) in self.output_state.iter() {
            for surface in layer_map_for_output(output).layers() {
                surface.send_frame(
                    output,
                    frame_callback_time,
                    FRAME_CALLBACK_THROTTLE,
                    |_, _| None,
                );
            }

            if let Some(surface) = &state.lock_surface {
                send_frames_surface_tree(
                    surface.wl_surface(),
                    output,
                    frame_callback_time,
                    FRAME_CALLBACK_THROTTLE,
                    |_, _| None,
                );
            }
        }

        if let Some(surface) = &self.dnd_icon.as_ref().map(|icon| &icon.surface) {
            send_frames_surface_tree(
                surface,
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                |_, _| None,
            );
        }

        if let CursorImageStatus::Surface(surface) = self.cursor_manager.cursor_image() {
            send_frames_surface_tree(
                surface,
                output,
                frame_callback_time,
                FRAME_CALLBACK_THROTTLE,
                |_, _| None,
            );
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

        if let Some(surface) = self.dnd_icon.as_ref().map(|icon| &icon.surface) {
            take_presentation_feedback_surface_tree(
                surface,
                &mut feedback,
                surface_primary_scanout_output,
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        for mapped in self.layout.windows_for_output(output) {
            mapped.window.take_presentation_feedback(
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
    fn render_for_screen_cast(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
        target_presentation_time: Duration,
    ) {
        let _span = tracy_client::span!("Niri::render_for_screen_cast");

        let target = CastTarget::Output(output.downgrade());

        let size = output.current_mode().unwrap().size;
        let transform = output.current_transform();
        let size = transform.transform_size(size);

        let scale = Scale::from(output.current_scale().fractional_scale());

        let mut elements = None;
        let mut casts_to_stop = vec![];

        let mut casts = mem::take(&mut self.casts);
        for cast in &mut casts {
            if !cast.is_active.get() {
                continue;
            }

            if cast.target != target {
                continue;
            }

            match cast.ensure_size(size) {
                Ok(CastSizeChange::Ready) => (),
                Ok(CastSizeChange::Pending) => continue,
                Err(err) => {
                    warn!("error updating stream size, stopping screencast: {err:?}");
                    casts_to_stop.push(cast.session_id);
                }
            }

            if cast.check_time_and_schedule(&self.event_loop, output, target_presentation_time) {
                continue;
            }

            // FIXME: Hidden / embedded / metadata cursor
            let elements = elements.get_or_insert_with(|| {
                self.render(renderer, output, true, RenderTarget::Screencast)
            });

            if cast.dequeue_buffer_and_render(renderer, elements, size, scale) {
                cast.last_frame_time = target_presentation_time;
            }
        }
        self.casts = casts;

        for id in casts_to_stop {
            self.stop_cast(id);
        }
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    fn render_windows_for_screen_cast(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
        target_presentation_time: Duration,
    ) {
        let _span = tracy_client::span!("Niri::render_windows_for_screen_cast");

        let scale = Scale::from(output.current_scale().fractional_scale());

        let mut casts_to_stop = vec![];

        let mut casts = mem::take(&mut self.casts);
        for cast in &mut casts {
            if !cast.is_active.get() {
                continue;
            }

            let CastTarget::Window { id } = cast.target else {
                continue;
            };

            let mut windows = self.layout.windows_for_output(output);
            let Some(mapped) = windows.find(|win| win.id().get() == id) else {
                continue;
            };

            let bbox = mapped
                .window
                .bbox_with_popups()
                .to_physical_precise_up(scale);

            match cast.ensure_size(bbox.size) {
                Ok(CastSizeChange::Ready) => (),
                Ok(CastSizeChange::Pending) => continue,
                Err(err) => {
                    warn!("error updating stream size, stopping screencast: {err:?}");
                    casts_to_stop.push(cast.session_id);
                }
            }

            if cast.check_time_and_schedule(&self.event_loop, output, target_presentation_time) {
                continue;
            }

            // FIXME: pointer.
            let elements: Vec<_> = mapped.render_for_screen_cast(renderer, scale).collect();

            if cast.dequeue_buffer_and_render(renderer, &elements, bbox.size, scale) {
                cast.last_frame_time = target_presentation_time;
            }
        }
        self.casts = casts;

        for id in casts_to_stop {
            self.stop_cast(id);
        }
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    fn render_window_for_screen_cast(
        &mut self,
        renderer: &mut GlesRenderer,
        window_id: u64,
        target_presentation_time: Duration,
    ) {
        let _span = tracy_client::span!("Niri::render_window_for_screen_cast");

        let mut window = None;
        self.layout.with_windows(|mapped, _, _| {
            if mapped.id().get() != window_id {
                return;
            }

            window = Some(mapped.window.clone());
        });

        let Some(window) = window else {
            return;
        };

        // Use the cached output since it will be present even if the output was
        // currently disconnected.
        let Some(output) = self.mapped_cast_output.get(&window) else {
            return;
        };

        let mut windows = self.layout.windows_for_output(output);
        let mapped = windows
            .find(|mapped| mapped.id().get() == window_id)
            .unwrap();

        let scale = Scale::from(output.current_scale().fractional_scale());
        let bbox = mapped
            .window
            .bbox_with_popups()
            .to_physical_precise_up(scale);

        let mut elements = None;
        let mut casts_to_stop = vec![];

        let mut casts = mem::take(&mut self.casts);
        for cast in &mut casts {
            if !cast.is_active.get() {
                continue;
            }

            if cast.target != (CastTarget::Window { id: window_id }) {
                continue;
            }

            match cast.ensure_size(bbox.size) {
                Ok(CastSizeChange::Ready) => (),
                Ok(CastSizeChange::Pending) => continue,
                Err(err) => {
                    warn!("error updating stream size, stopping screencast: {err:?}");
                    casts_to_stop.push(cast.session_id);
                }
            }

            if cast.check_time_and_schedule(&self.event_loop, output, target_presentation_time) {
                continue;
            }

            let elements = elements.get_or_insert_with(|| {
                // FIXME: pointer.
                mapped
                    .render_for_screen_cast(renderer, scale)
                    .rev()
                    .collect::<Vec<_>>()
            });

            if cast.dequeue_buffer_and_render(renderer, elements, bbox.size, scale) {
                cast.last_frame_time = target_presentation_time;
            }
        }
        self.casts = casts;

        drop(windows);

        for id in casts_to_stop {
            self.stop_cast(id);
        }
    }

    pub fn render_for_screencopy_with_damage(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) {
        let _span = tracy_client::span!("Niri::render_for_screencopy_with_damage");

        let mut screencopy_state = mem::take(&mut self.screencopy_state);
        let elements = OnceCell::new();

        for queue in screencopy_state.queues_mut() {
            let (damage_tracker, screencopy) = queue.split();
            if let Some(screencopy) = screencopy {
                if screencopy.output() == output {
                    let elements = elements.get_or_init(|| {
                        self.render(renderer, output, true, RenderTarget::ScreenCapture)
                    });
                    // FIXME: skip elements if not including pointers
                    let render_result = Self::render_for_screencopy_internal(
                        renderer,
                        output,
                        elements,
                        true,
                        damage_tracker,
                        screencopy,
                    );
                    match render_result {
                        Ok((sync, damages)) => {
                            if let Some(damages) = damages {
                                // Convert from Physical coordinates back to Buffer coordinates.
                                let transform = output.current_transform();
                                let physical_size =
                                    transform.transform_size(screencopy.buffer_size());
                                let damages = damages.iter().map(|dmg| {
                                    dmg.to_logical(1).to_buffer(
                                        1,
                                        transform.invert(),
                                        &physical_size.to_logical(1),
                                    )
                                });

                                screencopy.damage(damages);
                                queue.pop().submit_after_sync(false, sync, &self.event_loop);
                            } else {
                                trace!("no damage found, waiting till next redraw");
                            }
                        }
                        Err(err) => {
                            // Recreate damage tracker to report full damage next check.
                            *damage_tracker =
                                OutputDamageTracker::new((0, 0), 1.0, Transform::Normal);
                            queue.pop();
                            warn!("error rendering for screencopy: {err:?}");
                        }
                    }
                };
            }
        }

        self.screencopy_state = screencopy_state;
    }

    pub fn render_for_screencopy_without_damage(
        &mut self,
        renderer: &mut GlesRenderer,
        manager: &ZwlrScreencopyManagerV1,
        screencopy: Screencopy,
    ) -> anyhow::Result<()> {
        let _span = tracy_client::span!("Niri::render_for_screencopy");

        let output = screencopy.output();
        ensure!(
            self.output_state.contains_key(output),
            "screencopy output missing"
        );

        self.update_render_elements(Some(output));

        let elements = self.render(
            renderer,
            output,
            screencopy.overlay_cursor(),
            RenderTarget::ScreenCapture,
        );
        let Some(queue) = self.screencopy_state.get_queue_mut(manager) else {
            bail!("screencopy manager destroyed already");
        };
        let damage_tracker = queue.split().0;

        let render_result = Self::render_for_screencopy_internal(
            renderer,
            output,
            &elements,
            false,
            damage_tracker,
            &screencopy,
        );

        let res = render_result
            .map(|(sync, _damage)| screencopy.submit_after_sync(false, sync, &self.event_loop));

        if res.is_err() {
            // Recreate damage tracker to report full damage next check.
            *damage_tracker = OutputDamageTracker::new((0, 0), 1.0, Transform::Normal);
        }

        res
    }

    #[allow(clippy::type_complexity)]
    fn render_for_screencopy_internal<'a>(
        renderer: &mut GlesRenderer,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
        with_damage: bool,
        damage_tracker: &'a mut OutputDamageTracker,
        screencopy: &Screencopy,
    ) -> anyhow::Result<(Option<SyncPoint>, Option<&'a Vec<Rectangle<i32, Physical>>>)> {
        let OutputModeSource::Static {
            size: last_size,
            scale: last_scale,
            transform: last_transform,
        } = damage_tracker.mode().clone()
        else {
            unreachable!("damage tracker must have static mode");
        };

        let size = screencopy.buffer_size();
        let scale: Scale<f64> = output.current_scale().fractional_scale().into();
        let transform = output.current_transform();

        if size != last_size || scale != last_scale || transform != last_transform {
            *damage_tracker = OutputDamageTracker::new(size, scale, transform);
        }

        let region_loc = screencopy.region_loc();
        let elements = elements
            .iter()
            .map(|element| {
                RelocateRenderElement::from_element(
                    element,
                    region_loc.upscale(-1),
                    Relocate::Relative,
                )
            })
            .collect::<Vec<_>>();

        // Just checked damage tracker has static mode
        let damages = damage_tracker.damage_output(1, &elements).unwrap().0;
        if with_damage && damages.is_none() {
            return Ok((None, None));
        }

        let elements = elements.iter().rev();

        let sync = match screencopy.buffer() {
            ScreencopyBuffer::Dmabuf(dmabuf) => {
                let sync =
                    render_to_dmabuf(renderer, dmabuf.clone(), size, scale, transform, elements)
                        .context("error rendering to screencopy dmabuf")?;
                Some(sync)
            }
            ScreencopyBuffer::Shm(wl_buffer) => {
                render_to_shm(renderer, wl_buffer, size, scale, transform, elements)
                    .context("error rendering to screencopy shm buffer")?;
                None
            }
        };

        if let Err(err) = renderer.unbind() {
            warn!("error unbinding after rendering for screencopy: {err:?}");
        }

        Ok((sync, damages))
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

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn stop_casts_for_target(&mut self, target: CastTarget) {
        let _span = tracy_client::span!("Niri::stop_casts_for_target");

        // This is O(N^2) but it shouldn't be a problem I think.
        let ids: Vec<_> = self
            .casts
            .iter()
            .filter(|cast| cast.target == target)
            .map(|cast| cast.session_id)
            .collect();

        for id in ids {
            self.stop_cast(id);
        }
    }

    pub fn remove_screencopy_output(&mut self, output: &Output) {
        let _span = tracy_client::span!("Niri::remove_screencopy_output");
        for queue in self.screencopy_state.queues_mut() {
            queue.remove_output(output);
        }
    }

    pub fn debug_toggle_damage(&mut self) {
        self.debug_draw_damage = !self.debug_draw_damage;

        if self.debug_draw_damage {
            for (output, state) in &mut self.output_state {
                state.debug_damage_tracker = OutputDamageTracker::from_output(output);
            }
        }

        self.queue_redraw_all();
    }

    pub fn capture_screenshots<'a>(
        &'a self,
        renderer: &'a mut GlesRenderer,
    ) -> impl Iterator<Item = (Output, [OutputScreenshot; 3])> + 'a {
        self.global_space.outputs().cloned().filter_map(|output| {
            let size = output.current_mode().unwrap().size;
            let transform = output.current_transform();
            let size = transform.transform_size(size);

            let scale = Scale::from(output.current_scale().fractional_scale());
            let targets = [
                RenderTarget::Output,
                RenderTarget::Screencast,
                RenderTarget::ScreenCapture,
            ];
            let screenshot = targets.map(|target| {
                let elements = self.render::<GlesRenderer>(renderer, &output, false, target);
                let elements = elements.iter().rev();

                let res = render_to_texture(
                    renderer,
                    size,
                    scale,
                    Transform::Normal,
                    Fourcc::Abgr8888,
                    elements,
                );
                if let Err(err) = &res {
                    warn!("error rendering output {}: {err:?}", output.name());
                }
                let res_output = res.ok();

                let pointer = self.pointer_element(renderer, &output);
                let res_pointer = if pointer.is_empty() {
                    None
                } else {
                    let res = render_to_encompassing_texture(
                        renderer,
                        scale,
                        Transform::Normal,
                        Fourcc::Abgr8888,
                        &pointer,
                    );
                    if let Err(err) = &res {
                        warn!("error rendering pointer for {}: {err:?}", output.name());
                    }
                    res.ok()
                };

                res_output.map(|(texture, _)| {
                    OutputScreenshot::from_textures(
                        renderer,
                        scale,
                        texture,
                        res_pointer.map(|(texture, _, geo)| (texture, geo)),
                    )
                })
            });

            if screenshot.iter().any(|res| res.is_none()) {
                return None;
            }

            let screenshot = screenshot.map(|res| res.unwrap());
            Some((output, screenshot))
        })
    }

    pub fn screenshot(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) -> anyhow::Result<()> {
        let _span = tracy_client::span!("Niri::screenshot");

        self.update_render_elements(Some(output));

        let size = output.current_mode().unwrap().size;
        let transform = output.current_transform();
        let size = transform.transform_size(size);

        let scale = Scale::from(output.current_scale().fractional_scale());
        let elements =
            self.render::<GlesRenderer>(renderer, output, true, RenderTarget::ScreenCapture);
        let elements = elements.iter().rev();
        let pixels = render_to_vec(
            renderer,
            size,
            scale,
            Transform::Normal,
            Fourcc::Abgr8888,
            elements,
        )?;

        self.save_screenshot(size, pixels)
            .context("error saving screenshot")
    }

    pub fn screenshot_window(
        &self,
        renderer: &mut GlesRenderer,
        output: &Output,
        mapped: &Mapped,
    ) -> anyhow::Result<()> {
        let _span = tracy_client::span!("Niri::screenshot_window");

        let scale = Scale::from(output.current_scale().fractional_scale());
        let alpha = if mapped.is_fullscreen() {
            1.
        } else {
            mapped.rules().opacity.unwrap_or(1.).clamp(0., 1.)
        };
        // FIXME: pointer.
        let elements = mapped.render(
            renderer,
            mapped.window.geometry().loc.to_f64(),
            scale,
            alpha,
            RenderTarget::ScreenCapture,
        );
        let geo = elements
            .iter()
            .map(|ele| ele.geometry(scale))
            .reduce(|a, b| a.merge(b))
            .unwrap_or_default();

        let elements = elements.iter().rev().map(|elem| {
            RelocateRenderElement::from_element(elem, geo.loc.upscale(-1), Relocate::Relative)
        });
        let pixels = render_to_vec(
            renderer,
            geo.size,
            scale,
            Transform::Normal,
            Fourcc::Abgr8888,
            elements,
        )?;

        self.save_screenshot(geo.size, pixels)
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

                if let Some(parent) = path.parent() {
                    if let Err(err) = std::fs::create_dir(parent) {
                        if err.kind() != std::io::ErrorKind::AlreadyExists {
                            warn!("error creating screenshot directory: {err:?}");
                        }
                    }
                }

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
        &mut self,
        renderer: &mut GlesRenderer,
        include_pointer: bool,
        on_done: impl FnOnce(PathBuf) + Send + 'static,
    ) -> anyhow::Result<()> {
        let _span = tracy_client::span!("Niri::screenshot_all_outputs");

        self.update_render_elements(None);

        let outputs: Vec<_> = self.global_space.outputs().cloned().collect();

        // FIXME: support multiple outputs, needs fixing multi-scale handling and cropping.
        anyhow::ensure!(outputs.len() == 1);

        let output = outputs.into_iter().next().unwrap();
        let geom = self.global_space.output_geometry(&output).unwrap();

        let output_scale = output.current_scale().integer_scale();
        let geom = geom.to_physical(output_scale);

        let size = geom.size;
        let transform = output.current_transform();
        let size = transform.transform_size(size);

        let elements = self.render::<GlesRenderer>(
            renderer,
            &output,
            include_pointer,
            RenderTarget::ScreenCapture,
        );
        let elements = elements.iter().rev();
        let pixels = render_to_vec(
            renderer,
            size,
            Scale::from(f64::from(output_scale)),
            Transform::Normal,
            Fourcc::Abgr8888,
            elements,
        )?;

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
        // Check if another client is in the process of locking.
        if matches!(self.lock_state, LockState::Locking(_)) {
            info!("refusing lock as another client is currently locking");
            return;
        }

        // Check if we're already locked with an active client.
        if let LockState::Locked(lock) = &self.lock_state {
            if lock.is_alive() {
                info!("refusing lock as already locked with an active client");
                return;
            }

            // If the client had died, continue with the new lock.
        }

        info!("locking session");

        self.screenshot_ui.close();
        self.cursor_manager
            .set_cursor_image(CursorImageStatus::default_named());

        if self.output_state.is_empty() {
            // There are no outputs, lock the session right away.
            let lock = confirmation.ext_session_lock().clone();
            confirmation.lock();
            self.lock_state = LockState::Locked(lock);
        } else {
            // There are outputs, which we need to redraw before locking.
            self.lock_state = LockState::Locking(confirmation);
            self.queue_redraw_all();
        }
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

    /// Activates the pointer constraint if necessary according to the current pointer contents.
    ///
    /// Make sure the pointer location and contents are up to date before calling this.
    pub fn maybe_activate_pointer_constraint(&self) {
        let pointer = self.seat.get_pointer().unwrap();
        let pointer_pos = pointer.current_location();

        let Some((surface, surface_loc)) = &self.pointer_contents.surface else {
            return;
        };
        if Some(surface) != pointer.current_focus().as_ref() {
            return;
        }

        let pointer = &self.seat.get_pointer().unwrap();
        with_pointer_constraint(surface, pointer, |constraint| {
            let Some(constraint) = constraint else { return };

            if constraint.is_active() {
                return;
            }

            // Constraint does not apply if not within region.
            if let Some(region) = constraint.region() {
                let pos_within_surface = pointer_pos - *surface_loc;
                if !region.contains(pos_within_surface.to_i32_round()) {
                    return;
                }
            }

            constraint.activate();
        });
    }

    pub fn focus_layer_surface_if_on_demand(&mut self, surface: Option<LayerSurface>) {
        if let Some(surface) = surface {
            if surface.cached_state().keyboard_interactivity
                == wlr_layer::KeyboardInteractivity::OnDemand
            {
                if self.layer_shell_on_demand_focus.as_ref() != Some(&surface) {
                    self.layer_shell_on_demand_focus = Some(surface);

                    // FIXME: granular.
                    self.queue_redraw_all();
                }

                return;
            }
        }

        // Something else got clicked, clear on-demand layer-shell focus.
        if self.layer_shell_on_demand_focus.is_some() {
            self.layer_shell_on_demand_focus = None;

            // FIXME: granular.
            self.queue_redraw_all();
        }
    }

    #[cfg(feature = "dbus")]
    pub fn on_ipc_outputs_changed(&self) {
        let _span = tracy_client::span!("Niri::on_ipc_outputs_changed");

        let Some(dbus) = &self.dbus else { return };
        let Some(conn_display_config) = dbus.conn_display_config.clone() else {
            return;
        };

        let res = thread::Builder::new()
            .name("DisplayConfig MonitorsChanged Emitter".to_owned())
            .spawn(move || {
                use crate::dbus::mutter_display_config::DisplayConfig;
                let _span = tracy_client::span!("MonitorsChanged");
                let iface = match conn_display_config
                    .object_server()
                    .interface::<_, DisplayConfig>("/org/gnome/Mutter/DisplayConfig")
                {
                    Ok(iface) => iface,
                    Err(err) => {
                        warn!("error getting DisplayConfig interface: {err:?}");
                        return;
                    }
                };

                async_io::block_on(async move {
                    if let Err(err) = DisplayConfig::monitors_changed(iface.signal_context()).await
                    {
                        warn!("error emitting MonitorsChanged: {err:?}");
                    }
                });
            });

        if let Err(err) = res {
            warn!("error spawning a thread to send MonitorsChanged: {err:?}");
        }
    }

    pub fn handle_focus_follows_mouse(&mut self, new_focus: &PointContents) {
        let Some(ffm) = self.config.borrow().input.focus_follows_mouse else {
            return;
        };

        let pointer = &self.seat.get_pointer().unwrap();
        if pointer.is_grabbed() {
            return;
        }

        // Recompute the current pointer focus because we don't update it during animations.
        let current_focus = self.contents_under(pointer.current_location());

        if let Some(output) = &new_focus.output {
            if current_focus.output.as_ref() != Some(output) {
                self.layout.focus_output(output);
            }
        }

        if let Some(window) = &new_focus.window {
            if current_focus.window.as_ref() != Some(window) {
                if !self.layout.should_trigger_focus_follows_mouse_on(window) {
                    return;
                }

                if let Some(threshold) = ffm.max_scroll_amount {
                    if self.layout.scroll_amount_to_activate(window) > threshold.0 {
                        return;
                    }
                }

                self.layout.activate_window(window);
                self.layer_shell_on_demand_focus = None;
            }
        }

        if let Some(layer) = &new_focus.layer {
            if current_focus.layer.as_ref() != Some(layer) {
                self.layer_shell_on_demand_focus = Some(layer.clone());
            }
        }
    }

    pub fn do_screen_transition(&mut self, renderer: &mut GlesRenderer, delay_ms: Option<u16>) {
        let _span = tracy_client::span!("Niri::do_screen_transition");

        self.update_render_elements(None);

        let textures: Vec<_> = self
            .output_state
            .keys()
            .cloned()
            .filter_map(|output| {
                let size = output.current_mode().unwrap().size;
                let transform = output.current_transform();

                let scale = Scale::from(output.current_scale().fractional_scale());
                let targets = [
                    RenderTarget::Output,
                    RenderTarget::Screencast,
                    RenderTarget::ScreenCapture,
                ];
                let textures = targets.map(|target| {
                    let elements = self.render::<GlesRenderer>(renderer, &output, false, target);
                    let elements = elements.iter().rev();

                    let res = render_to_texture(
                        renderer,
                        size,
                        scale,
                        transform,
                        Fourcc::Abgr8888,
                        elements,
                    );

                    if let Err(err) = &res {
                        warn!("error rendering output {}: {err:?}", output.name());
                    }

                    res
                });

                if textures.iter().any(|res| res.is_err()) {
                    return None;
                }

                let textures = textures.map(|res| {
                    let texture = res.unwrap().0;
                    TextureBuffer::from_texture(
                        renderer,
                        texture,
                        scale,
                        transform,
                        Vec::new(), // We want windows below to get frame callbacks.
                    )
                });

                Some((output, textures))
            })
            .collect();

        let delay = delay_ms.map_or(screen_transition::DELAY, |d| {
            Duration::from_millis(u64::from(d))
        });

        for (output, from_texture) in textures {
            let state = self.output_state.get_mut(&output).unwrap();
            state.screen_transition = Some(ScreenTransition::new(
                from_texture,
                delay,
                self.clock.clone(),
            ));
        }

        // We don't actually need to queue a redraw because the point is to freeze the screen for a
        // bit, and even if the delay was zero, we're drawing the same contents anyway.
    }

    pub fn recompute_window_rules(&mut self) {
        let _span = tracy_client::span!("Niri::recompute_window_rules");

        let changed = {
            let window_rules = &self.config.borrow().window_rules;

            for unmapped in self.unmapped_windows.values_mut() {
                let new_rules = ResolvedWindowRules::compute(
                    window_rules,
                    WindowRef::Unmapped(unmapped),
                    self.is_at_startup,
                );
                if let InitialConfigureState::Configured { rules, .. } = &mut unmapped.state {
                    *rules = new_rules;
                }
            }

            let mut windows = vec![];
            self.layout.with_windows_mut(|mapped, _| {
                if mapped.recompute_window_rules(window_rules, self.is_at_startup) {
                    windows.push(mapped.window.clone());
                }
            });
            let changed = !windows.is_empty();
            for win in windows {
                self.layout.update_window(&win, None);
            }
            changed
        };

        if changed {
            // FIXME: granular.
            self.queue_redraw_all();
        }
    }

    pub fn recompute_layer_rules(&mut self) {
        let _span = tracy_client::span!("Niri::recompute_layer_rules");

        let mut changed = false;
        {
            let rules = &self.config.borrow().layer_rules;

            for mapped in self.mapped_layer_surfaces.values_mut() {
                if mapped.recompute_layer_rules(rules, self.is_at_startup) {
                    changed = true;
                }
            }
        }

        if changed {
            // FIXME: granular.
            self.queue_redraw_all();
        }
    }

    pub fn reset_pointer_inactivity_timer(&mut self) {
        let _span = tracy_client::span!("Niri::reset_pointer_inactivity_timer");

        if let Some(token) = self.pointer_inactivity_timer.take() {
            self.event_loop.remove(token);
        }

        let Some(timeout_ms) = self.config.borrow().cursor.hide_after_inactive_ms else {
            return;
        };

        let duration = Duration::from_millis(timeout_ms as u64);
        let timer = Timer::from_duration(duration);
        let token = self
            .event_loop
            .insert_source(timer, move |_, _, state| {
                state.niri.pointer_inactivity_timer = None;
                state.niri.pointer_hidden = true;
                state.niri.queue_redraw_all();

                TimeoutAction::Drop
            })
            .unwrap();
        self.pointer_inactivity_timer = Some(token);
    }
}

pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub can_view_decoration_globals: bool,
    /// Whether this client is denied from the restricted protocols such as security-context.
    pub restricted: bool,
    /// We cannot retrieve this client's socket credentials.
    pub credentials_unknown: bool,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

niri_render_elements! {
    OutputRenderElements<R> => {
        Monitor = MonitorRenderElement<R>,
        Tile = TileRenderElement<R>,
        LayerSurface = LayerSurfaceRenderElement<R>,
        Wayland = WaylandSurfaceRenderElement<R>,
        NamedPointer = MemoryRenderBufferRenderElement<R>,
        SolidColor = SolidColorRenderElement,
        ScreenshotUi = ScreenshotUiRenderElement,
        Texture = PrimaryGpuTextureRenderElement,
        // Used for the CPU-rendered panels.
        RelocatedMemoryBuffer = RelocateRenderElement<MemoryRenderBufferRenderElement<R>>,
    }
}
