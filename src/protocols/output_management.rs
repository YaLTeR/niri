use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::backend::IpcOutputMap;
use crate::niri::State;
use crate::utils::ipc_transform_to_smithay;
use niri_ipc::{LogicalOutput, Transform};
use smithay::reexports::wayland_server::{
    protocol::wl_output::Transform as WlTransform, Client, DataInit, Dispatch, DisplayHandle,
    GlobalDispatch, New, Resource, WEnum,
};
use smithay::reexports::{
    wayland_protocols_wlr::output_management::v1::server::{
        zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
        zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
        zwlr_output_head_v1::{self, AdaptiveSyncState, ZwlrOutputHeadV1},
        zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
        zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
    },
    wayland_server::backend::ClientId,
};

const VERSION: u32 = 4;

/*
    <clientId<
        <confID , IpcOutputMap>
        <headId, String /* output name */>
    >>
*/

#[derive(Debug)]
pub struct OutputConfigurationDataInner {
    serial: u32,
    new_out: IpcOutputMap,
}

pub type OutputConfigurationData = Mutex<OutputConfigurationDataInner>;

#[derive(Debug)]
struct ClientData {
    heads: HashMap<String, (ZwlrOutputHeadV1, Vec<ZwlrOutputModeV1>)>,
    confs: HashSet<ZwlrOutputConfigurationV1>,
    manager: ZwlrOutputManagerV1,
}

pub struct OutputManagementManagerState {
    display: DisplayHandle,
    serial: u32,
    clients: HashMap<ClientId, ClientData>,
    current_out: IpcOutputMap,
}

pub struct OutputManagementManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

impl OutputManagementManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F, outputs: IpcOutputMap) -> Self
    where
        D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
        D: Dispatch<ZwlrOutputManagerV1, ()>,
        D: Dispatch<ZwlrOutputHeadV1, String>,
        D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
        D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
        D: Dispatch<ZwlrOutputModeV1, ()>,
        D: OutputManagementHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = OutputManagementManagerGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, ZwlrOutputManagerV1, _>(VERSION, global_data);

        Self {
            display: display.clone(),
            clients: HashMap::new(),
            serial: 0,
            current_out: outputs,
        }
    }
}

