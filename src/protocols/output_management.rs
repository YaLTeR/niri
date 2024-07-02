use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::niri::State;
use crate::utils::ipc_transform_to_smithay;
use niri_ipc::{LogicalOutput, Transform};
use smithay::output::WeakOutput;
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

pub type OutputsConf = HashMap<WeakOutput, niri_ipc::Output>;

#[derive(Debug)]
pub struct OutputConfigurationDataInner {
    serial: u32,
    new_out: OutputsConf,
}

pub type OutputConfigurationData = Mutex<OutputConfigurationDataInner>;

#[derive(Debug)]
struct ClientData {
    heads: HashMap<WeakOutput, (ZwlrOutputHeadV1, Vec<ZwlrOutputModeV1>)>,
    confs: HashSet<ZwlrOutputConfigurationV1>,
    manager: ZwlrOutputManagerV1,
}

pub struct OutputManagementManagerState {
    display: DisplayHandle,
    serial: u32,
    clients: HashMap<ClientId, ClientData>,
    current_out: OutputsConf,
}

pub struct OutputManagementManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

impl OutputManagementManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
        D: Dispatch<ZwlrOutputManagerV1, ()>,
        D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
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
            current_out: HashMap::new(),
        }
    }
}

impl<D> GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData, D>
    for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
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
        for (output, conf) in &g_state.current_out {
            send_new_head::<D>(display, client, &mut client_data, output.clone(), conf);
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
    D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
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
                let g_state = state.output_management_state();
                let new_out = g_state.current_out.clone();
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
                state
                    .output_management_state()
                    .clients
                    .remove(&client.id())
                    .map(|c| c.manager.finished());
            }
            _ => unreachable!(),
        }
    }
    fn destroyed(state: &mut D, client: ClientId, _resource: &ZwlrOutputManagerV1, _data: &()) {
        state.output_management_state().clients.remove(&client);
    }
}

impl<D> Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData, D>
    for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
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
        if outdated {
            debug!("OutputConfiguration: request from an outdated configuration");
        }
        match request {
            zwlr_output_configuration_v1::Request::EnableHead { id, head } => {
                let Some(output) = head.data::<WeakOutput>() else {
                    error!("EnableHead: Missing attached output");
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                };
                if outdated {
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                }
                let mut conf_data = conf_data.lock().unwrap();
                let Some(output_conf) = conf_data.new_out.get_mut(output) else {
                    debug!("EnableHead: output configuration missing requested new head",);
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                };
                if output_conf.current_mode.is_none() {
                    output_conf.current_mode =
                        output_conf.modes.iter().position(|a| a.is_preferred);
                }
                let Some(current_mode) = output_conf.current_mode else {
                    error!("EnableHead: missing output preferred mode");
                    let _fail = data_init.init(id, OutputConfigurationHeadState::Cancelled);
                    return;
                };
                let mode = output_conf.modes[current_mode];
                data_init.init(
                    id,
                    OutputConfigurationHeadState::Ok(output.clone(), conf.clone()),
                );
                let logical = output_conf
                    .logical
                    .or_else(|| state.get_logical(&output_conf.name))
                    .unwrap_or(LogicalOutput {
                        // FIXME: implement a default scale/position logic
                        x: 0,
                        y: 0,
                        scale: 1.,
                        transform: Transform::Normal,
                        width: mode.width as u32,
                        height: mode.height as u32,
                    });
                output_conf.logical = Some(logical);
            }
            zwlr_output_configuration_v1::Request::DisableHead { head } => {
                if outdated {
                    return;
                }
                if let Some(output) = head.data::<WeakOutput>() {
                    if let Some(conf) = conf_data.lock().unwrap().new_out.get_mut(output) {
                        conf.current_mode = None;
                        conf.logical = None;
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
                let conf_data = conf_data.lock().unwrap();
                let enabled_head = conf_data
                    .new_out
                    .iter()
                    .find(|(_, c)| c.current_mode.is_some() && c.logical.is_some())
                    .is_some();

                if !enabled_head {
                    conf.failed();
                    return;
                }
                let mut new_conf = Vec::with_capacity(conf_data.new_out.len());
                for (_, out) in conf_data.new_out.iter() {
                    new_conf.push(match (out.current_mode, out.logical) {
                        (Some(current_mode), Some(logical)) => niri_config::Output {
                            off: false,
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
                            variable_refresh_rate: out.vrr_enabled,
                        },
                        _ => niri_config::Output {
                            off: true,
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
                    conf.failed();
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
    Ok(WeakOutput, ZwlrOutputConfigurationV1),
}

impl<D> Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState, D>
    for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
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

impl<D> Dispatch<ZwlrOutputHeadV1, WeakOutput, D> for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        _output_head: &ZwlrOutputHeadV1,
        request: zwlr_output_head_v1::Request,
        _data: &WeakOutput,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_head_v1::Request::Release => {}
            _ => unreachable!(),
        }
    }
    fn destroyed(state: &mut D, client: ClientId, _resource: &ZwlrOutputHeadV1, data: &WeakOutput) {
        state
            .output_management_state()
            .clients
            .get_mut(&client)
            .map(|c| {
                c.heads.remove(data);
            });
    }
}

impl<D> Dispatch<ZwlrOutputModeV1, (), D> for OutputManagementManagerState
where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        _mode: &ZwlrOutputModeV1,
        request: zwlr_output_mode_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_output_mode_v1::Request::Release => {}
            _ => unreachable!(),
        }
    }
}
pub trait OutputManagementHandler {
    fn output_management_state(&mut self) -> &mut OutputManagementManagerState;
    fn get_logical(&self, name: &str) -> Option<LogicalOutput>;
    fn apply_new_conf(&mut self, conf: Vec<niri_config::Output>);
    fn get_current_outputs(&self) -> OutputsConf;
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
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_head_v1::ZwlrOutputHeadV1: smithay::output::WeakOutput
        ] => $crate::protocols::output_management::OutputManagementManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_mode_v1::ZwlrOutputModeV1: ()
        ] => $crate::protocols::output_management::OutputManagementManagerState);
        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_configuration_head_v1::ZwlrOutputConfigurationHeadV1: $crate::protocols::output_management::OutputConfigurationHeadState
        ] => $crate::protocols::output_management::OutputManagementManagerState);
    };
}

