use smithay::delegate_xdg_shell;
use smithay::desktop::{Space, Window};
use smithay::input::pointer::{Focus, GrabStartData as PointerGrabStartData};
use smithay::input::Seat;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Rectangle, Serial};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
    XdgToplevelSurfaceData,
};

use crate::grabs::{MoveSurfaceGrab, ResizeSurfaceGrab};
use crate::Niri;

impl XdgShellHandler for Niri {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new(surface);
        self.space.map_element(window, (0, 0), false);
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {
        // TODO: Popup handling using PopupManager
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat = Seat::from_resource(&seat).unwrap();

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = seat.get_pointer().unwrap();

            let window = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == wl_surface)
                .unwrap()
                .clone();
            let initial_window_location = self.space.element_location(&window).unwrap();

            let grab = MoveSurfaceGrab {
                start_data,
                window,
                initial_window_location,
            };

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat = Seat::from_resource(&seat).unwrap();

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = seat.get_pointer().unwrap();

            let window = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == wl_surface)
                .unwrap()
                .clone();
            let initial_window_location = self.space.element_location(&window).unwrap();
            let initial_window_size = window.geometry().size;

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });

            surface.send_pending_configure();

            let grab = ResizeSurfaceGrab::start(
                start_data,
                window,
                edges.into(),
                Rectangle::from_loc_and_size(initial_window_location, initial_window_size),
            );

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // TODO popup grabs
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Maximize)
        {
            let wl_surface = surface.wl_surface();
            let window = self
                .space
                .elements()
                .find(|w| w.toplevel().wl_surface() == wl_surface)
                .unwrap()
                .clone();
            let geometry = self
                .space
                .output_geometry(self.output.as_ref().unwrap())
                .unwrap();

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Maximized);
                state.size = Some(geometry.size);
            });
            self.space.map_element(window, geometry.loc, true);
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if !surface
            .current_state()
            .states
            .contains(xdg_toplevel::State::Maximized)
        {
            return;
        }

        surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Maximized);
            state.size = None;
        });
        surface.send_pending_configure();
    }

    fn toplevel_destroyed(&mut self, _surface: ToplevelSurface) {
        self.queue_redraw();
    }

    fn popup_destroyed(&mut self, _surface: PopupSurface) {
        self.queue_redraw();
    }
}

// Xdg Shell
delegate_xdg_shell!(Niri);

fn check_grab(
    seat: &Seat<Niri>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<Niri>> {
    let pointer = seat.get_pointer()?;

    // Check that this surface has a click grab.
    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    // If the focus was for a different surface, ignore the request.
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

/// Should be called on `WlSurface::commit`
pub fn handle_commit(space: &Space<Window>, surface: &WlSurface) -> Option<()> {
    let window = space
        .elements()
        .find(|w| w.toplevel().wl_surface() == surface)
        .cloned()?;

    let initial_configure_sent = with_states(surface, |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .unwrap()
            .lock()
            .unwrap()
            .initial_configure_sent
    });

    if !initial_configure_sent {
        window.toplevel().send_configure();
    }

    Some(())
}
