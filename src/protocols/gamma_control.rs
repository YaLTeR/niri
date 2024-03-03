use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use smithay::reexports::wayland_protocols_wlr;
use smithay::reexports::wayland_server::backend::{ClientId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use wayland_protocols_wlr::gamma_control::v1::server::{
    zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};
use zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1;
use zwlr_gamma_control_v1::ZwlrGammaControlV1;

const VERSION: u32 = 1;

pub struct GammaControlManagerState {
    gamma_controls: HashMap<WlOutput, ZwlrGammaControlV1>,
}

pub struct GammaControlManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
    can_view: bool,
}

pub trait GammaControlHandler {
    fn gamma_control_manager_state(&mut self) -> &mut GammaControlManagerState;
    fn set_gamma(
        &mut self,
        output: &WlOutput,
        ramp: Vec<u16>,
        gamma_size: u32,
    ) -> anyhow::Result<()>;
    fn get_gamma(&mut self, output: &WlOutput) -> Option<Vec<u16>>;
    fn destroy(&mut self, output_id: ObjectId);
    fn get_gamma_size(&mut self, output: &WlOutput) -> Option<u32>;
}

pub struct GammaControlState {
    gamma_size: Option<u32>,
    previous_gamma_ramp: Option<Vec<u16>>,
    output: WlOutput,
    failed: bool,
}

impl GammaControlManagerState {
    pub fn new<D, F>(display: &DisplayHandle, can_view: bool, filter: F) -> Self
    where
        D: GlobalDispatch<ZwlrGammaControlManagerV1, GammaControlManagerGlobalData>,
        D: Dispatch<ZwlrGammaControlManagerV1, ()>,
        D: Dispatch<ZwlrGammaControlV1, GammaControlState>,
        D: GammaControlHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = GammaControlManagerGlobalData {
            filter: Box::new(filter),
            can_view,
        };
        display.create_global::<D, ZwlrGammaControlManagerV1, _>(VERSION, global_data);

        Self {
            gamma_controls: HashMap::new(),
        }
    }
    pub fn destroy_gamma_control(&mut self, output_id: ObjectId) {
        self.gamma_controls.remove(&output_id);
    }
}

impl<D> GlobalDispatch<ZwlrGammaControlManagerV1, GammaControlManagerGlobalData, D>
    for GammaControlManagerState
where
    D: GlobalDispatch<ZwlrGammaControlManagerV1, GammaControlManagerGlobalData>,
    D: Dispatch<ZwlrGammaControlManagerV1, ()>,
    D: Dispatch<ZwlrGammaControlV1, GammaControlState>,
    D: GammaControlHandler,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        manager: New<ZwlrGammaControlManagerV1>,
        _manager_state: &GammaControlManagerGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(manager, ());
    }

    fn can_view(client: Client, global_data: &GammaControlManagerGlobalData) -> bool {
        global_data.can_view && (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ZwlrGammaControlManagerV1, (), D> for GammaControlManagerState
where
    D: Dispatch<ZwlrGammaControlManagerV1, ()>,
    D: Dispatch<ZwlrGammaControlV1, GammaControlState>,
    D: GammaControlHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &ZwlrGammaControlManagerV1,
        request: <ZwlrGammaControlManagerV1 as Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_gamma_control_manager_v1::Request::GetGammaControl { id, output } => {
                let gamma_size = state.get_gamma_size(&output);
                let previous_gamma_ramp = state.get_gamma(&output);

                if state
                    .gamma_control_manager_state()
                    .gamma_controls
                    .contains_key(&output)
                    || gamma_size.is_none()
                    || previous_gamma_ramp.is_none()
                {
                    data_init
                        .init(
                            id,
                            GammaControlState {
                                gamma_size: gamma_size.clone(),
                                previous_gamma_ramp: None,
                                output: output.clone(),
                                failed: true,
                            },
                        )
                        .failed();
                    return;
                }

                let zwlr_gamma_control = data_init.init(
                    id,
                    GammaControlState {
                        gamma_size: gamma_size.clone(),
                        previous_gamma_ramp,
                        output: output.clone(),
                        failed: false,
                    },
                );

                zwlr_gamma_control.gamma_size(gamma_size.unwrap());
                state
                    .gamma_control_manager_state()
                    .gamma_controls
                    .insert(output, zwlr_gamma_control);
            }
            zwlr_gamma_control_manager_v1::Request::Destroy => (),
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ZwlrGammaControlV1, GammaControlState, D> for GammaControlManagerState
where
    D: Dispatch<ZwlrGammaControlV1, GammaControlState>,
    D: GammaControlHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ZwlrGammaControlV1,
        request: <ZwlrGammaControlV1 as Resource>::Request,
        data: &GammaControlState,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_gamma_control_v1::Request::SetGamma { fd } => {
                if data.failed {
                    return;
                }
                debug!("setting gamma for output {:?}", data.output);
                let buf = &mut Vec::new();
                if File::from(fd).read_to_end(buf).is_err() {
                    warn!("failed to read gamma data for output {:?}", data.output);
                    resource.failed();
                    return;
                }

                let gamma = bytemuck::cast_slice(buf).to_vec();
                let gamma_size = data.gamma_size.unwrap();

                if let Err(err) = state.set_gamma(&data.output, gamma, gamma_size) {
                    warn!("error setting gamma: {err:?}");
                    resource.failed();
                    return;
                }
            }
            zwlr_gamma_control_v1::Request::Destroy => (),
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: ClientId,
        _resource: &ZwlrGammaControlV1,
        data: &GammaControlState,
    ) {
        if data.failed {
            return;
        }
        let ramp = data.previous_gamma_ramp.as_ref().unwrap();

        if let Err(err) = state.set_gamma(&data.output, ramp.to_vec(), data.gamma_size.unwrap()) {
            warn!("error resetting gamma: {err:?}");
        }

        state.destroy(data.output.id());
    }
}

#[macro_export]
macro_rules! delegate_gamma_control {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::gamma_control::v1::server::zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1: $crate::protocols::gamma_control::GammaControlManagerGlobalData
        ] => $crate::protocols::gamma_control::GammaControlManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::gamma_control::v1::server::zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1: ()
        ] => $crate::protocols::gamma_control::GammaControlManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::gamma_control::v1::server::zwlr_gamma_control_v1::ZwlrGammaControlV1:  $crate::protocols::gamma_control::GammaControlState
        ] => $crate::protocols::gamma_control::GammaControlManagerState);
    };
}
