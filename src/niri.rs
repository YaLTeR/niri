use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::Duration;

use smithay::backend::renderer::element::render_elements;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::ImportAll;
use smithay::desktop::space::{space_render_elements, SpaceRenderElements};
use smithay::desktop::{Space, Window, WindowSurfaceType};
use smithay::input::keyboard::XkbConfig;
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::data_device::DataDeviceState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;

use crate::backend::Backend;
use crate::LoopData;

pub struct Niri {
    pub start_time: std::time::Instant,
    pub event_loop: LoopHandle<'static, LoopData>,
    pub stop_signal: LoopSignal,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,

    // Smithay state.
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,

    pub seat: Seat<Self>,
    pub output: Option<Output>,

    // Set to `true` if there's a redraw queued on the event loop. Reset to `false` in redraw()
    // which means that you cannot queue more than one redraw at once.
    pub redraw_queued: bool,
    pub waiting_for_vblank: bool,
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
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);

        let mut seat: Seat<Self> = seat_state.new_wl_seat(&display_handle, seat_name);
        // FIXME: get Xkb and repeat interval from GNOME dconf.
        seat.add_keyboard(XkbConfig::default(), 400, 30).unwrap();
        seat.add_pointer();

        let space = Space::default();

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

        Self {
            start_time,
            event_loop,
            stop_signal,
            display_handle,

            space,

            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,

            seat,
            output: None,
            redraw_queued: false,
            waiting_for_vblank: false,
        }
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, p + location))
            })
    }

    /// Schedules an immediate redraw if one is not already scheduled.
    pub fn queue_redraw(&mut self) {
        if self.redraw_queued || self.waiting_for_vblank {
            return;
        }

        self.redraw_queued = true;

        // Timer::immediate() adds a millisecond of delay for some reason.
        // This should be fixed in calloop v0.11: https://github.com/Smithay/calloop/issues/142
        self.event_loop.insert_idle(|data| {
            let backend: &mut dyn Backend = if let Some(tty) = &mut data.tty {
                tty
            } else {
                data.winit.as_mut().unwrap()
            };
            data.niri.redraw(backend);
        });
    }

    fn redraw(&mut self, backend: &mut dyn Backend) {
        let _span = tracy_client::span!("redraw");

        assert!(self.redraw_queued);
        assert!(!self.waiting_for_vblank);
        self.redraw_queued = false;

        let elements = space_render_elements(
            backend.renderer(),
            [&self.space],
            self.output.as_ref().unwrap(),
            1.,
        )
        .unwrap();

        let mut elements: Vec<_> = elements
            .into_iter()
            .map(OutputRenderElements::from)
            .collect();
        elements.insert(
            0,
            OutputRenderElements::Pointer(SolidColorRenderElement::new(
                smithay::backend::renderer::element::Id::new(),
                smithay::utils::Rectangle {
                    loc: self
                        .seat
                        .get_pointer()
                        .unwrap()
                        .current_location()
                        .to_physical_precise_round(1.),
                    size: (16, 16).into(),
                },
                CommitCounter::default(),
                [1., 0.8, 0., 1.],
            )),
        );

        backend.render(self, &elements);

        let output = self.output.as_ref().unwrap();
        self.space.elements().for_each(|window| {
            window.send_frame(
                output,
                self.start_time.elapsed(),
                Some(Duration::ZERO),
                |_, _| Some(output.clone()),
            )
        });

        self.space.refresh();
    }
}

render_elements! {
    pub OutputRenderElements<R, E> where R: ImportAll;
    Space=SpaceRenderElements<R, E>,
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
