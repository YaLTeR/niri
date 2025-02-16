use std::collections::HashMap;

use smithay::{
    output::Output,
    reexports::{
        wayland_protocols_wlr::output_power_management::v1::server::{
            zwlr_output_power_manager_v1, zwlr_output_power_v1,
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
        },
    },
};
use zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1;
use zwlr_output_power_v1::ZwlrOutputPowerV1;

const VERSION: u32 = 1;

pub enum Mode {
    Off,
    On,
}

impl From<bool> for Mode {
    fn from(value: bool) -> Self {
        match value {
            false => Self::Off,
            true => Self::On,
        }
    }
}

impl From<zwlr_output_power_v1::Mode> for Mode {
    fn from(value: zwlr_output_power_v1::Mode) -> Self {
        match value {
            zwlr_output_power_v1::Mode::Off => Self::Off,
            zwlr_output_power_v1::Mode::On => Self::On,
            _ => unreachable!(),
        }
    }
}

impl From<Mode> for zwlr_output_power_v1::Mode {
    fn from(value: Mode) -> Self {
        match value {
            Mode::Off => Self::Off,
            Mode::On => Self::On,
        }
    }
}

pub struct OutputPowerManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

pub trait OutputPowerManagementHandler {
    fn output_power_manager_state(&mut self) -> &mut OutputPowerManagementManagerState;

    // todo: set output power per-monitor. Currently, niri is written to power
    // all monitors at the same time.
    fn get_output_power_mode(&mut self, output: &Output) -> Mode;
    fn set_output_power_mode(&mut self, output: &Output, mode: Mode);
}

pub struct OutputPowerManagementManagerState {
    // Active controls only. Failed ones are removed.
    output_powers: HashMap<Output, ZwlrOutputPowerV1>,
}

pub struct OutputPowerState {}

impl OutputPowerManagementManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<ZwlrOutputPowerManagerV1, OutputPowerManagerGlobalData>,
        D: Dispatch<ZwlrOutputPowerManagerV1, ()>,
        D: Dispatch<ZwlrOutputPowerV1, OutputPowerState>,
        D: OutputPowerManagementHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = OutputPowerManagerGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, ZwlrOutputPowerManagerV1, _>(VERSION, global_data);
        Self {
            output_powers: HashMap::new(),
        }
    }

    pub fn output_removed(&mut self, output: &Output) {
        if let Some(output_power) = self.output_powers.remove(output) {
            output_power.failed();
        }
    }

    pub fn output_power_mode_changed(&mut self, output: &Output, mode: Mode) {
        if let Some(output_power) = self.output_powers.get(output) {
            output_power.mode(mode.into());
        }
    }
}

impl<D> GlobalDispatch<ZwlrOutputPowerManagerV1, OutputPowerManagerGlobalData, D>
    for OutputPowerManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputPowerManagerV1, OutputPowerManagerGlobalData>,
    D: Dispatch<ZwlrOutputPowerManagerV1, ()>,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        manager: New<ZwlrOutputPowerManagerV1>,
        _global_data: &OutputPowerManagerGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(manager, ());
    }

    fn can_view(client: Client, global_data: &OutputPowerManagerGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ZwlrOutputPowerManagerV1, (), D> for OutputPowerManagementManagerState
where
    D: Dispatch<ZwlrOutputPowerManagerV1, ()>,
    D: Dispatch<ZwlrOutputPowerV1, OutputPowerState>,
    D: OutputPowerManagementHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        _resource: &ZwlrOutputPowerManagerV1,
        request: <ZwlrOutputPowerManagerV1 as Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_power_manager_v1::Request::GetOutputPower { id, output } => {
                if let Some(output) = Output::from_resource(&output) {
                    // We borrow state in the middle.
                    #[allow(clippy::map_entry)]
                    if !state
                        .output_power_manager_state()
                        .output_powers
                        .contains_key(&output)
                    {
                        let zwlr_output_power = data_init.init(id, OutputPowerState {});
                        zwlr_output_power.mode(state.get_output_power_mode(&output).into());
                        state
                            .output_power_manager_state()
                            .output_powers
                            .insert(output, zwlr_output_power);
                        return;
                    }
                }

                // Output not found,
                // or output power instance already exists
                data_init.init(id, OutputPowerState {}).failed();
            }
            zwlr_output_power_manager_v1::Request::Destroy => (),
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ZwlrOutputPowerV1, OutputPowerState, D> for OutputPowerManagementManagerState
where
    D: Dispatch<ZwlrOutputPowerV1, OutputPowerState>,
    D: OutputPowerManagementHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        resource: &ZwlrOutputPowerV1,
        request: <ZwlrOutputPowerV1 as Resource>::Request,
        _data: &OutputPowerState,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_power_v1::Request::SetMode { mode } => {
                let output_powers = &mut state.output_power_manager_state().output_powers;
                let Some((output, _)) = output_powers.iter().find(|(_, x)| *x == resource) else {
                    return;
                };
                let output = output.clone();

                // todo: handle invalid enum values
                if let Ok(mode) = mode.into_result() {
                    // note: if set_output_power_mode becomes fallible, this should
                    // fail the resource; see gamma_control for an example of how to
                    // implement that.
                    state.set_output_power_mode(&output, mode.into());
                }
            }
            zwlr_output_power_v1::Request::Destroy => (),
            _ => unreachable!(),
        }
    }

    fn destroyed(
        state: &mut D,
        _client: wayland_backend::server::ClientId,
        resource: &ZwlrOutputPowerV1,
        _data: &OutputPowerState,
    ) {
        let output_powers = &mut state.output_power_manager_state().output_powers;
        let Some((output, _)) = output_powers.iter().find(|(_, x)| *x == resource) else {
            return;
        };
        output_powers.remove(&output.clone());
    }
}

#[macro_export]
macro_rules! delegate_output_power_management {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_power_management::v1::server::zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1: $crate::protocols::output_power_management::OutputPowerManagerGlobalData
        ] => $crate::protocols::output_power_management::OutputPowerManagementManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_power_management::v1::server::zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1: ()
        ] => $crate::protocols::output_power_management::OutputPowerManagementManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_power_management::v1::server::zwlr_output_power_v1::ZwlrOutputPowerV1:  $crate::protocols::output_power_management::OutputPowerState
        ] => $crate::protocols::output_power_management::OutputPowerManagementManagerState);
    };
}
