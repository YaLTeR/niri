use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr;
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use wayland_protocols_wlr::virtual_pointer::v1::server::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};
use zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
use zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

const VERSION: u32 = 2;

pub struct VirtualPointerManagerState {
    virtual_pointers: Vec<ZwlrVirtualPointerV1>,
}

pub struct VirtualPointerManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

pub trait VirtualPointerHandler {
    fn virtual_pointer_manager_state(&mut self) -> &mut VirtualPointerManagerState;
}

pub struct VirtualPointerState {}

impl VirtualPointerManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<ZwlrVirtualPointerManagerV1, VirtualPointerManagerGlobalData>,
        D: Dispatch<ZwlrVirtualPointerManagerV1, ()>,
        D: Dispatch<ZwlrVirtualPointerV1, VirtualPointerState>,
        D: VirtualPointerHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = VirtualPointerManagerGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, ZwlrVirtualPointerManagerV1, _>(VERSION, global_data);

        Self {
            virtual_pointers: Vec::new(),
        }
    }
}

impl<D> GlobalDispatch<ZwlrVirtualPointerManagerV1, VirtualPointerManagerGlobalData, D>
    for VirtualPointerManagerState
where
    D: GlobalDispatch<ZwlrVirtualPointerManagerV1, VirtualPointerManagerGlobalData>,
    D: Dispatch<ZwlrVirtualPointerManagerV1, ()>,
    D: Dispatch<ZwlrVirtualPointerV1, VirtualPointerState>,
    D: VirtualPointerHandler,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        manager: New<ZwlrVirtualPointerManagerV1>,
        _manager_state: &VirtualPointerManagerGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(manager, ());
    }

    fn can_view(client: Client, global_data: &VirtualPointerManagerGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ZwlrVirtualPointerManagerV1, (), D> for VirtualPointerManagerState
where
    D: Dispatch<ZwlrVirtualPointerManagerV1, ()>,
    D: Dispatch<ZwlrVirtualPointerV1, VirtualPointerState>,
    D: VirtualPointerHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &ZwlrVirtualPointerManagerV1,
        request: <ZwlrVirtualPointerManagerV1 as Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointer { seat, id } => {
                let virtual_pointer = data_init.init(id, VirtualPointerState {});
                state.virtual_pointer_manager_state().virtual_pointers.push(virtual_pointer);
            }
            zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointerWithOutput {
                seat,
                output,
                id,
            } => {
                info!("CreateVirtualPointerWithOutput");
                let virtual_pointer = data_init.init(id, VirtualPointerState {});
                state.virtual_pointer_manager_state().virtual_pointers.push(virtual_pointer);
            }
            zwlr_virtual_pointer_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ZwlrVirtualPointerV1, VirtualPointerState, D> for VirtualPointerManagerState
where
    D: Dispatch<ZwlrVirtualPointerV1, VirtualPointerState>,
    D: VirtualPointerHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ZwlrVirtualPointerV1,
        request: <ZwlrVirtualPointerV1 as Resource>::Request,
        data: &VirtualPointerState,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_virtual_pointer_v1::Request::Motion { time, dx, dy } => {
                info!("Motion: {dx},{dy}");
            }
            zwlr_virtual_pointer_v1::Request::MotionAbsolute {
                time,
                x,
                y,
                x_extent,
                y_extent,
            } => {
                info!("MotionAbsolute: {x}({x_extent}),{y}({y_extent})");
            }
            zwlr_virtual_pointer_v1::Request::Button {
                time,
                button,
                state,
            } => {
                info!("Button: {button} / {state:?}");
            }
            zwlr_virtual_pointer_v1::Request::Axis { time, axis, value } => {
                info!("Axis: {axis:?} / {value}");
            }
            zwlr_virtual_pointer_v1::Request::Frame => {
                info!("Frame");
            }
            zwlr_virtual_pointer_v1::Request::AxisSource { axis_source } => {
                info!("AxisSource: {axis_source:?}");
            }
            zwlr_virtual_pointer_v1::Request::AxisStop { time, axis } => {
                info!("AxisStop: {axis:?}");
            }
            zwlr_virtual_pointer_v1::Request::AxisDiscrete {
                time,
                axis,
                value,
                discrete,
            } => {
                info!("AxisDiscrete: {axis:?} / {value} / {discrete}");
            }
            zwlr_virtual_pointer_v1::Request::Destroy => {
                info!("Destroy");
            }
            _ => unreachable!(),
        }
    }
}

#[macro_export]
macro_rules! delegate_virtual_pointer {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::virtual_pointer::v1::server::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1: $crate::protocols::virtual_pointer::VirtualPointerManagerGlobalData
        ] => $crate::protocols::virtual_pointer::VirtualPointerManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::virtual_pointer::v1::server::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1: ()
        ] => $crate::protocols::virtual_pointer::VirtualPointerManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::virtual_pointer::v1::server::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1:  $crate::protocols::virtual_pointer::VirtualPointerState
        ] => $crate::protocols::virtual_pointer::VirtualPointerManagerState);
    };
}
