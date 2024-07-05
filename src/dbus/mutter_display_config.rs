use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use zbus::fdo::RequestNameFlags;
use zbus::zvariant::{self, OwnedValue, Type};
use zbus::{dbus_interface, fdo, SignalContext};

use super::Start;
use crate::backend::IpcOutputMap;

pub struct DisplayConfig {
    ipc_outputs: Arc<Mutex<IpcOutputMap>>,
}

#[derive(Serialize, Type)]
pub struct Monitor {
    names: (String, String, String, String),
    modes: Vec<Mode>,
    properties: HashMap<String, OwnedValue>,
}

#[derive(Serialize, Type)]
pub struct Mode {
    id: String,
    width: i32,
    height: i32,
    refresh_rate: f64,
    preferred_scale: f64,
    supported_scales: Vec<f64>,
    properties: HashMap<String, OwnedValue>,
}

#[derive(Serialize, Type)]
pub struct LogicalMonitor {
    x: i32,
    y: i32,
    scale: f64,
    transform: u32,
    is_primary: bool,
    monitors: Vec<(String, String, String, String)>,
    properties: HashMap<String, OwnedValue>,
}

#[dbus_interface(name = "org.gnome.Mutter.DisplayConfig")]
impl DisplayConfig {
    async fn get_current_state(
        &self,
    ) -> fdo::Result<(
        u32,
        Vec<Monitor>,
        Vec<LogicalMonitor>,
        HashMap<String, OwnedValue>,
    )> {
        // Construct the DBus response.
        let mut monitors: Vec<(Monitor, LogicalMonitor)> = self
            .ipc_outputs
            .lock()
            .unwrap()
            .values()
            // Take only enabled outputs.
            .filter(|output| output.current_mode.is_some() && output.logical.is_some())
            .map(|output| {
                // Loosely matches the check in Mutter.
                let c = &output.name;
                let is_laptop_panel = matches!(c.get(..4), Some("eDP-" | "LVDS" | "DSI-"));

                // FIXME: use proper serial when we have libdisplay-info.
                // A serial is required for correct session restore by xdp-gnome.
                let serial = c.clone();

                let mut properties = HashMap::new();
                if is_laptop_panel {
                    properties.insert(
                        String::from("display-name"),
                        OwnedValue::from(zvariant::Str::from_static("Built-in display")),
                    );
                }
                properties.insert(
                    String::from("is-builtin"),
                    OwnedValue::from(is_laptop_panel),
                );

                let mut modes: Vec<Mode> = output
                    .modes
                    .iter()
                    .map(|m| {
                        let niri_ipc::Mode {
                            width,
                            height,
                            refresh_rate,
                            is_preferred,
                        } = *m;
                        let refresh = refresh_rate as f64 / 1000.;

                        Mode {
                            id: format!("{width}x{height}@{refresh:.3}"),
                            width: i32::from(width),
                            height: i32::from(height),
                            refresh_rate: refresh,
                            preferred_scale: 1.,
                            supported_scales: vec![1., 2., 3.],
                            properties: HashMap::from([(
                                String::from("is-preferred"),
                                OwnedValue::from(is_preferred),
                            )]),
                        }
                    })
                    .collect();
                modes[output.current_mode.unwrap()]
                    .properties
                    .insert(String::from("is-current"), OwnedValue::from(true));

                let monitor = Monitor {
                    names: (c.clone(), String::new(), String::new(), serial),
                    modes,
                    properties,
                };

                let logical = output.logical.as_ref().unwrap();

                let transform = match logical.transform {
                    niri_ipc::Transform::Normal => 0,
                    niri_ipc::Transform::_90 => 1,
                    niri_ipc::Transform::_180 => 2,
                    niri_ipc::Transform::_270 => 3,
                    niri_ipc::Transform::Flipped => 4,
                    niri_ipc::Transform::Flipped90 => 5,
                    niri_ipc::Transform::Flipped180 => 6,
                    niri_ipc::Transform::Flipped270 => 7,
                };

                let logical_monitor = LogicalMonitor {
                    x: logical.x,
                    y: logical.y,
                    scale: logical.scale,
                    transform,
                    is_primary: false,
                    monitors: vec![monitor.names.clone()],
                    properties: HashMap::new(),
                };

                (monitor, logical_monitor)
            })
            .collect();

        // Sort the built-in monitor first, then by connector name.
        monitors.sort_unstable_by(|a, b| {
            let a_is_builtin = a.0.properties.contains_key("display-name");
            let b_is_builtin = b.0.properties.contains_key("display-name");
            a_is_builtin
                .cmp(&b_is_builtin)
                .reverse()
                .then_with(|| a.0.names.0.cmp(&b.0.names.0))
        });

        let (monitors, logical_monitors) = monitors.into_iter().unzip();
        let properties = HashMap::from([(String::from("layout-mode"), OwnedValue::from(1u32))]);
        Ok((0, monitors, logical_monitors, properties))
    }

    #[dbus_interface(signal)]
    pub async fn monitors_changed(ctxt: &SignalContext<'_>) -> zbus::Result<()>;
}

impl DisplayConfig {
    pub fn new(ipc_outputs: Arc<Mutex<IpcOutputMap>>) -> Self {
        Self { ipc_outputs }
    }
}

impl Start for DisplayConfig {
    fn start(self) -> anyhow::Result<zbus::blocking::Connection> {
        let conn = zbus::blocking::Connection::session()?;
        let flags = RequestNameFlags::AllowReplacement
            | RequestNameFlags::ReplaceExisting
            | RequestNameFlags::DoNotQueue;

        conn.object_server()
            .at("/org/gnome/Mutter/DisplayConfig", self)?;
        conn.request_name_with_flags("org.gnome.Mutter.DisplayConfig", flags)?;

        Ok(conn)
    }
}
