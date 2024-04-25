use smithay::desktop::{
    find_popup_root_surface, get_popup_toplevel_coords, layer_map_for_output, utils, LayerSurface,
    PopupKeyboardGrab, PopupKind, PopupManager, PopupPointerGrab, PopupUngrabStrategy, Window,
    WindowSurfaceType,
};
use smithay::input::pointer::Focus;
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_positioner::ConstraintAdjustment;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::{self, ResizeEdge};
use smithay::reexports::wayland_server::protocol::wl_output;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle, Serial};
use smithay::wayland::compositor::{
    add_pre_commit_hook, send_surface_state, with_states, BufferAssignment, HookId,
    SurfaceAttributes,
};
use smithay::wayland::input_method::InputMethodSeat;
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::shell::wlr_layer::{self, Layer};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgPopupSurfaceData, XdgShellHandler,
    XdgShellState, XdgToplevelSurfaceData,
};
use smithay::wayland::xdg_foreign::{XdgForeignHandler, XdgForeignState};
use smithay::{
    delegate_kde_decoration, delegate_xdg_decoration, delegate_xdg_foreign, delegate_xdg_shell,
};

use crate::layout::workspace::ColumnWidth;
use crate::layout::LayoutElement as _;
use crate::niri::{PopupGrabState, State};
use crate::window::{InitialConfigureState, ResolvedWindowRules, Unmapped, WindowRef};

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.niri.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();
        let unmapped = Unmapped::new(Window::new_wayland_window(surface));
        let existing = self.niri.unmapped_windows.insert(wl_surface, unmapped);
        assert!(existing.is_none());
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        let popup = PopupKind::Xdg(surface);
        self.unconstrain_popup(&popup);

        if let Err(err) = self.niri.popups.track_popup(popup) {
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
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&PopupKind::Xdg(surface.clone()));
        surface.send_repositioned(token);
    }

    fn grab(&mut self, surface: PopupSurface, _seat: WlSeat, serial: Serial) {
        // HACK: ignore grabs (pretend they work without actually grabbing) if the input method has
        // a grab. It will likely need refactors in Smithay to support properly since grabs just
        // replace each other.
        // FIXME: do this properly.
        if self.niri.seat.input_method().keyboard_grabbed() {
            trace!("ignoring popup grab because IME has keyboard grabbed");
            return;
        }

        let popup = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&popup) else {
            return;
        };

        // We need to hand out the grab in a way consistent with what update_keyboard_focus()
        // thinks the current focus is, otherwise it will desync and cause weird issues with
        // keyboard focus being at the wrong place.
        if self.niri.is_locked() {
            if Some(&root) != self.niri.lock_surface_focus().as_ref() {
                let _ = PopupManager::dismiss_popup(&root, &popup);
                return;
            }
        } else if self.niri.screenshot_ui.is_open() {
            let _ = PopupManager::dismiss_popup(&root, &popup);
            return;
        } else if let Some(output) = self.niri.layout.active_output() {
            let layers = layer_map_for_output(output);

            if let Some(layer_surface) =
                layers.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)
            {
                if !matches!(layer_surface.layer(), Layer::Overlay | Layer::Top) {
                    let _ = PopupManager::dismiss_popup(&root, &popup);
                    return;
                }
            } else {
                if layers.layers_on(Layer::Overlay).any(|l| {
                    l.cached_state().keyboard_interactivity
                        == wlr_layer::KeyboardInteractivity::Exclusive
                }) {
                    let _ = PopupManager::dismiss_popup(&root, &popup);
                    return;
                }

                let mon = self.niri.layout.monitor_for_output(output).unwrap();
                if !mon.render_above_top_layer()
                    && layers.layers_on(Layer::Top).any(|l| {
                        l.cached_state().keyboard_interactivity
                            == wlr_layer::KeyboardInteractivity::Exclusive
                    })
                {
                    let _ = PopupManager::dismiss_popup(&root, &popup);
                    return;
                }

                let layout_focus = self.niri.layout.focus();
                if Some(&root) != layout_focus.map(|win| win.toplevel().wl_surface()) {
                    let _ = PopupManager::dismiss_popup(&root, &popup);
                    return;
                }
            }
        } else {
            let _ = PopupManager::dismiss_popup(&root, &popup);
            return;
        }

        let seat = &self.niri.seat;
        let Ok(mut grab) = self
            .niri
            .popups
            .grab_popup(root.clone(), popup, seat, serial)
        else {
            return;
        };

        let keyboard = seat.get_keyboard().unwrap();
        let pointer = seat.get_pointer().unwrap();

        let keyboard_grab_mismatches = keyboard.is_grabbed()
            && !(keyboard.has_grab(serial)
                || grab
                    .previous_serial()
                    .map_or(true, |s| keyboard.has_grab(s)));
        let pointer_grab_mismatches = pointer.is_grabbed()
            && !(pointer.has_grab(serial)
                || grab.previous_serial().map_or(true, |s| pointer.has_grab(s)));
        if keyboard_grab_mismatches || pointer_grab_mismatches {
            grab.ungrab(PopupUngrabStrategy::All);
            return;
        }

        trace!("new grab for root {:?}", root);
        keyboard.set_focus(self, grab.current_grab(), serial);
        keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        self.niri.popup_grab = Some(PopupGrabState { root, grab });
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        // FIXME

        // A configure is required in response to this event. However, if an initial configure
        // wasn't sent, then we will send this as part of the initial configure later.
        if initial_configure_sent(&surface) {
            surface.send_configure();
        }
    }

    fn unmaximize_request(&mut self, _surface: ToplevelSurface) {
        // FIXME
    }

    fn fullscreen_request(
        &mut self,
        toplevel: ToplevelSurface,
        wl_output: Option<wl_output::WlOutput>,
    ) {
        let requested_output = wl_output.as_ref().and_then(Output::from_resource);

        if let Some((mapped, current_output)) = self
            .niri
            .layout
            .find_window_and_output(toplevel.wl_surface())
        {
            let window = mapped.window.clone();

            if let Some(requested_output) = requested_output {
                if &requested_output != current_output {
                    self.niri
                        .layout
                        .move_window_to_output(&window, &requested_output);
                }
            }

            self.niri.layout.set_fullscreen(&window, true);

            // A configure is required in response to this event regardless if there are pending
            // changes.
            toplevel.send_configure();
        } else if let Some(unmapped) = self.niri.unmapped_windows.get_mut(toplevel.wl_surface()) {
            match &mut unmapped.state {
                InitialConfigureState::NotConfigured { wants_fullscreen } => {
                    *wants_fullscreen = Some(requested_output);

                    // The required configure will be the initial configure.
                }
                InitialConfigureState::Configured { rules, output, .. } => {
                    // Figure out the monitor following a similar logic to initial configure.
                    // FIXME: deduplicate.
                    let mon = requested_output
                        .as_ref()
                        // If none requested, try currently configured output.
                        .or(output.as_ref())
                        .and_then(|o| self.niri.layout.monitor_for_output(o))
                        .map(|mon| (mon, false))
                        // If not, check if we have a parent with a monitor.
                        .or_else(|| {
                            toplevel
                                .parent()
                                .and_then(|parent| self.niri.layout.find_window_and_output(&parent))
                                .map(|(_win, output)| output)
                                .and_then(|o| self.niri.layout.monitor_for_output(o))
                                .map(|mon| (mon, true))
                        })
                        // If not, fall back to the active monitor.
                        .or_else(|| {
                            self.niri
                                .layout
                                .active_monitor_ref()
                                .map(|mon| (mon, false))
                        });

                    *output = mon
                        .filter(|(_, parent)| !parent)
                        .map(|(mon, _)| mon.output.clone());
                    let mon = mon.map(|(mon, _)| mon);

                    let ws = mon
                        .map(|mon| mon.active_workspace_ref())
                        .or_else(|| self.niri.layout.active_workspace());

                    if let Some(ws) = ws {
                        toplevel.with_pending_state(|state| {
                            state.states.set(xdg_toplevel::State::Fullscreen);
                        });
                        ws.configure_new_window(&unmapped.window, None, rules);
                    }

                    // We already sent the initial configure, so we need to reconfigure.
                    toplevel.send_configure();
                }
            }
        } else {
            error!("couldn't find the toplevel in fullscreen_request()");
            toplevel.send_configure();
        }
    }

    fn unfullscreen_request(&mut self, toplevel: ToplevelSurface) {
        if let Some((mapped, _)) = self
            .niri
            .layout
            .find_window_and_output(toplevel.wl_surface())
        {
            let window = mapped.window.clone();
            self.niri.layout.set_fullscreen(&window, false);

            // A configure is required in response to this event regardless if there are pending
            // changes.
            toplevel.send_configure();
        } else if let Some(unmapped) = self.niri.unmapped_windows.get_mut(toplevel.wl_surface()) {
            match &mut unmapped.state {
                InitialConfigureState::NotConfigured { wants_fullscreen } => {
                    *wants_fullscreen = None;

                    // The required configure will be the initial configure.
                }
                InitialConfigureState::Configured {
                    rules,
                    width,
                    is_full_width,
                    output,
                } => {
                    // Figure out the monitor following a similar logic to initial configure.
                    // FIXME: deduplicate.
                    let mon = output
                        .as_ref()
                        .and_then(|o| self.niri.layout.monitor_for_output(o))
                        .map(|mon| (mon, false))
                        // If not, check if we have a parent with a monitor.
                        .or_else(|| {
                            toplevel
                                .parent()
                                .and_then(|parent| self.niri.layout.find_window_and_output(&parent))
                                .map(|(_win, output)| output)
                                .and_then(|o| self.niri.layout.monitor_for_output(o))
                                .map(|mon| (mon, true))
                        })
                        // If not, fall back to the active monitor.
                        .or_else(|| {
                            self.niri
                                .layout
                                .active_monitor_ref()
                                .map(|mon| (mon, false))
                        });

                    *output = mon
                        .filter(|(_, parent)| !parent)
                        .map(|(mon, _)| mon.output.clone());
                    let mon = mon.map(|(mon, _)| mon);

                    let ws = mon
                        .map(|mon| mon.active_workspace_ref())
                        .or_else(|| self.niri.layout.active_workspace());

                    if let Some(ws) = ws {
                        toplevel.with_pending_state(|state| {
                            state.states.unset(xdg_toplevel::State::Fullscreen);
                        });

                        let configure_width = if *is_full_width {
                            Some(ColumnWidth::Proportion(1.))
                        } else {
                            *width
                        };
                        ws.configure_new_window(&unmapped.window, configure_width, rules);
                    }

                    // We already sent the initial configure, so we need to reconfigure.
                    toplevel.send_configure();
                }
            }
        } else {
            error!("couldn't find the toplevel in unfullscreen_request()");
            toplevel.send_configure();
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

        let win_out = self
            .niri
            .layout
            .find_window_and_output(surface.wl_surface());

        let Some((mapped, output)) = win_out else {
            // I have no idea how this can happen, but I saw it happen once, in a weird interaction
            // involving laptop going to sleep and resuming.
            error!("toplevel missing from both unmapped_windows and layout");
            return;
        };
        let window = mapped.window.clone();
        let output = output.clone();

        self.backend.with_primary_renderer(|renderer| {
            mapped.store_unmap_snapshot_if_empty(renderer);
        });
        self.backend.with_primary_renderer(|renderer| {
            self.niri
                .layout
                .start_close_animation_for_window(renderer, &window);
        });

        let active_window = self.niri.layout.active_window().map(|(m, _)| &m.window);
        let was_active = active_window == Some(&window);

        self.niri.layout.remove_window(&window);

        if was_active {
            self.maybe_warp_cursor_to_focus();
        }

        self.niri.queue_redraw(&output);
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        if let Some(output) = self.output_for_popup(&PopupKind::Xdg(surface)) {
            self.niri.queue_redraw(&output.clone());
        }
    }

    fn app_id_changed(&mut self, toplevel: ToplevelSurface) {
        self.update_window_rules(&toplevel);
    }

    fn title_changed(&mut self, toplevel: ToplevelSurface) {
        self.update_window_rules(&toplevel);
    }
}

