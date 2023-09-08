use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, thread};

use anyhow::Context;
use directories::UserDirs;
use sd_notify::NotifyState;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{
    render_elements, AsRenderElements, Element, RenderElement, RenderElementStates,
};
use smithay::backend::renderer::gles::{GlesMapping, GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ExportMem, Frame, ImportAll, Offscreen, Renderer};
use smithay::desktop::utils::{
    send_dmabuf_feedback_surface_tree, send_frames_surface_tree,
    surface_presentation_feedback_flags_from_states, take_presentation_feedback_surface_tree,
    OutputPresentationFeedback,
};
use smithay::desktop::{
    layer_map_for_output, LayerSurface, PopupManager, Space, Window, WindowSurfaceType,
};
use smithay::input::keyboard::XkbConfig;
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus, MotionEvent};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{self, Idle, Interest, LoopHandle, LoopSignal, Mode, PostAction};
use smithay::reexports::nix::libc::CLOCK_MONOTONIC;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities;
use smithay::reexports::wayland_server::backend::{
    ClientData, ClientId, DisconnectReason, GlobalId,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{
    IsAlive, Logical, Physical, Point, Rectangle, Scale, Size, Transform, SERIAL_COUNTER,
};
use smithay::wayland::compositor::{with_states, CompositorClientState, CompositorState};
use smithay::wayland::data_device::DataDeviceState;
use smithay::wayland::dmabuf::DmabufFeedback;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_gestures::PointerGesturesState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::shell::wlr_layer::{Layer, WlrLayerShellState};
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::tablet_manager::TabletManagerState;
use time::OffsetDateTime;

use crate::backend::{Backend, Tty, Winit};
use crate::config::Config;
use crate::dbus::mutter_display_config::DisplayConfig;
use crate::dbus::mutter_screen_cast::{self, ScreenCast, ToNiriMsg};
use crate::dbus::mutter_service_channel::ServiceChannel;
use crate::frame_clock::FrameClock;
use crate::layout::{MonitorRenderElement, MonitorSet};
use crate::pw_utils::{Cast, PipeWire};
use crate::utils::{center, get_monotonic_time, load_default_cursor};
use crate::LoopData;

pub struct Niri {
    pub event_loop: LoopHandle<'static, LoopData>,
    pub stop_signal: LoopSignal,
    pub display_handle: DisplayHandle,

    // Each workspace corresponds to a Space. Each workspace generally has one Output mapped to it,
    // however it may have none (when there are no outputs connected) or mutiple (when mirroring).
    pub monitor_set: MonitorSet<Window>,

    // This space does not actually contain any windows, but all outputs are mapped into it
    // according to their global position.
    pub global_space: Space<Window>,

    // Windows which don't have a buffer attached yet.
    pub unmapped_windows: HashMap<WlSurface, Window>,

    pub output_state: HashMap<Output, OutputState>,

    // Smithay state.
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<State>,
    pub tablet_state: TabletManagerState,
    pub pointer_gestures_state: PointerGesturesState,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub presentation_state: PresentationState,

    pub seat: Seat<State>,

    pub pointer_buffer: Option<(TextureBuffer<GlesTexture>, Point<i32, Physical>)>,
    pub cursor_image: CursorImageStatus,
    pub dnd_icon: Option<WlSurface>,

    pub zbus_conn: Option<zbus::blocking::Connection>,
    pub inhibit_power_key_fd: Option<zbus::zvariant::OwnedFd>,
    pub screen_cast: ScreenCast,

    // Casts are dropped before PipeWire to prevent a double-free (yay).
    pub casts: Vec<Cast>,
    pub pipewire: Option<PipeWire>,
}

pub struct OutputState {
    pub global: GlobalId,
    // Set if there's a redraw queued on the event loop. Reset in redraw() which means that you
    // cannot queue more than one redraw at once.
    pub queued_redraw: Option<Idle<'static>>,
    // Set to `true` when the output was redrawn and is waiting for a VBlank. Upon VBlank a redraw
    // will always be queued, so you cannot queue a redraw while waiting for a VBlank.
    pub waiting_for_vblank: bool,
    pub frame_clock: FrameClock,
}

pub struct State {
    pub config: Config,
    pub backend: Backend,
    pub niri: Niri,
}

impl State {
    pub fn new(
        config: Config,
        event_loop: LoopHandle<'static, LoopData>,
        stop_signal: LoopSignal,
        display: &mut Display<State>,
    ) -> Self {
        let has_display =
            env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("DISPLAY").is_some();

        let mut backend = if has_display {
            Backend::Winit(Winit::new(event_loop.clone()))
        } else {
            Backend::Tty(Tty::new(event_loop.clone()))
        };

        let mut niri = Niri::new(&config, event_loop, stop_signal, display, &backend);
        backend.init(&mut niri);

        Self {
            config,
            backend,
            niri,
        }
    }

    pub fn move_cursor(&mut self, location: Point<f64, Logical>) {
        let under = self.niri.surface_under_and_global_space(location);
        self.niri.seat.get_pointer().unwrap().motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: get_monotonic_time().as_millis() as u32,
            },
        );
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
                .monitor_set
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
}

