use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use zbus::fdo::RequestNameFlags;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{self, OwnedValue, Type};
use zbus::{fdo, interface};

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

#[interface(name = "org.gnome.Mutter.DisplayConfig")]
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
        let mut monitors = Vec::new();
        let mut logical_monitors = Vec::new();

        for output in self.ipc_outputs.lock().unwrap().values() {
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
            if let Some(mode) = output.current_mode {
                modes[mode]
                    .properties
                    .insert(String::from("is-current"), OwnedValue::from(true));
            }

            let connector = c.clone();
            let model = output.model.clone();
            let make = output.make.clone();

            // Serial is used for session restore, so fall back to the connector name if it's
            // not available.
            let serial = output.serial.as_ref().unwrap_or(&connector).clone();

            let names = (connector, make, model, serial);

            if let Some(logical) = output.logical.as_ref() {
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

                logical_monitors.push(LogicalMonitor {
                    x: logical.x,
                    y: logical.y,
                    scale: logical.scale,
                    transform,
                    is_primary: false,
                    monitors: vec![names.clone()],
                    properties: HashMap::new(),
                });
            }

            monitors.push(Monitor {
                names,
                modes,
                properties,
            });
        }

        // Sort by connector.
        monitors.sort_unstable_by(|a, b| a.names.0.cmp(&b.names.0));
        logical_monitors.sort_unstable_by(|a, b| a.monitors[0].0.cmp(&b.monitors[0].0));

        let properties = HashMap::from([(String::from("layout-mode"), OwnedValue::from(1u32))]);
        Ok((0, monitors, logical_monitors, properties))
    }

    #[zbus(signal)]
    pub async fn monitors_changed(ctxt: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(property)]
    fn power_save_mode(&self) -> i32 {
        -1
    }

    #[zbus(property)]
    fn set_power_save_mode(&self, _mode: i32) -> zbus::Result<()> {
        Err(zbus::Error::Unsupported)
    }

    #[zbus(property)]
    fn panel_orientation_managed(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn apply_monitors_config_allowed(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn night_light_supported(&self) -> bool {
        false
    }
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
