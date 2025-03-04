use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::time::Duration;

use hyprland_global_shortcut_v1::HyprlandGlobalShortcutV1;
use hyprland_global_shortcuts_manager_v1::HyprlandGlobalShortcutsManagerV1;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use super::raw::hyprland_global_shortcuts::v1::server::{
    hyprland_global_shortcut_v1, hyprland_global_shortcuts_manager_v1,
};

const VERSION: u32 = 1;

pub struct HyprlandGlobalShortcutsManagerState {
    // Keys are app_id + id pairs
    shortcuts: HashMap<(String, String), HyprlandGlobalShortcut>,
}

pub struct HyprlandGlobalShortcutsManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

pub trait HyprlandGlobalShortcutsHandler {
    fn hyprland_global_shortcuts_state(&mut self) -> &mut HyprlandGlobalShortcutsManagerState;
    fn shortcut_registered(&mut self, shortcut: &HyprlandGlobalShortcut);
    fn shortcut_destroyed(&mut self, shortcut: &HyprlandGlobalShortcut);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HyprlandGlobalShortcut {
    shortcut: HyprlandGlobalShortcutV1,
    pub id: String,
    pub app_id: String,
    pub description: String,
    pub trigger_description: String,
}

#[derive(Debug)]
pub struct HyprlandGlobalShortcutUserData {
    id: String,
    app_id: String,
}

impl HyprlandGlobalShortcut {
    pub fn press(&self, timestamp: Duration) {
        let tv_sec_hi = (timestamp.as_secs() >> 32) as u32;
        let tv_sec_lo = (timestamp.as_secs() & 0xFFFFFFFF) as u32;
        let tv_nsec = timestamp.subsec_nanos();
        self.shortcut.pressed(tv_sec_hi, tv_sec_lo, tv_nsec);
    }
    pub fn release(&self, timestamp: Duration) {
        let tv_sec_hi = (timestamp.as_secs() >> 32) as u32;
        let tv_sec_lo = (timestamp.as_secs() & 0xFFFFFFFF) as u32;
        let tv_nsec = timestamp.subsec_nanos();
        self.shortcut.pressed(tv_sec_hi, tv_sec_lo, tv_nsec);
    }
}

impl HyprlandGlobalShortcutsManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<
            HyprlandGlobalShortcutsManagerV1,
            HyprlandGlobalShortcutsManagerGlobalData,
        >,
        D: Dispatch<HyprlandGlobalShortcutsManagerV1, ()>,
        D: HyprlandGlobalShortcutsHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = HyprlandGlobalShortcutsManagerGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, HyprlandGlobalShortcutsManagerV1, _>(VERSION, global_data);

        Self {
            shortcuts: HashMap::new(),
        }
    }

    pub fn shortcut(&self, app_id: String, id: String) -> Option<HyprlandGlobalShortcut> {
        self.shortcuts.get(&(app_id, id)).cloned()
    }

    pub fn shortcuts(&self) -> impl Iterator<Item = &HyprlandGlobalShortcut> {
        self.shortcuts.values()
    }
}

impl<D>
    GlobalDispatch<HyprlandGlobalShortcutsManagerV1, HyprlandGlobalShortcutsManagerGlobalData, D>
    for HyprlandGlobalShortcutsManagerState
where
    D: GlobalDispatch<HyprlandGlobalShortcutsManagerV1, HyprlandGlobalShortcutsManagerGlobalData>,
    D: Dispatch<HyprlandGlobalShortcutsManagerV1, ()>,
    D: HyprlandGlobalShortcutsHandler,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        manager: New<HyprlandGlobalShortcutsManagerV1>,
        _manager_state: &HyprlandGlobalShortcutsManagerGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(manager, ());
    }

    fn can_view(client: Client, global_data: &HyprlandGlobalShortcutsManagerGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<HyprlandGlobalShortcutsManagerV1, (), D> for HyprlandGlobalShortcutsManagerState
where
    D: Dispatch<HyprlandGlobalShortcutsManagerV1, ()>,
    D: Dispatch<HyprlandGlobalShortcutV1, HyprlandGlobalShortcutUserData>,
    D: HyprlandGlobalShortcutsHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &HyprlandGlobalShortcutsManagerV1,
        request: <HyprlandGlobalShortcutsManagerV1 as Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            hyprland_global_shortcuts_manager_v1::Request::RegisterShortcut {
                shortcut,
                id,
                app_id,
                description,
                trigger_description,
            } => {
                let shortcut = HyprlandGlobalShortcut {
                    shortcut: data_init.init(
                        shortcut,
                        HyprlandGlobalShortcutUserData {
                            app_id: app_id.clone(),
                            id: id.clone(),
                        },
                    ),
                    id: id.clone(),
                    app_id: app_id.clone(),
                    description,
                    trigger_description,
                };
                let shortcuts_state = state.hyprland_global_shortcuts_state();
                match shortcuts_state.shortcuts.entry((app_id, id)) {
                    Entry::Occupied(_) => {
                        resource.post_error(
                            hyprland_global_shortcuts_manager_v1::Error::AlreadyTaken,
                            "app_id and id combination already taken",
                        );
                        return;
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(shortcut.clone());
                    }
                }
                state.shortcut_registered(&shortcut);
            }
            hyprland_global_shortcuts_manager_v1::Request::Destroy => (),
        }
    }
}

impl<D> Dispatch<HyprlandGlobalShortcutV1, HyprlandGlobalShortcutUserData, D>
    for HyprlandGlobalShortcutsManagerState
where
    D: Dispatch<HyprlandGlobalShortcutV1, HyprlandGlobalShortcutUserData>,
    D: HyprlandGlobalShortcutsHandler,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        _resource: &HyprlandGlobalShortcutV1,
        request: <HyprlandGlobalShortcutV1 as Resource>::Request,
        _data: &HyprlandGlobalShortcutUserData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            hyprland_global_shortcut_v1::Request::Destroy => (),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_backend::server::ClientId,
        _resource: &HyprlandGlobalShortcutV1,
        data: &HyprlandGlobalShortcutUserData,
    ) {
        let shortcuts_state = state.hyprland_global_shortcuts_state();
        if let Some(shortcut) = shortcuts_state
            .shortcuts
            .remove(&(data.app_id.clone(), data.id.clone()))
        {
            state.shortcut_destroyed(&shortcut);
        } else {
            warn!("destroyed global shortcut object with missing global shortcut");
        }
    }
}

#[macro_export]
macro_rules! delegate_hyprland_global_shortcuts {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::hyprland_global_shortcuts::v1::server::hyprland_global_shortcuts_manager_v1::HyprlandGlobalShortcutsManagerV1: $crate::protocols::hyprland_global_shortcuts::HyprlandGlobalShortcutsManagerGlobalData
        ] => $crate::protocols::hyprland_global_shortcuts::HyprlandGlobalShortcutsManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::hyprland_global_shortcuts::v1::server::hyprland_global_shortcuts_manager_v1::HyprlandGlobalShortcutsManagerV1: ()
        ] => $crate::protocols::hyprland_global_shortcuts::HyprlandGlobalShortcutsManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::hyprland_global_shortcuts::v1::server::hyprland_global_shortcut_v1::HyprlandGlobalShortcutV1:  $crate::protocols::hyprland_global_shortcuts::HyprlandGlobalShortcutUserData
        ] => $crate::protocols::hyprland_global_shortcuts::HyprlandGlobalShortcutsManagerState);
    };
}
