use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, mem, thread};

use _server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDecorationsMode;
use anyhow::Context;
use sd_notify::NotifyState;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::{
    default_primary_scanout_output_compare, render_elements, AsRenderElements, Kind, RenderElement,
    RenderElementStates,
};
use smithay::backend::renderer::gles::{GlesMapping, GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ExportMem, Frame, ImportAll, Offscreen, Renderer};
use smithay::desktop::utils::{
    bbox_from_surface_tree, output_update, send_dmabuf_feedback_surface_tree,
    send_frames_surface_tree, surface_presentation_feedback_flags_from_states,
    surface_primary_scanout_output, take_presentation_feedback_surface_tree,
    update_surface_primary_scanout_output, OutputPresentationFeedback,
};
use smithay::desktop::{layer_map_for_output, PopupManager, Space, Window, WindowSurfaceType};
use smithay::input::keyboard::XkbConfig;
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus, MotionEvent};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{
    self, Idle, Interest, LoopHandle, LoopSignal, Mode, PostAction, RegistrationToken,
};
use smithay::reexports::nix::libc::CLOCK_MONOTONIC;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities;
use smithay::reexports::wayland_protocols_misc::server_decoration as _server_decoration;
use smithay::reexports::wayland_server::backend::{
    ClientData, ClientId, DisconnectReason, GlobalId,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{
    IsAlive, Logical, Physical, Point, Rectangle, Scale, Size, Transform, SERIAL_COUNTER,
};
use smithay::wayland::compositor::{
    with_states, with_surface_tree_downward, CompositorClientState, CompositorState, SurfaceData,
    TraversalAction,
};
use smithay::wayland::dmabuf::DmabufFeedback;
use smithay::wayland::input_method::InputMethodManagerState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_gestures::PointerGesturesState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState;
use smithay::wayland::shell::kde::decoration::KdeDecorationState;
use smithay::wayland::shell::wlr_layer::{Layer, WlrLayerShellState};
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::tablet_manager::TabletManagerState;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState;
use zbus::fdo::RequestNameFlags;

use crate::backend::{Backend, Tty, Winit};
use crate::config::Config;
use crate::cursor::Cursor;
use crate::dbus::gnome_shell_screenshot::{self, NiriToScreenshot, ScreenshotToNiri};
use crate::dbus::mutter_display_config::DisplayConfig;
#[cfg(feature = "xdp-gnome-screencast")]
use crate::dbus::mutter_screen_cast::{self, ScreenCast, ToNiriMsg};
use crate::dbus::mutter_service_channel::ServiceChannel;
use crate::frame_clock::FrameClock;
use crate::layout::{output_size, Layout, MonitorRenderElement};
use crate::pw_utils::{Cast, PipeWire};
use crate::utils::{center, get_monotonic_time, make_screenshot_path};

pub struct Niri {
    pub config: Rc<RefCell<Config>>,

    pub event_loop: LoopHandle<'static, State>,
    pub stop_signal: LoopSignal,
    pub display_handle: DisplayHandle,

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

    // Smithay state.
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub kde_decoration_state: KdeDecorationState,
    pub layer_shell_state: WlrLayerShellState,
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

    pub default_cursor: Cursor,
    pub cursor_image: CursorImageStatus,
    pub dnd_icon: Option<WlSurface>,

    pub zbus_conn: Option<zbus::blocking::Connection>,
    pub inhibit_power_key_fd: Option<zbus::zvariant::OwnedFd>,
    #[cfg(feature = "xdp-gnome-screencast")]
    pub screen_cast: ScreenCast,

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

    pub fn move_cursor(&mut self, location: Point<f64, Logical>) {
        let under = self.niri.surface_under_and_global_space(location);
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

    pub fn move_cursor_to_output(&mut self, output: &Output) {
        let geo = self.niri.global_space.output_geometry(output).unwrap();
        self.move_cursor(center(geo).to_f64());
    }

    pub fn update_focus(&mut self) {
        let focus = self.niri.layer_surface_focus().or_else(|| {
            self.niri
                .layout
                .focus()
                .map(|win| win.toplevel().wl_surface().clone())
        });
        let keyboard = self.niri.seat.get_keyboard().unwrap();
        if keyboard.current_focus() != focus {
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
            self.niri.default_cursor =
                Cursor::load(&config.cursor.xcursor_theme, config.cursor.xcursor_size);
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
}

impl Niri {
    pub fn new(
        config: Rc<RefCell<Config>>,
        event_loop: LoopHandle<'static, State>,
        stop_signal: LoopSignal,
        display: Display<State>,
        backend: &Backend,
    ) -> Self {
        let display_handle = display.handle();
        let config_ = config.borrow();

        let layout = Layout::new(&config_);

        let compositor_state = CompositorState::new::<State>(&display_handle);
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
            PresentationState::new::<State>(&display_handle, CLOCK_MONOTONIC as u32);

        let text_input_state = TextInputManagerState::new::<State>(&display_handle);
        let input_method_state = InputMethodManagerState::new::<State>(&display_handle);
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

        let default_cursor =
            Cursor::load(&config_.cursor.xcursor_theme, config_.cursor.xcursor_size);

        let socket_source = ListeningSocketSource::new_auto().unwrap();
        let socket_name = socket_source.socket_name().to_os_string();
        event_loop
            .insert_source(socket_source, move |client, _, state| {
                if let Err(err) = state
                    .niri
                    .display_handle
                    .insert_client(client, Arc::new(ClientState::default()))
                {
                    error!("error inserting client: {err}");
                }
            })
            .unwrap();
        std::env::set_var("WAYLAND_DISPLAY", &socket_name);
        info!(
            "listening on Wayland socket: {}",
            socket_name.to_string_lossy()
        );

        let pipewire = match PipeWire::new(&event_loop) {
            Ok(pipewire) => Some(pipewire),
            Err(err) => {
                warn!("error starting PipeWire: {err:?}");
                None
            }
        };

        #[cfg(feature = "xdp-gnome-screencast")]
        let (to_niri, from_screen_cast) = calloop::channel::channel();
        #[cfg(feature = "xdp-gnome-screencast")]
        event_loop
            .insert_source(from_screen_cast, {
                let to_niri = to_niri.clone();
                move |event, _, state| match event {
                    calloop::channel::Event::Msg(msg) => match msg {
                        ToNiriMsg::StartCast {
                            session_id,
                            output,
                            cursor_mode,
                            signal_ctx,
                        } => {
                            let _span = tracy_client::span!("StartCast");

                            debug!(session_id, "StartCast");

                            let gbm = match state.backend.gbm_device() {
                                Some(gbm) => gbm,
                                None => {
                                    debug!("no GBM device available");
                                    return;
                                }
                            };

                            let pw = state.niri.pipewire.as_ref().unwrap();
                            match pw.start_cast(
                                to_niri.clone(),
                                gbm,
                                session_id,
                                output,
                                cursor_mode,
                                signal_ctx,
                            ) {
                                Ok(cast) => {
                                    state.niri.casts.push(cast);
                                }
                                Err(err) => {
                                    warn!("error starting screencast: {err:?}");

                                    if let Err(err) =
                                        to_niri.send(ToNiriMsg::StopCast { session_id })
                                    {
                                        warn!("error sending StopCast to niri: {err:?}");
                                    }
                                }
                            }
                        }
                        ToNiriMsg::StopCast { session_id } => {
                            let _span = tracy_client::span!("StopCast");

                            debug!(session_id, "StopCast");

                            for i in (0..state.niri.casts.len()).rev() {
                                let cast = &state.niri.casts[i];
                                if cast.session_id != session_id {
                                    continue;
                                }

                                let cast = state.niri.casts.swap_remove(i);
                                if let Err(err) = cast.stream.disconnect() {
                                    warn!("error disconnecting stream: {err:?}");
                                }
                            }

                            let server = state.niri.zbus_conn.as_ref().unwrap().object_server();
                            let path =
                                format!("/org/gnome/Mutter/ScreenCast/Session/u{}", session_id);
                            if let Ok(iface) =
                                server.interface::<_, mutter_screen_cast::Session>(path)
                            {
                                let _span = tracy_client::span!("invoking Session::stop");

                                async_io::block_on(async move {
                                    iface
                                        .get()
                                        .stop(&server, iface.signal_context().clone())
                                        .await
                                });
                            }
                        }
                    },
                    calloop::channel::Event::Closed => (),
                }
            })
            .unwrap();
        #[cfg(feature = "xdp-gnome-screencast")]
        let screen_cast = ScreenCast::new(backend.connectors(), to_niri);

        let (to_niri, from_screenshot) = calloop::channel::channel();
        let (to_screenshot, from_niri) = async_channel::unbounded();
        event_loop
            .insert_source(from_screenshot, move |event, _, state| match event {
                calloop::channel::Event::Msg(ScreenshotToNiri::TakeScreenshot {
                    include_cursor,
                }) => {
                    let Some(renderer) = state.backend.renderer() else {
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

                    let res = state
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
                calloop::channel::Event::Closed => (),
            })
            .unwrap();
        let screenshot = gnome_shell_screenshot::Screenshot::new(to_niri, from_niri);

        let mut zbus_conn = None;
        let mut inhibit_power_key_fd = None;
        if std::env::var_os("NOTIFY_SOCKET").is_some() {
            // We're starting as a systemd service. Export our variables and tell systemd we're
            // ready.
            let rv = Command::new("/bin/sh")
                .args([
                    "-c",
                    "systemctl --user import-environment WAYLAND_DISPLAY && \
                     hash dbus-update-activation-environment 2>/dev/null && \
                     dbus-update-activation-environment WAYLAND_DISPLAY",
                ])
                .spawn();
            // Wait for the import process to complete, otherwise services will start too fast
            // without environment variables available.
            match rv {
                Ok(mut child) => match child.wait() {
                    Ok(status) => {
                        if !status.success() {
                            warn!("import environment shell exited with {status}");
                        }
                    }
                    Err(err) => {
                        warn!("error waiting for import environment shell: {err:?}");
                    }
                },
                Err(err) => {
                    warn!("error spawning shell to import environment into systemd: {err:?}");
                }
            }

            // Set up zbus, make sure it happens before anything might want it.
            let conn = zbus::blocking::ConnectionBuilder::session()
                .unwrap()
                .name("org.gnome.Mutter.ServiceChannel")
                .unwrap()
                .serve_at(
                    "/org/gnome/Mutter/ServiceChannel",
                    ServiceChannel::new(display_handle.clone()),
                )
                .unwrap()
                .build()
                .unwrap();

            {
                let server = conn.object_server();
                let flags = RequestNameFlags::AllowReplacement
                    | RequestNameFlags::ReplaceExisting
                    | RequestNameFlags::DoNotQueue;

                server
                    .at("/org/gnome/Shell/Screenshot", screenshot)
                    .unwrap();
                conn.request_name_with_flags("org.gnome.Shell.Screenshot", flags)
                    .unwrap();

                server
                    .at(
                        "/org/gnome/Mutter/DisplayConfig",
                        DisplayConfig::new(backend.connectors()),
                    )
                    .unwrap();
                conn.request_name_with_flags("org.gnome.Mutter.DisplayConfig", flags)
                    .unwrap();

                #[cfg(feature = "xdp-gnome-screencast")]
                if pipewire.is_some() {
                    server
                        .at("/org/gnome/Mutter/ScreenCast", screen_cast.clone())
                        .unwrap();
                    conn.request_name_with_flags("org.gnome.Mutter.ScreenCast", flags)
                        .unwrap();
                }
            }

            zbus_conn = Some(conn);

            // Notify systemd we're ready.
            if let Err(err) = sd_notify::notify(true, &[NotifyState::Ready]) {
                warn!("error notifying systemd: {err:?}");
            };

            // Inhibit power key handling so we can suspend on it.
            let zbus_system_conn = zbus::blocking::ConnectionBuilder::system()
                .unwrap()
                .build()
                .unwrap();

            // logind-zbus has a wrong signature for this method, so do it manually.
            // https://gitlab.com/flukejones/logind-zbus/-/merge_requests/5
            let message = zbus_system_conn
                .call_method(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    Some("org.freedesktop.login1.Manager"),
                    "Inhibit",
                    &("handle-power-key", "niri", "Power key handling", "block"),
                )
                .unwrap();
            match message.body() {
                Ok(fd) => {
                    inhibit_power_key_fd = Some(fd);
                }
                Err(err) => {
                    warn!("error inhibiting power key: {err:?}");
                }
            }
        } else if config_.debug.dbus_interfaces_in_non_session_instances {
            let conn = zbus::blocking::Connection::session().unwrap();
            let flags = RequestNameFlags::AllowReplacement
                | RequestNameFlags::ReplaceExisting
                | RequestNameFlags::DoNotQueue;

            {
                let server = conn.object_server();

                server
                    .at("/org/gnome/Shell/Screenshot", screenshot)
                    .unwrap();
                conn.request_name_with_flags("org.gnome.Shell.Screenshot", flags)
                    .unwrap();

                server
                    .at(
                        "/org/gnome/Mutter/DisplayConfig",
                        DisplayConfig::new(backend.connectors()),
                    )
                    .unwrap();
                conn.request_name_with_flags("org.gnome.Mutter.DisplayConfig", flags)
                    .unwrap();

                #[cfg(feature = "xdp-gnome-screencast")]
                if pipewire.is_some() {
                    server
                        .at("/org/gnome/Mutter/ScreenCast", screen_cast.clone())
                        .unwrap();
                    conn.request_name_with_flags("org.gnome.Mutter.ScreenCast", flags)
                        .unwrap();
                }
            }

            zbus_conn = Some(conn);
        }

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
            display_handle,

            layout,
            global_space: Space::default(),
            output_state: HashMap::new(),
            output_by_name: HashMap::new(),
            unmapped_windows: HashMap::new(),

            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            kde_decoration_state,
            layer_shell_state,
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
            presentation_state,

            seat,
            default_cursor,
            cursor_image: CursorImageStatus::default_named(),
            dnd_icon: None,

            zbus_conn,
            inhibit_power_key_fd,
            #[cfg(feature = "xdp-gnome-screencast")]
            screen_cast,
            pipewire,
            casts: vec![],
        }
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

        let state = OutputState {
            global,
            redraw_state: RedrawState::Idle,
            unfinished_animations_remain: false,
            frame_clock: FrameClock::new(refresh_interval),
            current_estimated_sequence: None,
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
    }

    pub fn output_resized(&mut self, output: Output) {
        layer_map_for_output(&output).arrange();
        self.layout.update_output_size(&output);
        self.queue_redraw(output);
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
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        let (output, pos_within_output) = self.output_under(pos)?;
        let (window, win_pos_within_output) =
            self.layout.window_under(output, pos_within_output)?;

        let (surface, surface_pos_within_output) = window
            .surface_under(
                pos_within_output - win_pos_within_output.to_f64(),
                WindowSurfaceType::ALL,
            )
            .map(|(s, pos_within_window)| (s, pos_within_window + win_pos_within_output))?;
        let output_pos_in_global_space = self.global_space.output_geometry(output).unwrap().loc;
        let surface_loc_in_global_space = surface_pos_within_output + output_pos_in_global_space;

        Some((surface, surface_loc_in_global_space))
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
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) -> Vec<OutputRenderElements<GlesRenderer>> {
        let output_scale = Scale::from(output.current_scale().fractional_scale());
        let output_pos = self.global_space.output_geometry(output).unwrap().loc;
        let pointer_pos = self.seat.get_pointer().unwrap().current_location() - output_pos.to_f64();

        let output_scale_int = output.current_scale().integer_scale();
        let (default_buffer, default_hotspot) = self.default_cursor.get(renderer, output_scale_int);
        let default_hotspot = default_hotspot.to_logical(output_scale_int);

        let hotspot = if let CursorImageStatus::Surface(surface) = &mut self.cursor_image {
            if surface.alive() {
                with_states(surface, |states| {
                    states
                        .data_map
                        .get::<Mutex<CursorImageAttributes>>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .hotspot
                })
            } else {
                self.cursor_image = CursorImageStatus::default_named();
                default_hotspot
            }
        } else {
            default_hotspot
        };
        let pointer_pos = (pointer_pos - hotspot.to_f64()).to_physical_precise_round(output_scale);

        let mut pointer_elements = match &self.cursor_image {
            CursorImageStatus::Hidden => vec![],
            CursorImageStatus::Surface(surface) => render_elements_from_surface_tree(
                renderer,
                surface,
                pointer_pos,
                output_scale,
                1.,
                Kind::Cursor,
            ),
            // Default shape catch-all
            _ => vec![OutputRenderElements::DefaultPointer(
                TextureRenderElement::from_texture_buffer(
                    pointer_pos.to_f64(),
                    &default_buffer,
                    None,
                    None,
                    None,
                    Kind::Cursor,
                ),
            )],
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

    pub fn refresh_pointer_outputs(&self) {
        let _span = tracy_client::span!("Niri::refresh_pointer_outputs");

        match &self.cursor_image {
            CursorImageStatus::Hidden | CursorImageStatus::Named(_) => {
                // There's no cursor surface, but there might be a DnD icon.
                let Some(surface) = &self.dnd_icon else {
                    return;
                };

                let pointer_pos = self.seat.get_pointer().unwrap().current_location();

                for output in self.global_space.outputs() {
                    let geo = self.global_space.output_geometry(output).unwrap();

                    // The default cursor is rendered at the right scale for each output, which
                    // means that it may have a different hotspot for each output.
                    let output_scale = output.current_scale().integer_scale();
                    let Some(hotspot) = self.default_cursor.get_cached_hotspot(output_scale) else {
                        // Oh well; it'll get cached next time we render.
                        continue;
                    };
                    let hotspot = hotspot.to_logical(output_scale);

                    let surface_pos = pointer_pos.to_i32_round() - hotspot;
                    let bbox = bbox_from_surface_tree(surface, surface_pos);

                    if let Some(mut overlap) = geo.intersection(bbox) {
                        overlap.loc -= surface_pos;
                        output_update(output, Some(overlap), surface);
                    } else {
                        output_update(output, None, surface);
                    }
                }
            }
            CursorImageStatus::Surface(surface) => {
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

                for output in self.global_space.outputs() {
                    let geo = self.global_space.output_geometry(output).unwrap();

                    // Compute pointer surface overlap.
                    if let Some(mut overlap) = geo.intersection(bbox) {
                        overlap.loc -= surface_pos;
                        output_update(output, Some(overlap), surface);
                    } else {
                        output_update(output, None, surface);
                    }

                    // Compute DnD icon surface overlap.
                    if let Some((surface, bbox)) = dnd {
                        if let Some(mut overlap) = geo.intersection(bbox) {
                            overlap.loc -= surface_pos;
                            output_update(output, Some(overlap), surface);
                        } else {
                            output_update(output, None, surface);
                        }
                    }
                }
            }
        }
    }

    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
        include_pointer: bool,
    ) -> Vec<OutputRenderElements<GlesRenderer>> {
        let _span = tracy_client::span!("Niri::render");

        let output_scale = Scale::from(output.current_scale().fractional_scale());

        // Get monitor elements.
        let mon = self.layout.monitor_for_output(output).unwrap();
        let monitor_elements = mon.render_elements(renderer);

        // The pointer goes on the top.
        let mut elements = vec![];
        if include_pointer {
            elements = self.pointer_element(renderer, output);
        }

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
        // FIXME: hide top layer when a fullscreen surface is showing somehow.
        extend_from_layer(&mut elements, Layer::Top);

        // Then the regular monitor elements.
        elements.extend(monitor_elements.into_iter().map(OutputRenderElements::from));

        // Then the lower layer-shell elements.
        extend_from_layer(&mut elements, Layer::Bottom);
        extend_from_layer(&mut elements, Layer::Background);

        elements
    }

    fn redraw(&mut self, backend: &mut Backend, output: &Output) {
        let _span = tracy_client::span!("Niri::redraw");

        if !backend.is_active() {
            return;
        }

        let Some(renderer) = backend.renderer() else {
            return;
        };

        let state = self.output_state.get_mut(output).unwrap();
        assert!(matches!(
            state.redraw_state,
            RedrawState::Queued(_) | RedrawState::WaitingForEstimatedVBlankAndQueued(_)
        ));

        let presentation_time = state.frame_clock.next_presentation_time();

        // Update from the config and advance the animations.
        self.layout.advance_animations(presentation_time);
        state.unfinished_animations_remain = self
            .layout
            .monitor_for_output(output)
            .unwrap()
            .are_animations_ongoing();

        // Render the elements.
        let elements = self.render(renderer, output, true);

        // Hand it over to the backend.
        let dmabuf_feedback = backend.render(self, output, &elements, presentation_time);

        // Send the dmabuf feedbacks.
        if let Some(feedback) = dmabuf_feedback {
            self.send_dmabuf_feedbacks(output, feedback);
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
        if let CursorImageStatus::Surface(surface) = &self.cursor_image {
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
    }

    fn send_dmabuf_feedbacks(&self, output: &Output, feedback: &DmabufFeedback) {
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

        if let Some(surface) = &self.dnd_icon {
            send_dmabuf_feedback_surface_tree(
                surface,
                output,
                surface_primary_scanout_output,
                |_, _| feedback,
            );
        }

        if let CursorImageStatus::Surface(surface) = &self.cursor_image {
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

        if let Some(surface) = &self.dnd_icon {
            send_frames_surface_tree(surface, output, frame_callback_time, None, should_send);
        }

        if let CursorImageStatus::Surface(surface) = &self.cursor_image {
            send_frames_surface_tree(surface, output, frame_callback_time, None, should_send);
        }
    }

    pub fn take_presentation_feedbacks(
        &mut self,
        output: &Output,
        render_element_states: &RenderElementStates,
    ) -> OutputPresentationFeedback {
        let mut feedback = OutputPresentationFeedback::new(output);

        if let CursorImageStatus::Surface(surface) = &self.cursor_image {
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

    pub fn screenshot(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) -> anyhow::Result<()> {
        let _span = tracy_client::span!("Niri::screenshot");

        let size = output.current_mode().unwrap().size;
        let scale = Scale::from(output.current_scale().fractional_scale());
        let elements = self.render(renderer, output, true);
        let pixels = render_to_vec(renderer, size, scale, &elements)?;

        let path = make_screenshot_path().context("error making screenshot path")?;
        debug!("saving screenshot to {path:?}");

        thread::spawn(move || {
            if let Err(err) = image::save_buffer(
                path,
                &pixels,
                size.w as u32,
                size.h as u32,
                image::ColorType::Rgba8,
            ) {
                warn!("error saving screenshot image: {err:?}");
            }
        });

        Ok(())
    }

    pub fn screenshot_all_outputs(
        &mut self,
        renderer: &mut GlesRenderer,
        include_pointer: bool,
        on_done: impl FnOnce(PathBuf) + Send + 'static,
    ) -> anyhow::Result<()> {
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
        let pixels = render_to_vec(renderer, size, Scale::from(1.), &elements)?;

        let path = make_screenshot_path().context("error making screenshot path")?;
        debug!("saving screenshot to {path:?}");

        thread::spawn(move || {
            let res = image::save_buffer(
                &path,
                &pixels,
                size.w as u32,
                size.h as u32,
                image::ColorType::Rgba8,
            );

            if let Err(err) = res {
                warn!("error saving screenshot image: {err:?}");
                return;
            }

            on_done(path);
        });

        Ok(())
    }
}

render_elements! {
    #[derive(Debug)]
    pub OutputRenderElements<R> where R: ImportAll;
    Monitor = MonitorRenderElement<R>,
    Wayland = WaylandSurfaceRenderElement<R>,
    DefaultPointer = TextureRenderElement<<R as Renderer>::TextureId>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

fn render_and_download(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<GlesMapping> {
    let _span = tracy_client::span!("render_and_download");

    let output_rect = Rectangle::from_loc_and_size((0, 0), size);
    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);
    let fourcc = Fourcc::Abgr8888;

    let texture: GlesTexture = renderer
        .create_buffer(fourcc, buffer_size)
        .context("error creating texture")?;

    renderer.bind(texture).context("error binding texture")?;
    let mut frame = renderer
        .render(size, Transform::Normal)
        .context("error starting frame")?;

    frame
        .clear([0.1, 0.1, 0.1, 1.], &[output_rect])
        .context("error clearing")?;

    for element in elements.iter().rev() {
        let src = element.src();
        let dst = element.geometry(scale);
        element
            .draw(&mut frame, src, dst, &[output_rect])
            .context("error drawing element")?;
    }

    let sync_point = frame.finish().context("error finishing frame")?;
    sync_point.wait();

    let mapping = renderer
        .copy_framebuffer(Rectangle::from_loc_and_size((0, 0), buffer_size), fourcc)
        .context("error copying framebuffer")?;
    Ok(mapping)
}

fn render_to_vec(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<Vec<u8>> {
    let _span = tracy_client::span!("render_to_vec");

    let mapping =
        render_and_download(renderer, size, scale, elements).context("error rendering")?;
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

    frame
        .clear([0.1, 0.1, 0.1, 1.], &[output_rect])
        .context("error clearing")?;

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
