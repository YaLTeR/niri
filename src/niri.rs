use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::Duration;

use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{render_elements, AsRenderElements};
use smithay::backend::renderer::ImportAll;
use smithay::desktop::{
    layer_map_for_output, LayerSurface, PopupManager, Space, Window, WindowSurfaceType,
};
use smithay::input::keyboard::XkbConfig;
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Idle, Interest, LoopHandle, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities;
use smithay::reexports::wayland_server::backend::{
    ClientData, ClientId, DisconnectReason, GlobalId,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Logical, Point, Scale, SERIAL_COUNTER};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::data_device::DataDeviceState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::shell::wlr_layer::{Layer, WlrLayerShellState};
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;

use crate::backend::Backend;
use crate::frame_clock::FrameClock;
use crate::layout::{MonitorRenderElement, MonitorSet};
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
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,

    pub pointer_buffer: SolidColorBuffer,
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
            [
                WmCapabilities::Fullscreen,
                WmCapabilities::Maximize,
                WmCapabilities::WindowMenu,
            ],
        );
        let layer_shell_state = WlrLayerShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);

        let mut seat: Seat<Self> = seat_state.new_wl_seat(&display_handle, seat_name);
        // FIXME: get Xkb and repeat interval from GNOME dconf.
        let xkb = XkbConfig {
            layout: "us,ru",
            options: Some("grp:win_space_toggle".to_owned()),
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

        let pointer_buffer = SolidColorBuffer::new((16, 16), [1., 0.8, 0., 1.]);

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
            data_device_state,
            popups: PopupManager::default(),

            seat,
            pointer_buffer,
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

        let output_pos = self.global_space.output_geometry(output).unwrap().loc;
        let pointer_pos = self.seat.get_pointer().unwrap().current_location() - output_pos.to_f64();

        // Get layer-shell elements.
        let layer_map = layer_map_for_output(output);
        let (lower, upper): (Vec<&LayerSurface>, Vec<&LayerSurface>) = layer_map
            .layers()
            .rev()
            .partition(|s| matches!(s.layer(), Layer::Background | Layer::Bottom));

        // The pointer goes on the top.
        let mut elements = vec![OutputRenderElements::Pointer(
            SolidColorRenderElement::from_buffer(
                &self.pointer_buffer,
                pointer_pos.to_physical_precise_round(1.),
                1.,
                1.,
            ),
        )];

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

        // Hand it over to the backend.
        backend.render(self, output, &elements);

        // Send frame callbacks.
        let frame_callback_time = self.start_time.elapsed();
        self.monitor_set.send_frame(output, frame_callback_time);

        for surface in layer_map.layers() {
            surface.send_frame(output, frame_callback_time, None, |_, _| {
                Some(output.clone())
            });
        }
    }
}

render_elements! {
    #[derive(Debug)]
    pub OutputRenderElements<R> where R: ImportAll;
    Monitor = MonitorRenderElement<R>,
    Wayland = WaylandSurfaceRenderElement<R>,
    Pointer = SolidColorRenderElement,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