delegate_xdg_shell!(State);

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // If we want CSD, we hide this global altogether.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ServerSide);
        });
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: zxdg_toplevel_decoration_v1::Mode) {
        // Set whatever the client wants, rather than our preferred mode. This especially matters
        // for SDL2 which has a bug where forcing a different (client-side) decoration mode during
        // their window creation sequence would leave the window permanently hidden.
        //
        // https://github.com/libsdl-org/SDL/issues/8173
        //
        // The bug has been fixed, but there's a ton of apps which will use the buggy version for a
        // long while...
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
        });

        // A configure is required in response to this event. However, if an initial configure
        // wasn't sent, then we will send this as part of the initial configure later.
        if initial_configure_sent(&toplevel) {
            toplevel.send_configure();
        }
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        // If we want CSD, we hide this global altogether.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(zxdg_toplevel_decoration_v1::Mode::ServerSide);
        });

        // A configure is required in response to this event. However, if an initial configure
        // wasn't sent, then we will send this as part of the initial configure later.
        if initial_configure_sent(&toplevel) {
            toplevel.send_configure();
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

impl XdgForeignHandler for State {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.niri.xdg_foreign_state
    }
}
delegate_xdg_foreign!(State);

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
    pub fn send_initial_configure(&mut self, toplevel: &ToplevelSurface) {
        let _span = tracy_client::span!("State::send_initial_configure");

        let Some(unmapped) = self.niri.unmapped_windows.get_mut(toplevel.wl_surface()) else {
            error!("window must be present in unmapped_windows in send_initial_configure()");
            return;
        };

        let config = self.niri.config.borrow();
        let rules =
            ResolvedWindowRules::compute(&config.window_rules, WindowRef::Unmapped(unmapped));

        let Unmapped { window, state } = unmapped;

        let InitialConfigureState::NotConfigured { wants_fullscreen } = state else {
            error!("window must not be already configured in send_initial_configure()");
            return;
        };

        // Pick the target monitor. First, check if we had an output set in the window rules.
        let mon = rules
            .open_on_output
            .as_deref()
            .and_then(|name| self.niri.output_by_name.get(name))
            .and_then(|o| self.niri.layout.monitor_for_output(o));

        // If not, check if the window requested one for fullscreen.
        let mon = mon.or_else(|| {
            wants_fullscreen
                .as_ref()
                .and_then(|x| x.as_ref())
                // The monitor might not exist if the output was disconnected.
                .and_then(|o| self.niri.layout.monitor_for_output(o))
        });

        // If not, check if this is a dialog with a parent, to place it next to the parent.
        let mon = mon.map(|mon| (mon, false)).or_else(|| {
            toplevel
                .parent()
                .and_then(|parent| self.niri.layout.find_window_and_output(&parent))
                .map(|(_win, output)| output)
                .and_then(|o| self.niri.layout.monitor_for_output(o))
                .map(|mon| (mon, true))
        });

        // If not, use the active monitor.
        let mon = mon.or_else(|| {
            self.niri
                .layout
                .active_monitor_ref()
                .map(|mon| (mon, false))
        });

        // If we're following the parent, don't set the target output, so that when the window is
        // mapped, it fetches the possibly changed parent's output again, and shows up there.
        let output = mon
            .filter(|(_, parent)| !parent)
            .map(|(mon, _)| mon.output.clone());
        let mon = mon.map(|(mon, _)| mon);

        let mut width = None;
        let is_full_width = rules.open_maximized.unwrap_or(false);

        // Tell the surface the preferred size and bounds for its likely output.
        let ws = mon
            .map(|mon| mon.active_workspace_ref())
            .or_else(|| self.niri.layout.active_workspace());

        if let Some(ws) = ws {
            // Set a fullscreen state based on window request and window rule.
            if (wants_fullscreen.is_some() && rules.open_fullscreen.is_none())
                || rules.open_fullscreen == Some(true)
            {
                toplevel.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Fullscreen);
                });
            }

            width = ws.resolve_default_width(rules.default_width);

            let configure_width = if is_full_width {
                Some(ColumnWidth::Proportion(1.))
            } else {
                width
            };
            ws.configure_new_window(window, configure_width, &rules);
        }

        // If the user prefers no CSD, it's a reasonable assumption that they would prefer to get
        // rid of the various client-side rounded corners also by using the tiled state.
        if config.prefer_no_csd {
            toplevel.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::TiledLeft);
                state.states.set(xdg_toplevel::State::TiledRight);
                state.states.set(xdg_toplevel::State::TiledTop);
                state.states.set(xdg_toplevel::State::TiledBottom);
            });
        }

        // Set the configured settings.
        *state = InitialConfigureState::Configured {
            rules,
            width,
            is_full_width,
            output,
        };

        toplevel.send_configure();
    }

    pub fn queue_initial_configure(&self, toplevel: ToplevelSurface) {
        // Send the initial configure in an idle, in case the client sent some more info after the
        // initial commit.
        self.niri.event_loop.insert_idle(move |state| {
            if !toplevel.alive() {
                return;
            }

            if let Some(unmapped) = state.niri.unmapped_windows.get(toplevel.wl_surface()) {
                if unmapped.needs_initial_configure() {
                    state.send_initial_configure(&toplevel);
                }
            }
        });
    }

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
                        if let Some(output) = self.output_for_popup(&PopupKind::Xdg(popup.clone()))
                        {
                            let scale = output.current_scale().integer_scale();
                            let transform = output.current_transform();
                            with_states(surface, |data| {
                                send_surface_state(surface, data, scale, transform);
                            });
                        }
                        popup.send_configure().expect("initial configure failed");
                    }
                }
                // Input method popup can arbitrary change its geometry, so we need to unconstrain
                // it on commit.
                PopupKind::InputMethod(_) => {
                    self.unconstrain_popup(&popup);
                }
            }
        }
    }

    pub fn output_for_popup(&self, popup: &PopupKind) -> Option<&Output> {
        let root = find_popup_root_surface(popup).ok()?;
        self.niri.output_for_root(&root)
    }

    pub fn unconstrain_popup(&self, popup: &PopupKind) {
        let _span = tracy_client::span!("Niri::unconstrain_popup");

        // Popups with a NULL parent will get repositioned in their respective protocol handlers
        // (i.e. layer-shell).
        let Ok(root) = find_popup_root_surface(popup) else {
            return;
        };

        // Figure out if the root is a window or a layer surface.
        if let Some((mapped, output)) = self.niri.layout.find_window_and_output(&root) {
            self.unconstrain_window_popup(popup, &mapped.window, output);
        } else if let Some((layer_surface, output)) = self.niri.layout.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let layer_surface = map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)?;
            Some((layer_surface.clone(), o))
        }) {
            self.unconstrain_layer_shell_popup(popup, &layer_surface, output);
        }
    }

    fn unconstrain_window_popup(&self, popup: &PopupKind, window: &Window, output: &Output) {
        let window_geo = window.geometry();
        let output_geo = self.niri.global_space.output_geometry(output).unwrap();

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        //
        // We try to keep regular window popups within the window itself horizontally (since the
        // window can be scrolled to both edges of the screen), but within the whole monitor's
        // height.
        let mut target =
            Rectangle::from_loc_and_size((0, 0), (window_geo.size.w, output_geo.size.h));
        target.loc.y -= self.niri.layout.window_y(window).unwrap();
        target.loc -= get_popup_toplevel_coords(popup);

        self.position_popup_within_rect(popup, target);
    }

    pub fn unconstrain_layer_shell_popup(
        &self,
        popup: &PopupKind,
        layer_surface: &LayerSurface,
        output: &Output,
    ) {
        let output_geo = self.niri.global_space.output_geometry(output).unwrap();
        let map = layer_map_for_output(output);
        let Some(layer_geo) = map.layer_geometry(layer_surface) else {
            return;
        };

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        let mut target = Rectangle::from_loc_and_size((0, 0), output_geo.size);
        target.loc -= layer_geo.loc;
        target.loc -= get_popup_toplevel_coords(popup);

        self.position_popup_within_rect(popup, target);
    }

    fn position_popup_within_rect(&self, popup: &PopupKind, target: Rectangle<i32, Logical>) {
        match popup {
            PopupKind::Xdg(popup) => {
                popup.with_pending_state(|state| {
                    state.geometry = unconstrain_with_padding(state.positioner, target);
                });
            }
            PopupKind::InputMethod(popup) => {
                let text_input_rectangle = popup.text_input_rectangle();
                let mut bbox =
                    utils::bbox_from_surface_tree(popup.wl_surface(), text_input_rectangle.loc);

                // Position bbox horizontally first.
                let overflow_x = (bbox.loc.x + bbox.size.w) - (target.loc.x + target.size.w);
                if overflow_x > 0 {
                    bbox.loc.x -= overflow_x;
                }

                // Ensure that the popup starts within the window.
                bbox.loc.x = bbox.loc.x.max(target.loc.x);

                // Try to position IME popup below the text input rectangle.
                let mut below = bbox;
                below.loc.y += text_input_rectangle.size.h;

                let mut above = bbox;
                above.loc.y -= bbox.size.h;

                if target.loc.y + target.size.h >= below.loc.y + below.size.h {
                    popup.set_location(below.loc);
                } else {
                    popup.set_location(above.loc);
                }
            }
        }
    }

    pub fn update_reactive_popups(&self, window: &Window, output: &Output) {
        let _span = tracy_client::span!("Niri::update_reactive_popups");

        for (popup, _) in PopupManager::popups_for_surface(
            window.toplevel().expect("no x11 support").wl_surface(),
        ) {
            match &popup {
                xdg_popup @ PopupKind::Xdg(popup) => {
                    if popup.with_pending_state(|state| state.positioner.reactive) {
                        self.unconstrain_window_popup(xdg_popup, window, output);
                        if let Err(err) = popup.send_pending_configure() {
                            warn!("error re-configuring reactive popup: {err:?}");
                        }
                    }
                }
                PopupKind::InputMethod(_) => (),
            }
        }
    }

    pub fn update_window_rules(&mut self, toplevel: &ToplevelSurface) {
        let config = self.niri.config.borrow();
        let window_rules = &config.window_rules;

        if let Some(unmapped) = self.niri.unmapped_windows.get_mut(toplevel.wl_surface()) {
            let new_rules =
                ResolvedWindowRules::compute(window_rules, WindowRef::Unmapped(unmapped));
            if let InitialConfigureState::Configured { rules, .. } = &mut unmapped.state {
                *rules = new_rules;
            }
        } else if let Some((mapped, output)) = self
            .niri
            .layout
            .find_window_and_output_mut(toplevel.wl_surface())
        {
            if mapped.recompute_window_rules(window_rules) {
                drop(config);
                let output = output.cloned();
                let window = mapped.window.clone();
                self.niri.layout.update_window(&window);

                if let Some(output) = output {
                    self.niri.queue_redraw(&output);
                }
            }
        }
    }
}