impl<D> GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData, D>
    for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn bind(
        state: &mut D,
        display: &DisplayHandle,
        client: &Client,
        manager: New<ZwlrOutputManagerV1>,
        _manager_state: &OutputManagementManagerGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        let manager = data_init.init(manager, ());
        let g_state = state.output_management_state();
        let mut client_data = ClientData {
            heads: HashMap::new(),
            confs: HashSet::new(),
            manager: manager.clone(),
        };
        for (_, head) in &g_state.current_out {
            send_new_head::<D>(display, client, &mut client_data, &head);
        }
        g_state.clients.insert(client.id(), client_data);
        manager.done(g_state.serial);
    }

    fn can_view(client: Client, global_data: &OutputManagementManagerGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ZwlrOutputManagerV1, (), D> for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        client: &Client,
        _manager: &ZwlrOutputManagerV1,
        request: zwlr_output_manager_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                let new_out = state.output_management_state().current_out.clone();
                let g_state = state.output_management_state();
                let conf = data_init.init(
                    id,
                    Mutex::new(OutputConfigurationDataInner { serial, new_out }),
                );
                if let Some(client_data) = g_state.clients.get_mut(&client.id()) {
                    if serial != g_state.serial {
                        conf.cancelled();
                    }
                    client_data.confs.insert(conf);
                } else {
                    error!("CreateConfiguration: missing client data");
                }
            }
            zwlr_output_manager_v1::Request::Stop => {
                let clients = &mut state.output_management_state().clients;
                clients.get(&client.id()).map(|c| c.manager.finished());
                clients.remove(&client.id());
            }
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData, D>
    for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        client: &Client,
        conf: &ZwlrOutputConfigurationV1,
        request: zwlr_output_configuration_v1::Request,
        conf_data: &OutputConfigurationData,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        let g_state = state.output_management_state();
        let outdated = conf_data.lock().unwrap().serial != g_state.serial;
        match request {
            zwlr_output_configuration_v1::Request::EnableHead { id, head } => {
                let Some(head_name) = head.data::<String>() else {
                    error!("EnableHead: Missing attached output head name");
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                };
                if outdated {
                    info!("EnableHead: request from an outdated configuration");
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                }
                let mut conf_data = conf_data.lock().unwrap();
                let Some(output) = conf_data.new_out.get_mut(head_name) else {
                    error!("EnableHead: output configuration missing requested new head",);
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                };
                if output.current_mode.is_none() {
                    output.current_mode = output.modes.iter().position(|a| a.is_preferred);
                }
                let Some(current_mode) = output.current_mode else {
                    error!("EnableHead: missing output prefered mode");
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                };
                let mode = output.modes[current_mode];
                data_init.init(
                    id,
                    OutputConfigurationHeadState::Ok(head_name.clone(), conf.clone()),
                );
                let logical = output
                    .logical
                    .or_else(|| state.get_logical(&head_name))
                    .unwrap_or(LogicalOutput {
                        x: 0,
                        y: 0,
                        scale: 1.,
                        transform: Transform::Normal,
                        width: mode.width as u32,
                        height: mode.height as u32,
                    });
                output.logical = Some(logical);
            }
            zwlr_output_configuration_v1::Request::DisableHead { head } => {
                if outdated {
                    return;
                }
                if let Some(head_name) = head.data::<String>() {
                    if let Some(output) = conf_data.lock().unwrap().new_out.get_mut(head_name) {
                        output.current_mode = None;
                        output.logical = None;
                    }
                } else {
                    error!("DisableHead: missing attached output head name");
                    return;
                }
            }
            zwlr_output_configuration_v1::Request::Apply => {
                if outdated {
                    conf.cancelled();
                    return;
                }
                let enabled_head = conf_data
                    .lock()
                    .unwrap()
                    .new_out
                    .iter()
                    .find(|(_, c)| c.current_mode.is_some() && c.logical.is_some())
                    .is_some();

                if !enabled_head {
                    conf.cancelled();
                    return;
                }
                let conf_data = conf_data.lock().unwrap();
                if conf_data.serial != g_state.serial {
                    conf.cancelled();
                    return;
                }
                let mut new_conf = Vec::with_capacity(conf_data.new_out.len());
                for (_, out) in conf_data.new_out.iter() {
                    new_conf.push(match (out.current_mode, out.logical) {
                        (Some(current_mode), Some(logical)) => niri_config::Output {
                            off: !(out.current_mode.is_some() && out.logical.is_some()),
                            name: out.name.clone(),
                            scale: Some(niri_config::FloatOrInt(logical.scale)),
                            transform: logical.transform,
                            position: Some(niri_config::Position {
                                x: logical.x,
                                y: logical.y,
                            }),
                            mode: Some(niri_ipc::ConfiguredMode {
                                width: logical.width as u16,
                                height: logical.height as u16,
                                refresh: Some(out.modes[current_mode].refresh_rate as f64 / 1000.),
                            }),
                            variable_refresh_rate: false,
                        },
                        _ => niri_config::Output {
                            off: !(out.current_mode.is_some() && out.logical.is_some()),
                            name: out.name.clone(),
                            scale: None,
                            transform: Transform::Normal,
                            position: None,
                            mode: None,
                            variable_refresh_rate: false,
                        },
                    });
                }
                drop(conf_data);
                state.apply_new_conf(new_conf);
                conf.succeeded();
            }
            zwlr_output_configuration_v1::Request::Test => {
                if outdated {
                    conf.cancelled();
                    return;
                }
                let enabled_head = conf_data
                    .lock()
                    .unwrap()
                    .new_out
                    .iter()
                    .find(|(_, c)| c.current_mode.is_some() && c.logical.is_some())
                    .is_some();

                if !enabled_head {
                    conf.cancelled();
                    return;
                }
                conf.succeeded()
            }
            zwlr_output_configuration_v1::Request::Destroy => {
                g_state
                    .clients
                    .get_mut(&client.id())
                    .map(|d| d.confs.remove(conf));
            }
            _ => unreachable!(),
        }
    }
}

pub enum OutputConfigurationHeadState {
    Cancelled,
    Ok(String, ZwlrOutputConfigurationV1),
}

