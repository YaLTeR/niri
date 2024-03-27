use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use smithay::utils::Transform;
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
            .iter()
            // Take only enabled outputs.
            .filter_map(|(c, (ipc, output))| {
                ipc.current_mode?;
                output.as_ref().map(move |output| (c, (ipc, output)))
            })
            .map(|(c, (ipc, output))| {
                // Loosely matches the check in Mutter.
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

                let mut modes: Vec<Mode> = ipc
                    .modes
                    .iter()
                    .map(|m| {
                        let niri_ipc::Mode {
                            width,
                            height,
                            refresh_rate,
                        } = *m;
                        let refresh = refresh_rate as f64 / 1000.;

                        Mode {
                            id: format!("{width}x{height}@{refresh:.3}"),
                            width: i32::from(width),
                            height: i32::from(height),
                            refresh_rate: refresh,
                            preferred_scale: 1.,
                            supported_scales: vec![1., 2., 3.],
                            properties: HashMap::new(),
                        }
                    })
                    .collect();
                modes[ipc.current_mode.unwrap()]
                    .properties
                    .insert(String::from("is-current"), OwnedValue::from(true));

                let monitor = Monitor {
                    names: (c.clone(), String::new(), String::new(), serial),
                    modes,
                    properties,
                };

                let loc = output.current_location();

                let transform = match output.current_transform() {
                    Transform::Normal => 0,
                    Transform::_90 => 1,
                    Transform::_180 => 2,
                    Transform::_270 => 3,
                    Transform::Flipped => 4,
                    Transform::Flipped90 => 5,
                    Transform::Flipped180 => 6,
                    Transform::Flipped270 => 7,
                };

                let logical_monitor = LogicalMonitor {
                    x: loc.x,
                    y: loc.y,
                    scale: output.current_scale().fractional_scale(),
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
