use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use zbus::fdo::RequestNameFlags;
use zbus::zvariant::{self, OwnedValue, Type};
use zbus::{dbus_interface, fdo, SignalContext};

use super::Start;
use crate::backend::IpcOutputMap;
use crate::utils::is_laptop_panel;

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
                let is_laptop_panel = is_laptop_panel(c);
                let display_name = make_display_name(output, is_laptop_panel);

                let mut properties = HashMap::new();
                properties.insert(
                    String::from("display-name"),
                    OwnedValue::from(zvariant::Str::from(display_name)),
                );
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

                let connector = c.clone();
                let model = output.model.clone();
                let make = output.make.clone();

                // Serial is used for session restore, so fall back to the connector name if it's
                // not available.
                let serial = output.serial.as_ref().unwrap_or(&connector).clone();

                let monitor = Monitor {
                    names: (connector, make, model, serial),
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

        // Sort by connector.
        monitors.sort_unstable_by(|a, b| a.0.names.0.cmp(&b.0.names.0));

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

// Adapted from Mutter.
fn make_display_name(output: &niri_ipc::Output, is_laptop_panel: bool) -> String {
    if is_laptop_panel {
        return String::from("Built-in display");
    }

    let make = &output.make;
    let model = &output.model;
    if let Some(diagonal) = output.physical_size.map(|(width_mm, height_mm)| {
        let diagonal = f64::hypot(f64::from(width_mm), f64::from(height_mm)) / 25.4;
        format_diagonal(diagonal)
    }) {
        format!("{make} {diagonal}")
    } else if model != "Unknown" {
        format!("{make} {model}")
    } else {
        make.clone()
    }
}

fn format_diagonal(diagonal_inches: f64) -> String {
    let known = [12.1, 13.3, 15.6];
    if let Some(d) = known.iter().find(|d| (*d - diagonal_inches).abs() < 0.1) {
        format!("{d:.1}″")
    } else {
        format!("{}″", diagonal_inches.round() as u32)
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn test_format_diagonal() {
        assert_snapshot!(format_diagonal(12.11), @"12.1″");
        assert_snapshot!(format_diagonal(13.28), @"13.3″");
        assert_snapshot!(format_diagonal(15.6), @"15.6″");
        assert_snapshot!(format_diagonal(23.2), @"23″");
        assert_snapshot!(format_diagonal(24.8), @"25″");
    }
}