impl<D> Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState, D>
    for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        client: &Client,
        _conf_head: &ZwlrOutputConfigurationHeadV1,
        request: zwlr_output_configuration_head_v1::Request,
        data: &OutputConfigurationHeadState,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        let g_state = state.output_management_state();
        let Some(client_data) = g_state.clients.get_mut(&client.id()) else {
            error!("ConfigurationHead: missing client data");
            return;
        };
        let OutputConfigurationHeadState::Ok(output_name, conf) = data else {
            warn!("ConfigurationHead: request sent to a cancelled head");
            return;
        };
        let Some(conf_data) = conf.data::<OutputConfigurationData>() else {
            error!("ConfigurationHead: missing conf data");
            return;
        };
        let mut conf_data = conf_data.lock().unwrap();
        if conf_data.serial != g_state.serial {
            warn!("ConfigurationHead: request sent to an outdated");
            return;
        }
        let Some(new_out) = conf_data.new_out.get_mut(output_name) else {
            error!("ConfigurationHead: missing matching head conf data");
            return;
        };
        match request {
            zwlr_output_configuration_head_v1::Request::SetMode { mode } => {
                let index = match client_data
                    .heads
                    .get(output_name)
                    .map(|(_, mods)| mods.iter().position(|m| m.id() == mode.id()))
                {
                    Some(Some(index)) => index,
                    _ => {
                        error!("SetMode: failed to find requested mode");
                        return;
                    }
                };
                if new_out.modes.len() <= index {
                    error!("SetMode: requested mode is out of range");
                    return;
                }
                new_out.current_mode = Some(index);
            }
            zwlr_output_configuration_head_v1::Request::SetCustomMode {
                width,
                height,
                refresh,
            } => {
                // TODO: Support custom mode
                let (width, height, refresh): (u16, u16, u32) =
                    match (width.try_into(), height.try_into(), refresh.try_into()) {
                        (Ok(width), Ok(height), Ok(refresh)) => (width, height, refresh),
                        _ => {
                            warn!("SetCustomMode: invalid input data");
                            return;
                        }
                    };
                if let Some(index) = new_out.modes.iter().position(|m| {
                    m.width == width && m.height == height && m.refresh_rate == refresh
                }) {
                    new_out.current_mode = Some(index)
                } else {
                    error!("SetCustomMode: no matching mode");
                    return;
                }
            }
            zwlr_output_configuration_head_v1::Request::SetPosition { x, y } => {
                match &mut new_out.logical {
                    Some(mut logical) => {
                        logical.x = x;
                        logical.y = y;
                    }
                    None => {
                        error!("SetPosition: head is disabled");
                        return;
                    }
                }
            }
            zwlr_output_configuration_head_v1::Request::SetTransform { transform } => {
                let Some(logical) = &mut new_out.logical else {
                    error!("SetTransform: head is disabled");
                    return;
                };
                logical.transform = match transform {
                    WEnum::Value(WlTransform::Normal) => Transform::Normal,
                    WEnum::Value(WlTransform::_90) => Transform::_90,
                    WEnum::Value(WlTransform::_180) => Transform::_180,
                    WEnum::Value(WlTransform::_270) => Transform::_270,
                    WEnum::Value(WlTransform::Flipped) => Transform::Flipped,
                    WEnum::Value(WlTransform::Flipped90) => Transform::Flipped90,
                    WEnum::Value(WlTransform::Flipped180) => Transform::Flipped180,
                    WEnum::Value(WlTransform::Flipped270) => Transform::Flipped270,
                    _ => {
                        error!("SetTransform: unknown requested transform");
                        return;
                    }
                }
            }
            zwlr_output_configuration_head_v1::Request::SetScale { scale } => {
                let Some(logical) = &mut new_out.logical else {
                    error!("SetScale: head is disabled");
                    return;
                };
                logical.scale = scale;
            }
            zwlr_output_configuration_head_v1::Request::SetAdaptiveSync { state } => match state {
                WEnum::Value(AdaptiveSyncState::Enabled) => new_out.vrr_enabled = true,
                WEnum::Value(AdaptiveSyncState::Disabled) => new_out.vrr_enabled = false,
                _ => {
                    error!("SetAdaptativeSync: Unknown requested adaptative sync");
                    return;
                }
            },
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ZwlrOutputHeadV1, String, D> for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        client: &Client,
        _output_head: &ZwlrOutputHeadV1,
        request: zwlr_output_head_v1::Request,
        data: &String,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_head_v1::Request::Release => {
                let g_state = state.output_management_state();
                let Some(client_data) = g_state.clients.get_mut(&client.id()) else {
                    error!("Release: missing client data");
                    return;
                };
                client_data.heads.remove(data).map(|(h, _)| h.finished());
            }
            _ => unreachable!(),
        }
    }
}