impl Niri {
    pub fn new(
        config: &Config,
        event_loop: LoopHandle<'static, LoopData>,
        stop_signal: LoopSignal,
        display: &mut Display<State>,
        backend: &Backend,
    ) -> Self {
        let display_handle = display.handle();

        let compositor_state = CompositorState::new::<State>(&display_handle);
        let xdg_shell_state = XdgShellState::new_with_capabilities::<State>(
            &display_handle,
            [WmCapabilities::Fullscreen],
        );
        let layer_shell_state = WlrLayerShellState::new::<State>(&display_handle);
        let shm_state = ShmState::new::<State>(&display_handle, vec![]);
        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<State>(&display_handle);
        let mut seat_state = SeatState::new();
        let tablet_state = TabletManagerState::new::<State>(&display_handle);
        let pointer_gestures_state = PointerGesturesState::new::<State>(&display_handle);
        let data_device_state = DataDeviceState::new::<State>(&display_handle);
        let presentation_state =
            PresentationState::new::<State>(&display_handle, CLOCK_MONOTONIC as u32);

        let mut seat: Seat<State> = seat_state.new_wl_seat(&display_handle, backend.seat_name());
        let xkb = XkbConfig {
            rules: &config.input.keyboard.xkb.rules,
            model: &config.input.keyboard.xkb.model,
            layout: config.input.keyboard.xkb.layout.as_deref().unwrap_or("us"),
            variant: &config.input.keyboard.xkb.variant,
            options: config.input.keyboard.xkb.options.clone(),
        };
        seat.add_keyboard(xkb, 400, 30).unwrap();
        seat.add_pointer();

        let socket_source = ListeningSocketSource::new_auto().unwrap();
        let socket_name = socket_source.socket_name().to_os_string();
        event_loop
            .insert_source(socket_source, move |client, _, data| {
                if let Err(err) = data
                    .state
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

        let (to_niri, from_screen_cast) = calloop::channel::channel();
        event_loop
            .insert_source(from_screen_cast, {
                let to_niri = to_niri.clone();
                move |event, _, data| match event {
                    calloop::channel::Event::Msg(msg) => match msg {
                        ToNiriMsg::StartCast {
                            session_id,
                            output,
                            cursor_mode,
                            signal_ctx,
                        } => {
                            let _span = tracy_client::span!("StartCast");

                            debug!(session_id, "StartCast");

                            let gbm = match data.state.backend.gbm_device() {
                                Some(gbm) => gbm,
                                None => {
                                    debug!("no GBM device available");
                                    return;
                                }
                            };

                            let pw = data.state.niri.pipewire.as_ref().unwrap();
                            match pw.start_cast(
                                to_niri.clone(),
                                gbm,
                                session_id,
                                output,
                                cursor_mode,
                                signal_ctx,
                            ) {
                                Ok(cast) => {
                                    data.state.niri.casts.push(cast);
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

                            for i in (0..data.state.niri.casts.len()).rev() {
                                let cast = &data.state.niri.casts[i];
                                if cast.session_id != session_id {
                                    continue;
                                }

                                let cast = data.state.niri.casts.swap_remove(i);
                                if let Err(err) = cast.stream.disconnect() {
                                    warn!("error disconnecting stream: {err:?}");
                                }
                            }

                            let server =
                                data.state.niri.zbus_conn.as_ref().unwrap().object_server();
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
        let screen_cast = ScreenCast::new(backend.connectors(), to_niri);

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
            let mut conn = zbus::blocking::ConnectionBuilder::session()
                .unwrap()
                .name("org.gnome.Mutter.ServiceChannel")
                .unwrap()
                .serve_at(
                    "/org/gnome/Mutter/ServiceChannel",
                    ServiceChannel::new(display_handle.clone()),
                )
                .unwrap();

            if pipewire.is_some() && !config.debug.screen_cast_in_non_session_instances {
                conn = conn
                    .name("org.gnome.Mutter.ScreenCast")
                    .unwrap()
                    .serve_at("/org/gnome/Mutter/ScreenCast", screen_cast.clone())
                    .unwrap()
                    .name("org.gnome.Mutter.DisplayConfig")
                    .unwrap()
                    .serve_at(
                        "/org/gnome/Mutter/DisplayConfig",
                        DisplayConfig::new(backend.connectors()),
                    )
                    .unwrap();
            }

            let conn = conn.build().unwrap();
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
        } else if pipewire.is_some() && config.debug.screen_cast_in_non_session_instances {
            let conn = zbus::blocking::ConnectionBuilder::session()
                .unwrap()
                .name("org.gnome.Mutter.ScreenCast")
                .unwrap()
                .serve_at("/org/gnome/Mutter/ScreenCast", screen_cast.clone())
                .unwrap()
                .name("org.gnome.Mutter.DisplayConfig")
                .unwrap()
                .serve_at(
                    "/org/gnome/Mutter/DisplayConfig",
                    DisplayConfig::new(backend.connectors()),
                )
                .unwrap()
                .build()
                .unwrap();
            zbus_conn = Some(conn);
        }

        let display_source = Generic::new(
            display.backend().poll_fd().as_raw_fd(),
            Interest::READ,
            Mode::Level,
        );
        event_loop
            .insert_source(display_source, |_, _, data| {
                data.display.dispatch_clients(&mut data.state).unwrap();
                Ok(PostAction::Continue)
            })
            .unwrap();

        Self {
            event_loop,
            stop_signal,
            display_handle,

            monitor_set: MonitorSet::new(),
            global_space: Space::default(),
            output_state: HashMap::new(),
            unmapped_windows: HashMap::new(),

            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            tablet_state,
            pointer_gestures_state,
            data_device_state,
            popups: PopupManager::default(),
            presentation_state,

            seat,
            pointer_buffer: None,
            cursor_image: CursorImageStatus::Default,
            dnd_icon: None,

            zbus_conn,
            inhibit_power_key_fd,
            screen_cast,
            pipewire,
            casts: vec![],
        }
    }

    pub fn add_output(&mut self, output: Output, refresh_interval: Option<Duration>) {
        let x = self
            .global_space
            .outputs()
            .map(|output| self.global_space.output_geometry(output).unwrap())
            .map(|geom| geom.loc.x + geom.size.w)
            .max()
            .unwrap_or(0);

        self.global_space.map_output(&output, (x, 0));
        self.monitor_set.add_output(output.clone());

        let state = OutputState {
            global: output.create_global::<State>(&self.display_handle),
            queued_redraw: None,
            waiting_for_vblank: false,
            frame_clock: FrameClock::new(refresh_interval),
        };
        let rv = self.output_state.insert(output, state);
        assert!(rv.is_none(), "output was already tracked");
    }

    pub fn remove_output(&mut self, output: &Output) {
        let mut state = self.output_state.remove(output).unwrap();
        self.display_handle.remove_global::<State>(state.global);

        if let Some(idle) = state.queued_redraw.take() {
            idle.cancel();
        }

        self.monitor_set.remove_output(output);
        self.global_space.unmap_output(output);
        // FIXME: reposition outputs so they are adjacent.
    }

    pub fn output_resized(&mut self, output: Output) {
        self.monitor_set.update_output(&output);
        layer_map_for_output(&output).arrange();
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
        let (window, _loc) = self.monitor_set.window_under(output, pos_within_output)?;
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
            self.monitor_set.window_under(output, pos_within_output)?;

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
        let active = self.monitor_set.active_output()?;
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
        let active = self.monitor_set.active_output()?;
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
        let active = self.monitor_set.active_output()?;
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
        let active = self.monitor_set.active_output()?;
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

    fn layer_surface_focus(&self) -> Option<WlSurface> {
        let output = self.monitor_set.active_output()?;
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

        if state.queued_redraw.is_some() || state.waiting_for_vblank {
            return;
        }

        // Timer::immediate() adds a millisecond of delay for some reason.
        // This should be fixed in calloop v0.11: https://github.com/Smithay/calloop/issues/142
        let idle = self.event_loop.insert_idle(move |data| {
            data.state.niri.redraw(&mut data.state.backend, &output);
        });
        state.queued_redraw = Some(idle);
    }

    pub fn pointer_element(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) -> Vec<OutputRenderElements<GlesRenderer>> {
        let output_pos = self.global_space.output_geometry(output).unwrap().loc;
        let pointer_pos = self.seat.get_pointer().unwrap().current_location() - output_pos.to_f64();

        let (default_buffer, default_hotspot) = self
            .pointer_buffer
            .get_or_insert_with(|| load_default_cursor(renderer));
        let default_hotspot = default_hotspot.to_logical(1);

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
                self.cursor_image = CursorImageStatus::Default;
                default_hotspot
            }
        } else {
            default_hotspot
        };
        let pointer_pos = (pointer_pos - hotspot.to_f64()).to_physical_precise_round(1.);

        let mut pointer_elements = match &self.cursor_image {
            CursorImageStatus::Hidden => vec![],
            CursorImageStatus::Default => vec![OutputRenderElements::DefaultPointer(
                TextureRenderElement::from_texture_buffer(
                    pointer_pos.to_f64(),
                    default_buffer,
                    None,
                    None,
                    None,
                ),
            )],
            CursorImageStatus::Surface(surface) => {
                render_elements_from_surface_tree(renderer, surface, pointer_pos, 1., 1.)
            }
        };

        if let Some(dnd_icon) = &self.dnd_icon {
            pointer_elements.extend(render_elements_from_surface_tree(
                renderer,
                dnd_icon,
                pointer_pos,
                1.,
                1.,
            ));
        }

        pointer_elements
    }

    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) -> Vec<OutputRenderElements<GlesRenderer>> {
        let _span = tracy_client::span!("Niri::render");

        // Get monitor elements.
        let mon = self.monitor_set.monitor_for_output(output).unwrap();
        let monitor_elements = mon.render_elements(renderer);

        // Get layer-shell elements.
        let layer_map = layer_map_for_output(output);
        let (lower, upper): (Vec<&LayerSurface>, Vec<&LayerSurface>) = layer_map
            .layers()
            .rev()
            .partition(|s| matches!(s.layer(), Layer::Background | Layer::Bottom));

        // The pointer goes on the top.
        let mut elements = self.pointer_element(renderer, output);

        // Then the upper layer-shell elements.
        elements.extend(
            upper
                .into_iter()
                .filter_map(|surface| {
                    layer_map
                        .layer_geometry(surface)
                        .map(|geo| (geo.loc, surface))
                })
                .flat_map(|(loc, surface)| {
                    surface
                        .render_elements(
                            renderer,
                            loc.to_physical_precise_round(1.),
                            Scale::from(1.),
                            1.,
                        )
                        .into_iter()
                        .map(OutputRenderElements::Wayland)
                }),
        );

        // Then the regular monitor elements.
        elements.extend(monitor_elements.into_iter().map(OutputRenderElements::from));

        // Then the lower layer-shell elements.
        elements.extend(
            lower
                .into_iter()
                .filter_map(|surface| {
                    layer_map
                        .layer_geometry(surface)
                        .map(|geo| (geo.loc, surface))
                })
                .flat_map(|(loc, surface)| {
                    surface
                        .render_elements(
                            renderer,
                            loc.to_physical_precise_round(1.),
                            Scale::from(1.),
                            1.,
                        )
                        .into_iter()
                        .map(OutputRenderElements::Wayland)
                }),
        );

        elements
    }

    fn redraw(&mut self, backend: &mut Backend, output: &Output) {
        let _span = tracy_client::span!("Niri::redraw");

        let state = self.output_state.get_mut(output).unwrap();
        let presentation_time = state.frame_clock.next_presentation_time();

        assert!(state.queued_redraw.take().is_some());
        assert!(!state.waiting_for_vblank);

        // Advance the animations.
        let mon = self.monitor_set.monitor_for_output_mut(output).unwrap();
        mon.advance_animations(presentation_time);

        // Render the elements.
        let elements = self.render(backend.renderer(), output);

        // Hand it over to the backend.
        let dmabuf_feedback = backend.render(self, output, &elements);

        // Send the dmabuf feedbacks.
        if let Some(feedback) = dmabuf_feedback {
            self.send_dmabuf_feedbacks(output, feedback);
        }

        // Send the frame callbacks.
        self.send_frame_callbacks(output);

        // Render and send to PipeWire screencast streams.
        self.send_for_screen_cast(backend, output, &elements, presentation_time);
    }

    fn send_dmabuf_feedbacks(&self, output: &Output, feedback: &DmabufFeedback) {
        let _span = tracy_client::span!("Niri::send_dmabuf_feedbacks");

        self.monitor_set.send_dmabuf_feedback(output, feedback);

        for surface in layer_map_for_output(output).layers() {
            surface.send_dmabuf_feedback(output, |_, _| Some(output.clone()), |_, _| feedback);
        }

        if let Some(surface) = &self.dnd_icon {
            send_dmabuf_feedback_surface_tree(
                surface,
                output,
                |_, _| Some(output.clone()),
                |_, _| feedback,
            );
        }

        if let CursorImageStatus::Surface(surface) = &self.cursor_image {
            send_dmabuf_feedback_surface_tree(
                surface,
                output,
                |_, _| Some(output.clone()),
                |_, _| feedback,
            );
        }
    }

    fn send_frame_callbacks(&self, output: &Output) {
        let _span = tracy_client::span!("Niri::send_frame_callbacks");

        let frame_callback_time = get_monotonic_time();
        self.monitor_set.send_frame(output, frame_callback_time);

        for surface in layer_map_for_output(output).layers() {
            surface.send_frame(output, frame_callback_time, None, |_, _| {
                Some(output.clone())
            });
        }

        if let Some(surface) = &self.dnd_icon {
            send_frames_surface_tree(surface, output, frame_callback_time, None, |_, _| {
                Some(output.clone())
            });
        }

        if let CursorImageStatus::Surface(surface) = &self.cursor_image {
            send_frames_surface_tree(surface, output, frame_callback_time, None, |_, _| {
                Some(output.clone())
            });
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
                |_, _| Some(output.clone()),
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        if let Some(surface) = &self.dnd_icon {
            take_presentation_feedback_surface_tree(
                surface,
                &mut feedback,
                |_, _| Some(output.clone()),
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        for win in self.monitor_set.windows_for_output(output) {
            win.take_presentation_feedback(
                &mut feedback,
                |_, _| Some(output.clone()),
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            )
        }

        for surface in layer_map_for_output(output).layers() {
            surface.take_presentation_feedback(
                &mut feedback,
                |_, _| Some(output.clone()),
                |surface, _| {
                    surface_presentation_feedback_flags_from_states(surface, render_element_states)
                },
            );
        }

        feedback
    }

    fn send_for_screen_cast(
        &mut self,
        backend: &mut Backend,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
        presentation_time: Duration,
    ) {
        let _span = tracy_client::span!("Niri::send_for_screen_cast");

        let size = output.current_mode().unwrap().size;

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
                render_to_dmabuf(backend.renderer(), dmabuf, size, elements).unwrap();

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
        let elements = self.render(renderer, output);

        let mapping = render_and_download(renderer, size, &elements).context("error rendering")?;
        let copy = renderer
            .map_texture(&mapping)
            .context("error mapping texture")?;
        let pixels = copy.to_vec();

        let dirs = UserDirs::new().context("error retrieving home directory")?;
        let mut path = dirs.picture_dir().map(|p| p.to_owned()).unwrap_or_else(|| {
            let mut dir = dirs.home_dir().to_owned();
            dir.push("Pictures");
            dir
        });
        path.push("Screenshots");

        unsafe {
            // are you kidding me
            time::util::local_offset::set_soundness(time::util::local_offset::Soundness::Unsound);
        };

        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let desc = time::macros::format_description!(
            "Screenshot from [year]-[month]-[day] [hour]-[minute]-[second].png"
        );
        let name = now.format(desc).context("error formatting time")?;
        path.push(name);

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
    elements: &[OutputRenderElements<GlesRenderer>],
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
        let dst = element.geometry(Scale::from(1.));
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

fn render_to_dmabuf(
    renderer: &mut GlesRenderer,
    dmabuf: Dmabuf,
    size: Size<i32, Physical>,
    elements: &[OutputRenderElements<GlesRenderer>],
) -> anyhow::Result<()> {
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
        let dst = element.geometry(Scale::from(1.));
        element
            .draw(&mut frame, src, dst, &[output_rect])
            .context("error drawing element")?;
    }

    let _sync_point = frame.finish().context("error finishing frame")?;

    Ok(())
}
