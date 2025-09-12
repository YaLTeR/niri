use std::collections::hash_map::Entry;
use std::collections::HashMap;

use arrayvec::ArrayVec;
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_protocols_wlr;
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::wayland::shell::xdg::{
    ToplevelState, ToplevelStateSet, XdgToplevelSurfaceRoleAttributes,
};
use wayland_protocols_wlr::foreign_toplevel::v1::server::{
    zwlr_foreign_toplevel_handle_v1, zwlr_foreign_toplevel_manager_v1,
};
use zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;

use crate::niri::State;
use crate::utils::with_toplevel_role_and_current;

const VERSION: u32 = 3;

pub struct ForeignToplevelManagerState {
    display: DisplayHandle,
    instances: Vec<ZwlrForeignToplevelManagerV1>,
    toplevels: HashMap<WlSurface, ToplevelData>,
}

pub trait ForeignToplevelHandler {
    fn foreign_toplevel_manager_state(&mut self) -> &mut ForeignToplevelManagerState;
    fn activate(&mut self, wl_surface: WlSurface);
    fn close(&mut self, wl_surface: WlSurface);
    fn set_fullscreen(&mut self, wl_surface: WlSurface, wl_output: Option<WlOutput>);
    fn unset_fullscreen(&mut self, wl_surface: WlSurface);
    fn set_maximized(&mut self, wl_surface: WlSurface);
    fn unset_maximized(&mut self, wl_surface: WlSurface);
}

struct ToplevelData {
    title: Option<String>,
    app_id: Option<String>,
    states: ArrayVec<u32, 3>,
    output: Option<Output>,
    instances: HashMap<ZwlrForeignToplevelHandleV1, Vec<WlOutput>>,
    // FIXME: parent.
}

pub struct ForeignToplevelGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

impl ForeignToplevelManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<ZwlrForeignToplevelManagerV1, ForeignToplevelGlobalData>,
        D: Dispatch<ZwlrForeignToplevelManagerV1, ()>,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = ForeignToplevelGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, ZwlrForeignToplevelManagerV1, _>(VERSION, global_data);
        Self {
            display: display.clone(),
            instances: Vec::new(),
            toplevels: HashMap::new(),
        }
    }
}

pub fn refresh(state: &mut State) {
    let _span = tracy_client::span!("foreign_toplevel::refresh");

    let protocol_state = &mut state.niri.foreign_toplevel_state;

    // Handle closed windows.
    protocol_state.toplevels.retain(|surface, data| {
        if state.niri.layout.find_window_and_output(surface).is_some() {
            return true;
        }

        for instance in data.instances.keys() {
            instance.closed();
        }

        false
    });

    // Handle new and existing windows.
    //
    // Save the focused window for last, this way when the focus changes, we will first deactivate
    // the previous window and only then activate the newly focused window.
    let mut focused = None;
    state.niri.layout.with_windows(|mapped, output, _, _| {
        let toplevel = mapped.toplevel();
        let wl_surface = toplevel.wl_surface();
        with_toplevel_role_and_current(toplevel, |role, cur| {
            let Some(cur) = cur else {
                error!("mapped must have had initial commit");
                return;
            };

            if state.niri.keyboard_focus.surface() == Some(wl_surface) {
                focused = Some((mapped.window.clone(), output.cloned()));
            } else {
                refresh_toplevel(protocol_state, wl_surface, role, cur, output, false);
            }
        });
    });

    // Finally, refresh the focused window.
    if let Some((window, output)) = focused {
        let toplevel = window.toplevel().expect("no X11 support");
        let wl_surface = toplevel.wl_surface();
        with_toplevel_role_and_current(toplevel, |role, cur| {
            let Some(cur) = cur else {
                error!("mapped must have had initial commit");
                return;
            };

            refresh_toplevel(protocol_state, wl_surface, role, cur, output.as_ref(), true);
        });
    }
}

pub fn on_output_bound(state: &mut State, output: &Output, wl_output: &WlOutput) {
    let _span = tracy_client::span!("foreign_toplevel::on_output_bound");

    let Some(client) = wl_output.client() else {
        return;
    };

    let protocol_state = &mut state.niri.foreign_toplevel_state;
    for data in protocol_state.toplevels.values_mut() {
        if data.output.as_ref() != Some(output) {
            continue;
        }

        for (instance, outputs) in &mut data.instances {
            if instance.client().as_ref() != Some(&client) {
                continue;
            }

            instance.output_enter(wl_output);
            instance.done();
            outputs.push(wl_output.clone());
        }
    }
}

