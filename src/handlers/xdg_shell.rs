use smithay::desktop::{find_popup_root_surface, layer_map_for_output, PopupKind, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::{self, ResizeEdge};
use smithay::reexports::wayland_server::protocol::wl_output;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
    XdgShellState, XdgToplevelSurfaceData,
};
use smithay::{delegate_kde_decoration, delegate_xdg_decoration, delegate_xdg_shell};

use crate::layout::configure_new_window;
use crate::niri::State;

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.niri.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();
        let window = Window::new(surface);

        // Tell the surface the preferred size and bounds for its likely output.
        let output = self.niri.monitor_set.active_output().unwrap();
        let working_area = layer_map_for_output(output).non_exclusive_zone();
        configure_new_window(working_area, &window);

        // At the moment of creation, xdg toplevels must have no buffer.
        let existing = self.niri.unmapped_windows.insert(wl_surface, window);
        assert!(existing.is_none());
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        // FIXME: adjust the geometry so the popup doesn't overflow at least off the top and bottom
        // screen edges, and ideally off the view size.
        if let Err(err) = self.niri.popups.track_popup(PopupKind::Xdg(surface)) {
            warn!("error tracking popup: {err:?}");
        }
    }

    fn move_request(&mut self, _surface: ToplevelSurface, _seat: WlSeat, _serial: Serial) {
        // FIXME
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: WlSeat,
        _serial: Serial,
        _edges: ResizeEdge,
    ) {
        // FIXME
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        // FIXME: adjust the geometry so the popup doesn't overflow at least off the top and bottom
        // screen edges, and ideally off the view size.
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // FIXME popup grabs
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        // FIXME

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unmaximize_request(&mut self, _surface: ToplevelSurface) {
        // FIXME
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
            // NOTE: This is only one part of the solution. We can set the
            // location and configure size here, but the surface should be rendered fullscreen
            // independently from its buffer size
            if let Some((window, current_output)) = self
                .niri
                .monitor_set
                .find_window_and_output(surface.wl_surface())
            {
                if let Some(requested_output) = wl_output.as_ref().and_then(Output::from_resource) {
                    if requested_output != current_output {
                        self.niri
                            .monitor_set
                            .move_window_to_output(window.clone(), &requested_output);
                    }
                }

                self.niri.monitor_set.set_fullscreen(&window, true);
            }
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some((window, _)) = self
            .niri
            .monitor_set
            .find_window_and_output(surface.wl_surface())
        {
            self.niri.monitor_set.set_fullscreen(&window, false);
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if self
            .niri
            .unmapped_windows
            .remove(surface.wl_surface())
            .is_some()
        {
            // An unmapped toplevel got destroyed.
            return;
        }

        let (window, output) = self
            .niri
            .monitor_set
            .find_window_and_output(surface.wl_surface())
            .unwrap();
        self.niri.monitor_set.remove_window(&window);
        self.niri.queue_redraw(output);
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        if let Ok(root) = find_popup_root_surface(&surface.into()) {
            let root_window_output = self.niri.monitor_set.find_window_and_output(&root);
            if let Some((_window, output)) = root_window_output {
                self.niri.queue_redraw(output);
            }
        }
    }
}

delegate_xdg_shell!(State);

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        let mode = if self.niri.config.borrow().prefer_no_csd {
            Some(zxdg_toplevel_decoration_v1::Mode::ServerSide)
        } else {
            None
        };
        toplevel.with_pending_state(|state| {
            state.decoration_mode = mode;
        });
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: zxdg_toplevel_decoration_v1::Mode) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
        });

        // Only send configure if it's non-initial.
        if initial_configure_sent(&toplevel) {
            toplevel.send_pending_configure();
        }
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        let mode = if self.niri.config.borrow().prefer_no_csd {
            Some(zxdg_toplevel_decoration_v1::Mode::ServerSide)
        } else {
            None
        };
        toplevel.with_pending_state(|state| {
            state.decoration_mode = mode;
        });

        // Only send configure if it's non-initial.
        if initial_configure_sent(&toplevel) {
            toplevel.send_pending_configure();
        }
    }
}
delegate_xdg_decoration!(State);

impl KdeDecorationHandler for State {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.niri.kde_decoration_state
    }
}

delegate_kde_decoration!(State);

pub fn send_initial_configure_if_needed(toplevel: &ToplevelSurface) {
    if !initial_configure_sent(toplevel) {
        toplevel.send_configure();
    }
}

fn initial_configure_sent(toplevel: &ToplevelSurface) -> bool {
    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .unwrap()
            .lock()
            .unwrap()
            .initial_configure_sent
    })
}

impl State {
    /// Should be called on `WlSurface::commit`
    pub fn popups_handle_commit(&mut self, surface: &WlSurface) {
        self.niri.popups.commit(surface);

        if let Some(popup) = self.niri.popups.find_popup(surface) {
            match popup {
                PopupKind::Xdg(ref popup) => {
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
                // Input method popups don't require a configure.
                PopupKind::InputMethod(_) => (),
            }
        }
    }
}