impl<D> Dispatch<ZwlrOutputModeV1, (), D> for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        mode: &ZwlrOutputModeV1,
        request: zwlr_output_mode_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_mode_v1::Request::Release => mode.finished(),
            _ => unreachable!(),
        }
    }
}
pub trait OutputManagementHandler {
    fn output_management_state(&mut self) -> &mut OutputManagementManagerState;
    fn get_logical(&self, name: &str) -> Option<LogicalOutput>;
    fn apply_new_conf(&mut self, conf: Vec<niri_config::Output>);
}

#[macro_export]
macro_rules! delegate_output_management{
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_manager_v1::ZwlrOutputManagerV1: $crate::protocols::output_management::OutputManagementManagerGlobalData
        ] => $crate::protocols::output_management::OutputManagementManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_manager_v1::ZwlrOutputManagerV1: ()
        ] => $crate::protocols::output_management::OutputManagementManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_configuration_v1::ZwlrOutputConfigurationV1: $crate::protocols::output_management::OutputConfigurationData
        ] => $crate::protocols::output_management::OutputManagementManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_head_v1::ZwlrOutputHeadV1: String
        ] => $crate::protocols::output_management::OutputManagementManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_mode_v1::ZwlrOutputModeV1: ()
        ] => $crate::protocols::output_management::OutputManagementManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_configuration_head_v1::ZwlrOutputConfigurationHeadV1: $crate::protocols::output_management::OutputConfigurationHeadState
        ] => $crate::protocols::output_management::OutputManagementManagerState);
    };
}

fn notify_removed_head(clients: &mut HashMap<ClientId, ClientData>, name: &str) {
    for (_, data) in clients.iter_mut() {
        data.heads.remove(name).map(|(head, mods)| {
            mods.iter().for_each(|m| m.finished());
            head.finished();
        });
    }
}

fn notify_new_head(state: &mut OutputManagementManagerState, output: &niri_ipc::Output) {
    let display = &state.display;
    let clients = &mut state.clients;
    for (_, data) in clients.iter_mut() {
        if let Some(client) = data.manager.client() {
            send_new_head::<State>(&display, &client, data, output);
        }
    }
}

fn send_new_head<D>(
    display: &DisplayHandle,
    client: &Client,
    client_data: &mut ClientData,
    output: &niri_ipc::Output,
) where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
    D: Dispatch<ZwlrOutputHeadV1, String>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: 'static,
{
    let new_head = client
        .create_resource::<ZwlrOutputHeadV1, _, D>(
            display,
            client_data.manager.version(),
            output.name.clone(),
        )
        .unwrap();
    client_data.manager.head(&new_head);
    new_head.name(output.name.clone());
    new_head.description(output.name.clone());
    if let Some((width, height)) = output.physical_size {
        if let (Ok(a), Ok(b)) = (width.try_into(), height.try_into()) {
            new_head.physical_size(a, b);
        }
    }
    let mut index = 0;
    let mut new_modes = Vec::with_capacity(output.modes.len());
    for mode in &output.modes {
        let new_mode = client
            .create_resource::<ZwlrOutputModeV1, _, D>(display, new_head.version(), ())
            .unwrap();
        new_head.mode(&new_mode);
        if let (Ok(width), Ok(height)) = (mode.width.try_into(), mode.height.try_into()) {
            new_mode.size(width, height);
        }
        if mode.is_preferred {
            new_mode.preferred();
        }
        if let Ok(refresh_rate) = mode.refresh_rate.try_into() {
            new_mode.refresh(refresh_rate);
        }
        if Some(index) == output.current_mode {
            new_head.current_mode(&new_mode);
        }
        new_modes.push(new_mode);
        index += 1;
    }
    if let Some(logical) = output.logical {
        new_head.position(logical.x, logical.y);
        new_head.transform(ipc_transform_to_smithay(logical.transform).into());
        new_head.scale(logical.scale);
    }
    new_head.enabled(output.current_mode.is_some() as i32);
    new_head.make(output.make.clone());
    new_head.model(output.model.clone());

    new_head.adaptive_sync(match output.vrr_enabled {
        true => AdaptiveSyncState::Enabled,
        false => AdaptiveSyncState::Disabled,
    });
    // new_head.serial_number(output.serial);
    client_data
        .heads
        .insert(output.name.clone(), (new_head, new_modes));
}

