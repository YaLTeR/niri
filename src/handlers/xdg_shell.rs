use smithay::delegate_xdg_shell;
use smithay::desktop::{find_popup_root_surface, PopupKind, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::{self, ResizeEdge};
use smithay::reexports::wayland_server::protocol::wl_output;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
    XdgShellState, XdgToplevelSurfaceData,
};

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

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        // FIXME: adjust the geometry so the popup doesn't overflow at least off the top and bottom
        // screen edges, and ideally off the view size.
        if let Err(err) = self.popups.track_popup(PopupKind::Xdg(surface)) {
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
                .monitor_set
                .find_window_and_output(surface.wl_surface())
            {
                if let Some(requested_output) = wl_output.as_ref().and_then(Output::from_resource) {
                    if requested_output != current_output {
                        self.monitor_set
                            .move_window_to_output(window.clone(), &requested_output);
                    }
                }

                self.monitor_set.set_fullscreen(&window, true);
            }
        }

        // The protocol demands us to always reply with a configure,
        // regardless of we fulfilled the request or not
        surface.send_configure();
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some((window, _)) = self
            .monitor_set
            .find_window_and_output(surface.wl_surface())
        {
            self.monitor_set.set_fullscreen(&window, false);
        }
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

delegate_xdg_shell!(Niri);

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