fn refresh_toplevel(
    protocol_state: &mut ForeignToplevelManagerState,
    wl_surface: &WlSurface,
    role: &XdgToplevelSurfaceRoleAttributes,
    current: &ToplevelState,
    output: Option<&Output>,
    has_focus: bool,
) {
    let states = to_state_vec(&current.states, has_focus);

    match protocol_state.toplevels.entry(wl_surface.clone()) {
        Entry::Occupied(entry) => {
            // Existing window, check if anything changed.
            let data = entry.into_mut();

            let mut new_title = None;
            if data.title != role.title {
                data.title.clone_from(&role.title);
                new_title = role.title.as_deref();

                if new_title.is_none() {
                    error!("toplevel title changed to None");
                }
            }

            let mut new_app_id = None;
            if data.app_id != role.app_id {
                data.app_id.clone_from(&role.app_id);
                new_app_id = role.app_id.as_deref();

                if new_app_id.is_none() {
                    error!("toplevel app_id changed to None");
                }
            }

            let mut states_changed = false;
            if data.states != states {
                data.states = states;
                states_changed = true;
            }

            let mut output_changed = false;
            if data.output.as_ref() != output {
                data.output = output.cloned();
                output_changed = true;
            }

            let something_changed =
                new_title.is_some() || new_app_id.is_some() || states_changed || output_changed;

            if something_changed {
                for (instance, outputs) in &mut data.instances {
                    if let Some(new_title) = new_title {
                        instance.title(new_title.to_owned());
                    }
                    if let Some(new_app_id) = new_app_id {
                        instance.app_id(new_app_id.to_owned());
                    }
                    if states_changed {
                        instance.state(data.states.iter().flat_map(|x| x.to_ne_bytes()).collect());
                    }
                    if output_changed {
                        for wl_output in outputs.drain(..) {
                            instance.output_leave(&wl_output);
                        }
                        if let Some(output) = &data.output {
                            if let Some(client) = instance.client() {
                                for wl_output in output.client_outputs(&client) {
                                    instance.output_enter(&wl_output);
                                    outputs.push(wl_output);
                                }
                            }
                        }
                    }
                    instance.done();
                }
            }

            for outputs in data.instances.values_mut() {
                // Clean up dead wl_outputs.
                outputs.retain(|x| x.is_alive());
            }
        }
        Entry::Vacant(entry) => {
            // New window, start tracking it.
            let mut data = ToplevelData {
                title: role.title.clone(),
                app_id: role.app_id.clone(),
                states,
                output: output.cloned(),
                instances: HashMap::new(),
            };

            for manager in &protocol_state.instances {
                if let Some(client) = manager.client() {
                    data.add_instance::<State>(&protocol_state.display, &client, manager);
                }
            }

            entry.insert(data);
        }
    }
}

impl ToplevelData {
    fn add_instance<D>(
        &mut self,
        handle: &DisplayHandle,
        client: &Client,
        manager: &ZwlrForeignToplevelManagerV1,
    ) where
        D: Dispatch<ZwlrForeignToplevelHandleV1, ()>,
        D: 'static,
    {
        let toplevel = client
            .create_resource::<ZwlrForeignToplevelHandleV1, _, D>(handle, manager.version(), ())
            .unwrap();
        manager.toplevel(&toplevel);

        if let Some(title) = &self.title {
            toplevel.title(title.clone());
        }
        if let Some(app_id) = &self.app_id {
            toplevel.app_id(app_id.clone());
        }

        toplevel.state(self.states.iter().flat_map(|x| x.to_ne_bytes()).collect());

        let mut outputs = Vec::new();
        if let Some(output) = &self.output {
            for wl_output in output.client_outputs(client) {
                toplevel.output_enter(&wl_output);
                outputs.push(wl_output);
            }
        }

        toplevel.done();

        self.instances.insert(toplevel, outputs);
    }
}

impl<D> GlobalDispatch<ZwlrForeignToplevelManagerV1, ForeignToplevelGlobalData, D>
    for ForeignToplevelManagerState