fn notify_removed_head(clients: &mut HashMap<ClientId, ClientData>, head: &WeakOutput) {
    for (_, data) in clients.iter_mut() {
        data.heads.remove(head).map(|(head, mods)| {
            mods.iter().for_each(|m| m.finished());
            head.finished();
        });
    }
}

fn notify_new_head(
    state: &mut OutputManagementManagerState,
    output: &WeakOutput,
    conf: &niri_ipc::Output,
) {
    let display = &state.display;
    let clients = &mut state.clients;
    for (_, data) in clients.iter_mut() {
        if let Some(client) = data.manager.client() {
            send_new_head::<State>(&display, &client, data, output.clone(), conf);
        }
    }
}

fn send_new_head<D>(
    display: &DisplayHandle,
    client: &Client,
    client_data: &mut ClientData,
    output: WeakOutput,
    conf: &niri_ipc::Output,
) where
    D: GlobalDispatch<ZwlrOutputManagerV1, OutputManagementManagerGlobalData>,
    D: Dispatch<ZwlrOutputManagerV1, ()>,
    D: Dispatch<ZwlrOutputConfigurationV1, OutputConfigurationData>,
    D: Dispatch<ZwlrOutputConfigurationHeadV1, OutputConfigurationHeadState>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: OutputManagementHandler,
    D: 'static,
    D: Dispatch<ZwlrOutputHeadV1, WeakOutput>,
    D: Dispatch<ZwlrOutputModeV1, ()>,
    D: 'static,
{
    let new_head = client
        .create_resource::<ZwlrOutputHeadV1, _, D>(
            display,
            client_data.manager.version(),
            output.clone(),
        )
        .unwrap();
    client_data.manager.head(&new_head);
    new_head.name(conf.name.clone());
    new_head.description(format!("{} - {} - {}", conf.make, conf.model, conf.name));
    if let Some((width, height)) = conf.physical_size {
        if let (Ok(a), Ok(b)) = (width.try_into(), height.try_into()) {
            new_head.physical_size(a, b);
        }
    }
    let mut new_modes = Vec::with_capacity(conf.modes.len());
    for (index, mode) in conf.modes.iter().enumerate() {
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
        if Some(index) == conf.current_mode {
            new_head.current_mode(&new_mode);
        }
        new_modes.push(new_mode);
    }
    if let Some(logical) = conf.logical {
        new_head.position(logical.x, logical.y);
        new_head.transform(ipc_transform_to_smithay(logical.transform).into());
        new_head.scale(logical.scale);
    }
    new_head.enabled(conf.current_mode.is_some() as i32);
    if new_head.version() >= zwlr_output_head_v1::EVT_MAKE_SINCE {
        new_head.make(conf.make.clone());
    }
    if new_head.version() >= zwlr_output_head_v1::EVT_MODEL_SINCE {
        new_head.model(conf.model.clone());
    }

    if new_head.version() >= zwlr_output_head_v1::EVT_ADAPTIVE_SYNC_SINCE {
        new_head.adaptive_sync(match conf.vrr_enabled {
            true => AdaptiveSyncState::Enabled,
            false => AdaptiveSyncState::Disabled,
        });
    }
    // new_head.serial_number(output.serial);
    client_data.heads.insert(output, (new_head, new_modes));
}

