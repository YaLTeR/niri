use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use smithay::output::Output;
use zbus::zvariant::{OwnedValue, Type};
use zbus::{dbus_interface, fdo};

pub struct DisplayConfig {
    connectors: Arc<Mutex<HashMap<String, Output>>>,
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
        let monitors: Vec<Monitor> = self
            .connectors
            .lock()
            .unwrap()
            .keys()
            .map(|c| Monitor {
                names: (c.clone(), String::new(), String::new(), String::new()),
                modes: vec![],
                properties: HashMap::new(),
            })
            .collect();

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
    pub fn new(connectors: Arc<Mutex<HashMap<String, Output>>>) -> Self {
        Self { connectors }
    }
}