pub fn notify_changes(state: &mut OutputManagementManagerState, new_out: IpcOutputMap) {
    let mut changed = false; /* most likely to endup true */
    for (name, new) in new_out.iter() {
        if let Some(old) = state.current_out.get(name) {
            if old.vrr_enabled != new.vrr_enabled {
                changed = true;
                for (_, client) in &state.clients {
                    if let Some((head, _)) = client.heads.get(name) {
                        head.adaptive_sync(match new.vrr_enabled {
                            true => AdaptiveSyncState::Enabled,
                            false => AdaptiveSyncState::Disabled,
                        });
                    }
                }
            }
            match (old.current_mode, new.current_mode) {
                (Some(old_index), Some(new_index)) => {
                    if old.modes.len() <= old_index
                        || old.modes.len() <= new_index
                        || new.modes.len() <= new_index
                    {
                        error!("notify_changes: out of bound mode");
                    } else if old.modes[old_index] != new.modes[new_index] {
                        changed = true;
                        if old.modes[new_index] == new.modes[new_index] {
                            for (_, client) in &state.clients {
                                if let Some((head, modes)) = client.heads.get(name) {
                                    head.current_mode(&modes[new_index]);
                                }
                            }
                        } else {
                            // The mod has not been registerd with the head :/
                        }
                    }
                }
                (Some(_), None) => {
                    changed = true;
                    for (_, client) in &state.clients {
                        if let Some((head, _)) = client.heads.get(name) {
                            head.enabled(0);
                        }
                    }
                }
                (None, Some(new_index)) => {
                    changed = true;
                    for (_, client) in &state.clients {
                        if let Some((head, _)) = client.heads.get(name) {
                            head.enabled(1);
                            if old.modes.len() <= new_index || new.modes.len() <= new_index {
                                error!("notify_changes: out of bound mode");
                            } else if old.modes[new_index] == new.modes[new_index] {
                                for (_, client) in &state.clients {
                                    if let Some((head, modes)) = client.heads.get(name) {
                                        head.current_mode(&modes[new_index]);
                                    }
                                }
                            } else {
                                // The mod has not been registerd with the head :/
                            }
                        }
                    }
                }
                (None, None) => {}
            }
            match (old.logical, new.logical) {
                (Some(old_logical), Some(new_logical)) => {
                    if old_logical != new_logical {
                        changed = true;
                        for (_, client) in &state.clients {
                            if let Some((head, _)) = client.heads.get(name) {
                                if old_logical.x != new_logical.x || old_logical.y != old_logical.x
                                {
                                    head.position(new_logical.x, new_logical.y);
                                }
                                if old_logical.scale != new_logical.scale {
                                    head.scale(new_logical.scale);
                                }
                                if old_logical.transform != new_logical.transform {
                                    head.transform(
                                        ipc_transform_to_smithay(new_logical.transform).into(),
                                    );
                                }
                            }
                        }
                    }
                }
                (None, Some(new_logical)) => {
                    changed = true;
                    for (_, client) in &state.clients {
                        if let Some((head, _)) = client.heads.get(name) {
                            head.enabled(0);
                            head.position(new_logical.x, new_logical.y);
                            head.transform(ipc_transform_to_smithay(new_logical.transform).into());
                            head.scale(new_logical.scale);
                        }
                    }
                }
                (Some(_), None) => {
                    changed = true;
                    for (_, client) in &state.clients {
                        if let Some((head, _)) = client.heads.get(name) {
                            head.enabled(0);
                        }
                    }
                }
                (None, None) => {}
            }
        } else {
            changed = true;
            notify_new_head(state, new);
        }
    }
    for (old, _) in state.current_out.iter() {
        if new_out.get(old).is_none() {
            changed = true;
            notify_removed_head(&mut state.clients, old);
        }
    }
    if changed {
        state.current_out = new_out;
        state.serial += 1;
        for (_, data) in state.clients.iter() {
            data.manager.done(state.serial);
            for conf in data.confs.iter() {
                conf.cancelled();
            }
        }
    }
}
