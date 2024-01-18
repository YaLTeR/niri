use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use smithay::output::Output;
use zbus::fdo::RequestNameFlags;
use zbus::zvariant::{self, OwnedValue, Type};
use zbus::{dbus_interface, fdo};

use super::Start;

pub struct DisplayConfig {
    enabled_outputs: Arc<Mutex<HashMap<String, Output>>>,
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
        let mut monitors: Vec<Monitor> = self
            .enabled_outputs
            .lock()
            .unwrap()
            .keys()
            .map(|c| {
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

                Monitor {
                    names: (c.clone(), String::new(), String::new(), serial),
                    modes: vec![],
                    properties,
                }
            })
            .collect();

        // Sort the built-in monitor first, then by connector name.
        monitors.sort_unstable_by(|a, b| {
            let a_is_builtin = a.properties.contains_key("display-name");
            let b_is_builtin = b.properties.contains_key("display-name");
            a_is_builtin
                .cmp(&b_is_builtin)
                .reverse()
                .then_with(|| a.names.0.cmp(&b.names.0))
        });

        let logical_monitors = monitors
            .iter()
            .map(|m| LogicalMonitor {
                x: 0,
                y: 0,
                scale: 1.,
                transform: 0,
                is_primary: false,
                monitors: vec![m.names.clone()],
                properties: HashMap::new(),
            })
            .collect();

        Ok((0, monitors, logical_monitors, HashMap::new()))
    }

    // FIXME: monitors-changed signal.
}

impl DisplayConfig {
    pub fn new(enabled_outputs: Arc<Mutex<HashMap<String, Output>>>) -> Self {
        Self { enabled_outputs }
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