where
    D: GlobalDispatch<ZwlrForeignToplevelManagerV1, ForeignToplevelGlobalData>,
    D: Dispatch<ZwlrForeignToplevelManagerV1, ()>,
    D: Dispatch<ZwlrForeignToplevelHandleV1, ()>,
    D: ForeignToplevelHandler,
{
    fn bind(
        state: &mut D,
        handle: &DisplayHandle,
        client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _global_data: &ForeignToplevelGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        let manager = data_init.init(resource, ());

        let state = state.foreign_toplevel_manager_state();

        for data in state.toplevels.values_mut() {
            data.add_instance::<D>(handle, client, &manager);
        }

        state.instances.push(manager);
    }

    fn can_view(client: Client, global_data: &ForeignToplevelGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ZwlrForeignToplevelManagerV1, (), D> for ForeignToplevelManagerState
where
    D: Dispatch<ZwlrForeignToplevelManagerV1, ()>,
    D: ForeignToplevelHandler,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ZwlrForeignToplevelManagerV1,
        request: <ZwlrForeignToplevelManagerV1 as Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_foreign_toplevel_manager_v1::Request::Stop => {
                resource.finished();

                let state = state.foreign_toplevel_manager_state();
                state.instances.retain(|x| x != resource);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: ClientId,
        resource: &ZwlrForeignToplevelManagerV1,
        _data: &(),
    ) {
        let state = state.foreign_toplevel_manager_state();
        state.instances.retain(|x| x != resource);
    }
}

impl<D> Dispatch<ZwlrForeignToplevelHandleV1, (), D> for ForeignToplevelManagerState
where
    D: Dispatch<ZwlrForeignToplevelHandleV1, ()>,
    D: ForeignToplevelHandler,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ZwlrForeignToplevelHandleV1,
        request: <ZwlrForeignToplevelHandleV1 as Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        let protocol_state = state.foreign_toplevel_manager_state();

        let Some((surface, _)) = protocol_state
            .toplevels
            .iter()
            .find(|(_, data)| data.instances.contains_key(resource))
        else {
            return;
        };
        let surface = surface.clone();

        match request {
            zwlr_foreign_toplevel_handle_v1::Request::SetMaximized => state.set_maximized(surface),
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMaximized => {
                state.unset_maximized(surface)
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetMinimized => (),
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMinimized => (),
            zwlr_foreign_toplevel_handle_v1::Request::Activate { .. } => {
                state.activate(surface);
            }
            zwlr_foreign_toplevel_handle_v1::Request::Close => {
                state.close(surface);
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetRectangle { .. } => (),
            zwlr_foreign_toplevel_handle_v1::Request::Destroy => (),
            zwlr_foreign_toplevel_handle_v1::Request::SetFullscreen { output } => {
                state.set_fullscreen(surface, output);
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetFullscreen => {
                state.unset_fullscreen(surface);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: ClientId,
        resource: &ZwlrForeignToplevelHandleV1,
        _data: &(),
    ) {
        let state = state.foreign_toplevel_manager_state();
        for data in state.toplevels.values_mut() {
            data.instances.retain(|instance, _| instance != resource);
        }
    }
}

fn to_state_vec(states: &ToplevelStateSet, has_focus: bool) -> ArrayVec<u32, 3> {
    let mut rv = ArrayVec::new();
    if states.contains(xdg_toplevel::State::Maximized) {
        rv.push(zwlr_foreign_toplevel_handle_v1::State::Maximized as u32);
    }
    if states.contains(xdg_toplevel::State::Fullscreen) {
        rv.push(zwlr_foreign_toplevel_handle_v1::State::Fullscreen as u32);
    }

    // HACK: wlr-foreign-toplevel-management states:
    //
    // These have the same meaning as the states with the same names defined in xdg-toplevel
    //
    // However, clients such as sfwbar and fcitx seem to treat the activated state as keyboard
    // focus, i.e. they don't expect multiple windows to have it set at once. Even Waybar which
    // handles multiple activated windows correctly uses it in its design in such a way that
    // keyboard focus would make more sense. Let's do what the clients expect.
    if has_focus {
        rv.push(zwlr_foreign_toplevel_handle_v1::State::Activated as u32);
    }

    rv
}

#[macro_export]
macro_rules! delegate_foreign_toplevel {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1: $crate::protocols::foreign_toplevel::ForeignToplevelGlobalData
        ] => $crate::protocols::foreign_toplevel::ForeignToplevelManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1: ()
        ] => $crate::protocols::foreign_toplevel::ForeignToplevelManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1: ()
        ] => $crate::protocols::foreign_toplevel::ForeignToplevelManagerState);
    };
}
