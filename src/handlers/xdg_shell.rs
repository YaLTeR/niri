use smithay::delegate_xdg_shell;
use smithay::desktop::{find_popup_root_surface, PopupKind, Window};
use smithay::input::pointer::{Focus, GrabStartData as PointerGrabStartData};
use smithay::input::Seat;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::protocol::{wl_output, wl_seat};
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Rectangle, Serial};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
    XdgShellState, XdgToplevelSurfaceData,
};

use crate::grabs::{MoveSurfaceGrab, ResizeSurfaceGrab};
use crate::layout::{configure_new_window, output_size};
use crate::Niri;

impl XdgShellHandler for Niri {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();
        let window = Window::new(surface);

        // Tell the surface the preferred size and bounds for its likely output.
        let output = self.monitor_set.active_output().unwrap();
        configure_new_window(output_size(output), &window);

        // At the moment of creation, xdg toplevels must have no buffer.
        let existing = self.unmapped_windows.insert(wl_surface, window);
        assert!(existing.is_none());
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        surface.with_pending_state(|state| {
            // NOTE: This is not really necessary as the default geometry
            // is already set the same way, but for demonstrating how
            // to set the initial popup geometry this code is left as
            // an example
            state.geometry = positioner.get_geometry();
        });
        if let Err(err) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            warn!("error tracking popup: {err:?}");
        }
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        // FIXME

        // let seat = Seat::from_resource(&seat).unwrap();

        // let wl_surface = surface.wl_surface();

        // if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
        //     let pointer = seat.get_pointer().unwrap();

        //     let (window, space) = self.monitor_set.find_window_and_space(wl_surface).unwrap();
        //     let initial_window_location = space.element_location(&window).unwrap();

        //     let grab = MoveSurfaceGrab {
        //         start_data,
        //         window: window.clone(),
        //         initial_window_location,
        //     };

        //     pointer.set_grab(self, grab, serial, Focus::Clear);
        // }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        // FIXME

        // let seat = Seat::from_resource(&seat).unwrap();

        // let wl_surface = surface.wl_surface();

        // if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
        //     let pointer = seat.get_pointer().unwrap();

        //     let (window, space) = self.monitor_set.find_window_and_space(wl_surface).unwrap();
        //     let initial_window_location = space.element_location(&window).unwrap();
        //     let initial_window_size = window.geometry().size;

        //     surface.with_pending_state(|state| {
        //         state.states.set(xdg_toplevel::State::Resizing);
        //     });

        //     surface.send_pending_configure();

        //     let grab = ResizeSurfaceGrab::start(
        //         start_data,
        //         window.clone(),
        //         edges.into(),
        //         Rectangle::from_loc_and_size(initial_window_location, initial_window_size),
        //     );

        //     pointer.set_grab(self, grab, serial, Focus::Clear);
        // }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            // NOTE: This is again a simplification, a proper compositor would
            // calculate the geometry of the popup here. For simplicity we just
            // use the default implementation here that does not take the
            // window position and output constraints into account.
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // FIXME popup grabs
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Maximize)
        {
            // let wl_surface = surface.wl_surface();
            // let (window, space) = self.monitor_set.find_window_and_space(wl_surface).unwrap();
            // let geometry = space
            //     .output_geometry(space.outputs().next().unwrap())
            //     .unwrap();

            // surface.with_pending_state(|state| {
            //     state.states.set(xdg_toplevel::State::Maximized);
            //     state.size = Some(geometry.size);
            // });
            // space.map_element(window.clone(), geometry.loc, true);
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

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        wl_output: Option<wl_output::WlOutput>,
    ) {
        if surface
            .current_state()
            .capabilities
            .contains(xdg_toplevel::WmCapabilities::Fullscreen)
        {
            // // NOTE: This is only one part of the solution. We can set the
            // // location and configure size here, but the surface should be rendered fullscreen
            // // independently from its buffer size
            // let wl_surface = surface.wl_surface();

            // let output = wl_output
            //     .as_ref()
            //     .and_then(Output::from_resource)
            //     .or_else(|| {
            //         self.monitor_set
            //             .find_window_and_space(wl_surface)
            //             .and_then(|(_window, space)| space.outputs().next().cloned())
            //     });

            // if let Some(output) = output {
            //     let (window, space) =
            // self.monitor_set.find_window_and_space(wl_surface).unwrap();
            //     let geometry = space.output_geometry(&output).unwrap();

            //     surface.with_pending_state(|state| {
            //         state.states.set(xdg_toplevel::State::Fullscreen);
            //         state.size = Some(geometry.size);
            //     });

            //     space.map_element(window.clone(), geometry.loc, true);
            // }
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if !surface
            .current_state()
            .states
            .contains(xdg_toplevel::State::Fullscreen)
        {
            return;
        }

        surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Fullscreen);
            state.size = None;
        });

        surface.send_pending_configure();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if self.unmapped_windows.remove(surface.wl_surface()).is_some() {
            // An unmapped toplevel got destroyed.
            return;
        }

        let (window, output) = self
            .monitor_set
            .find_window_and_output(surface.wl_surface())
            .unwrap();
        self.monitor_set.remove_window(&window);
        self.queue_redraw(output);
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        if let Ok(root) = find_popup_root_surface(&surface.into()) {
            let root_window_output = self.monitor_set.find_window_and_output(&root);
            if let Some((_window, output)) = root_window_output {
                self.queue_redraw(output);
            }
        }
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

pub fn send_initial_configure_if_needed(window: &Window) {
    let initial_configure_sent = with_states(window.toplevel().wl_surface(), |states| {
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
}

impl Niri {
    /// Should be called on `WlSurface::commit`
    pub fn popups_handle_commit(&mut self, surface: &WlSurface) {
        self.popups.commit(surface);

        if let Some(popup) = self.popups.find_popup(surface) {
            let PopupKind::Xdg(ref popup) = popup;
            let initial_configure_sent = with_states(surface, |states| {
                states
                    .data_map
                    .get::<XdgPopupSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .initial_configure_sent
            });
            if !initial_configure_sent {
                // NOTE: This should never fail as the initial configure is always
                // allowed.
                popup.send_configure().expect("initial configure failed");
            }
        }
    }
}