fn unconstrain_with_padding(
    positioner: PositionerState,
    target: Rectangle<i32, Logical>,
) -> Rectangle<i32, Logical> {
    // Try unconstraining with a small padding first which looks nicer, then if it doesn't fit try
    // unconstraining without padding.
    const PADDING: i32 = 8;

    let mut padded = target;
    if PADDING * 2 < padded.size.w {
        padded.loc.x += PADDING;
        padded.size.w -= PADDING * 2;
    }
    if PADDING * 2 < padded.size.h {
        padded.loc.y += PADDING;
        padded.size.h -= PADDING * 2;
    }

    // No padding, so just unconstrain with the original target.
    if padded == target {
        return positioner.get_unconstrained_geometry(target);
    }

    // Do not try to resize to fit the padded target rectangle.
    let mut no_resize = positioner;
    no_resize
        .constraint_adjustment
        .remove(ConstraintAdjustment::ResizeX);
    no_resize
        .constraint_adjustment
        .remove(ConstraintAdjustment::ResizeY);

    let geo = no_resize.get_unconstrained_geometry(padded);
    if padded.contains_rect(geo) {
        return geo;
    }

    // Could not unconstrain into the padded target, so resort to the regular one.
    positioner.get_unconstrained_geometry(target)
}

pub fn add_mapped_toplevel_pre_commit_hook(toplevel: &ToplevelSurface) -> HookId {
    add_pre_commit_hook::<State, _>(toplevel.wl_surface(), move |state, _dh, surface| {
        let _span = tracy_client::span!("mapped toplevel pre-commit");

        let Some((mapped, _)) = state.niri.layout.find_window_and_output_mut(surface) else {
            error!("pre-commit hook for mapped surfaces must be removed upon unmapping");
            return;
        };

        let (got_unmapped, commit_serial) = with_states(surface, |states| {
            let attrs = states.cached_state.pending::<SurfaceAttributes>();
            let got_unmapped = matches!(attrs.buffer, Some(BufferAssignment::Removed));

            let role = states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap();

            (got_unmapped, role.configure_serial)
        });

        let animate = if let Some(serial) = commit_serial {
            mapped.should_animate_commit(serial)
        } else {
            error!("commit on a mapped surface without a configured serial");
            false
        };

        if got_unmapped {
            state.backend.with_primary_renderer(|renderer| {
                mapped.store_unmap_snapshot_if_empty(renderer);
            });
        } else {
            // The toplevel remains mapped; clear any stored unmap snapshot.
            let _ = mapped.take_unmap_snapshot();

            if animate {
                state.backend.with_primary_renderer(|renderer| {
                    mapped.store_animation_snapshot(renderer);
                });

                let window = mapped.window.clone();
                state.niri.layout.prepare_for_resize_animation(&window);
            }
        }
    })
}
