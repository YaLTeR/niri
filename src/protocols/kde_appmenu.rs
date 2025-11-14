use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New,
};
use wayland_backend::server::GlobalId;

use super::raw::kde_appmenu::v2::server::org_kde_kwin_appmenu_manager::{
    OrgKdeKwinAppmenuManager, Request as ManagerRequest,
};
use crate::protocols::raw::kde_appmenu::v2::server::org_kde_kwin_appmenu::{
    OrgKdeKwinAppmenu, Request as AppmenuRequest,
};

const APPMENU_VERSION: u32 = 2;

pub struct KDEAppMenuState {
    pub global: GlobalId,
}

pub trait KDEAppMenuHandler {
    /// Called when the new appmenu address is created
    fn new_appmenu(&mut self, surface: &WlSurface, service_name: String, object_path: String);
    /// Called when the appmenu is removed
    fn remove_appmenu(&mut self, surface: &WlSurface);
}

#[derive(Default, Debug)]
pub struct KDEAppMenuSurfaceState {
    pub service_name: String,
    pub object_path: String,
}

impl KDEAppMenuState {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<OrgKdeKwinAppmenuManager, ()>,
        D: Dispatch<OrgKdeKwinAppmenuManager, ()>,
        D: 'static,
    {
        let global = display.create_global::<D, OrgKdeKwinAppmenuManager, _>(APPMENU_VERSION, ());

        Self { global }
    }
}
impl<D> GlobalDispatch<OrgKdeKwinAppmenuManager, (), D> for KDEAppMenuState
where
    D: GlobalDispatch<OrgKdeKwinAppmenuManager, ()>,
    D: Dispatch<OrgKdeKwinAppmenuManager, ()>,
    D: Dispatch<OrgKdeKwinAppmenu, WlSurface>,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<OrgKdeKwinAppmenuManager>,
        _global_data: &(),
        data_init: &mut DataInit<'_, D>,
    ) {
        trace!(manager = ?resource);
        data_init.init(resource, ());
    }
}

impl<D> GlobalDispatch<OrgKdeKwinAppmenu, (), D> for KDEAppMenuState
where
    D: Dispatch<OrgKdeKwinAppmenu, ()>,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<OrgKdeKwinAppmenu>,
        _manager_state: &(),
        data_init: &mut DataInit<'_, D>,
    ) {
        trace!(menu = ?resource);
        data_init.init(resource, ());
    }
}

impl<D> Dispatch<OrgKdeKwinAppmenuManager, (), D> for KDEAppMenuState
where
    D: Dispatch<OrgKdeKwinAppmenuManager, ()>,
    D: Dispatch<OrgKdeKwinAppmenu, WlSurface>,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        _manager: &OrgKdeKwinAppmenuManager,
        request: ManagerRequest,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        trace!(?request);
        match request {
            ManagerRequest::Create { id, surface } => {
                data_init.init(id, surface);
            }
            ManagerRequest::Release => {
                // Auto-handled by smithay with Drop
            }
        }
    }
}

impl<D> Dispatch<OrgKdeKwinAppmenu, WlSurface, D> for KDEAppMenuState
where
    D: Dispatch<OrgKdeKwinAppmenu, WlSurface>,
    D: KDEAppMenuHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _appmenu: &OrgKdeKwinAppmenu,
        request: AppmenuRequest,
        surface: &WlSurface,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        trace!(?request);
        match request {
            AppmenuRequest::SetAddress {
                service_name,
                object_path,
            } => state.new_appmenu(surface, service_name, object_path),
            AppmenuRequest::Release => state.remove_appmenu(surface),
        }
    }
}

#[macro_export]
macro_rules! delegate_kde_appmenu {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::kde_appmenu::v2::server::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager: ()
        ] => $crate::protocols::kde_appmenu::KDEAppMenuState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::kde_appmenu::v2::server::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager: ()
        ] => $crate::protocols::kde_appmenu::KDEAppMenuState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            $crate::protocols::raw::kde_appmenu::v2::server::org_kde_kwin_appmenu::OrgKdeKwinAppmenu:
                smithay::reexports::wayland_server::protocol::wl_surface::WlSurface
        ] => $crate::protocols::kde_appmenu::KDEAppMenuState);
    };
}
