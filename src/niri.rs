use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sd_notify::NotifyState;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{render_elements, AsRenderElements, RenderElementStates};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{ImportAll, Renderer};
use smithay::desktop::utils::{
    send_frames_surface_tree, surface_presentation_feedback_flags_from_states,
    take_presentation_feedback_surface_tree, OutputPresentationFeedback,
};
use smithay::desktop::{
    layer_map_for_output, LayerSurface, PopupManager, Space, Window, WindowSurfaceType,
};
use smithay::input::keyboard::XkbConfig;
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus, MotionEvent};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Idle, Interest, LoopHandle, LoopSignal, Mode, PostAction};
use smithay::reexports::nix::libc::CLOCK_MONOTONIC;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities;
use smithay::reexports::wayland_server::backend::{
    ClientData, ClientId, DisconnectReason, GlobalId,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale, SERIAL_COUNTER};
use smithay::wayland::compositor::{with_states, CompositorClientState, CompositorState};
use smithay::wayland::data_device::DataDeviceState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::shell::wlr_layer::{Layer, WlrLayerShellState};
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::tablet_manager::TabletManagerState;

use crate::backend::Backend;
use crate::dbus::mutter_service_channel::ServiceChannel;
use crate::frame_clock::FrameClock;
use crate::layout::{MonitorRenderElement, MonitorSet};
use crate::utils::{center, get_monotonic_time, load_default_cursor};
use crate::LoopData;

pub struct Niri {
    pub start_time: std::time::Instant,
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
    pub seat_state: SeatState<Self>,
    pub tablet_state: TabletManagerState,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub presentation_state: PresentationState,

    pub seat: Seat<Self>,

    pub pointer_buffer: Option<(TextureBuffer<GlesTexture>, Point<i32, Physical>)>,
    pub cursor_image: CursorImageStatus,
    pub dnd_icon: Option<WlSurface>,

    pub zbus_conn: Option<zbus::blocking::Connection>,
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

impl Niri {
    pub fn new(
        event_loop: LoopHandle<'static, LoopData>,
        stop_signal: LoopSignal,
        display: &mut Display<Self>,
        seat_name: String,
    ) -> Self {
        let start_time = std::time::Instant::now();

        let display_handle = display.handle();

        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new_with_capabilities::<Self>(
            &display_handle,
            [WmCapabilities::Fullscreen],
        );
        let layer_shell_state = WlrLayerShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let mut seat_state = SeatState::new();
        let tablet_state = TabletManagerState::new::<Self>(&display_handle);
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let presentation_state =
            PresentationState::new::<Self>(&display_handle, CLOCK_MONOTONIC as u32);

        let mut seat: Seat<Self> = seat_state.new_wl_seat(&display_handle, seat_name);
        // FIXME: get Xkb and repeat interval from GNOME dconf.
        let xkb = XkbConfig {
            layout: "us,ru",
            options: Some("grp:win_space_toggle,compose:ralt,ctrl:nocaps".to_owned()),
            ..Default::default()
        };
        seat.add_keyboard(xkb, 400, 30).unwrap();
        seat.add_pointer();

        let socket_source = ListeningSocketSource::new_auto().unwrap();
        let socket_name = socket_source.socket_name().to_os_string();
        event_loop
            .insert_source(socket_source, move |client, _, data| {
                if let Err(err) = data
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

        let mut zbus_conn = None;
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
            if let Err(err) = rv {
                warn!("error spawning shell to import environment into systemd: {err:?}");
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
            zbus_conn = Some(conn);

            // Notify systemd we're ready.
            if let Err(err) = sd_notify::notify(false, &[NotifyState::Ready]) {
                warn!("error notifying systemd: {err:?}");
            };
        }

        let display_source = Generic::new(
            display.backend().poll_fd().as_raw_fd(),
            Interest::READ,
            Mode::Level,
        );
        event_loop
            .insert_source(display_source, |_, _, data| {
                data.display.dispatch_clients(&mut data.niri).unwrap();
                Ok(PostAction::Continue)
            })
            .unwrap();

        Self {
            start_time,
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
            data_device_state,
            popups: PopupManager::default(),
            presentation_state,

            seat,
            pointer_buffer: None,
            cursor_image: CursorImageStatus::Default,
            dnd_icon: None,

            zbus_conn,
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
            global: output.create_global::<Niri>(&self.display_handle),
            queued_redraw: None,
            waiting_for_vblank: false,
            frame_clock: FrameClock::new(refresh_interval),
        };
        let rv = self.output_state.insert(output, state);
        assert!(rv.is_none(), "output was already tracked");
    }

    pub fn remove_output(&mut self, output: &Output) {
        let mut state = self.output_state.remove(output).unwrap();
        self.display_handle.remove_global::<Niri>(state.global);

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

    pub fn move_cursor(&mut self, location: Point<f64, Logical>) {
        let under = self.surface_under_and_global_space(location);
        self.seat.get_pointer().unwrap().motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: get_monotonic_time().as_millis() as u32,
            },
        );
        // FIXME: granular
        self.queue_redraw_all();
    }

    pub fn move_cursor_to_output(&mut self, output: &Output) {
        let geo = self.global_space.output_geometry(output).unwrap();
        self.move_cursor(center(geo).to_f64());
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

    pub fn update_focus(&mut self) {
        let focus = self.layer_surface_focus().or_else(|| {
            self.monitor_set
                .focus()
                .map(|win| win.toplevel().wl_surface().clone())
        });
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.current_focus() != focus {
            keyboard.set_focus(self, focus, SERIAL_COUNTER.next_serial());
            // FIXME: can be more granular.
            self.queue_redraw_all();
        }
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
            let backend: &mut dyn Backend = if let Some(tty) = &mut data.tty {
                tty
            } else {
                data.winit.as_mut().unwrap()
            };
            data.niri.redraw(backend, &output);
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

    fn redraw(&mut self, backend: &mut dyn Backend, output: &Output) {
        let _span = tracy_client::span!("redraw");
        let state = self.output_state.get_mut(output).unwrap();
        let presentation_time = state.frame_clock.next_presentation_time();

        assert!(state.queued_redraw.take().is_some());
        assert!(!state.waiting_for_vblank);

        let renderer = backend.renderer();

        let mon = self.monitor_set.monitor_for_output_mut(output).unwrap();
        mon.advance_animations(presentation_time);
        // Get monitor elements.
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

        // backend.render() uses this.
        drop(layer_map);

        // Hand it over to the backend.
        backend.render(self, output, &elements);

        // Send frame callbacks.
        let frame_callback_time = self.start_time.elapsed();
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