pub fn notify_changes(state: &mut impl OutputManagementHandler) {
    let mut changed = false; /* most likely to endup true */
    let new_out = state.get_current_outputs();
    let g_state = state.output_management_state();
    for (output, conf) in new_out.iter() {
        if let Some(old) = g_state.current_out.get(output) {
            if old.vrr_enabled != conf.vrr_enabled {
                changed = true;
                for (_, client) in &g_state.clients {
                    if let Some((head, _)) = client.heads.get(output) {
                        if head.version() >= zwlr_output_head_v1::EVT_ADAPTIVE_SYNC_SINCE {
                            head.adaptive_sync(match conf.vrr_enabled {
                                true => AdaptiveSyncState::Enabled,
                                false => AdaptiveSyncState::Disabled,
                            });
                        }
                    }
                }
            }
            match (old.current_mode, conf.current_mode) {
                (Some(old_index), Some(new_index)) => {
                    if old.modes != conf.modes {
                        error!("output's old modes dosnt match new modes");
                    } else if old_index != new_index {
                        changed = true;
                        for (_, client) in &g_state.clients {
                            if let Some((head, modes)) = client.heads.get(output) {
                                if let Some(new_mode) = modes.get(new_index) {
                                    head.current_mode(new_mode);
                                } else {
                                    error!("output new mode doesnt exist for the client's output");
                                }
                            }
                        }
                    }
                }
                (Some(_), None) => {
                    changed = true;
                    for (_, client) in &g_state.clients {
                        if let Some((head, _)) = client.heads.get(output) {
                            head.enabled(0);
                        }
                    }
                }
                (None, Some(new_index)) => {
                    if old.modes != conf.modes {
                        error!("output's old modes dosnt match new modes");
                    } else {
                        changed = true;
                        for (_, client) in &g_state.clients {
                            if let Some((head, modes)) = client.heads.get(output) {
                                head.enabled(1);
                                if let Some(mode) = modes.get(new_index) {
                                    head.current_mode(mode);
                                } else {
                                    error!("output new mode doesnt exist for the client's output");
                                }
                            }
                        }
                    }
                }
                (None, None) => {}
            }
            match (old.logical, conf.logical) {
                (Some(old_logical), Some(new_logical)) => {
                    if old_logical != new_logical {
                        changed = true;
                        for (_, client) in &g_state.clients {
                            if let Some((head, _)) = client.heads.get(output) {
                                if old_logical.x != new_logical.x || old_logical.y != new_logical.y
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
                    for (_, client) in &g_state.clients {
                        if let Some((head, _)) = client.heads.get(output) {
                            // head enable in the mode diff check
                            head.position(new_logical.x, new_logical.y);
                            head.transform(ipc_transform_to_smithay(new_logical.transform).into());
                            head.scale(new_logical.scale);
                        }
                    }
                }
                (Some(_), None) => {
                    // heads disabled in the mode diff check
                }
                (None, None) => {}
            }
        } else {
            changed = true;
            notify_new_head(g_state, output, conf);
        }
    }
    for (old, _) in g_state.current_out.iter() {
        if new_out.get(old).is_none() {
            changed = true;
            notify_removed_head(&mut g_state.clients, old);
        }
    }
    if changed {
        g_state.current_out = new_out;
        g_state.serial += 1;
        for (_, data) in g_state.clients.iter() {
            data.manager.done(g_state.serial);
            for conf in data.confs.iter() {
                conf.cancelled();
            }
        }
    }
}
