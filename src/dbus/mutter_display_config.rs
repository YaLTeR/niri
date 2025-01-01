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
pub struct CrtcResource {
    id: u32,
    winsys_id: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    current_mode: i32,
    current_transform: u32,
    transforms: Vec<u32>,
    properties: HashMap<String, OwnedValue>,
}

#[derive(Serialize, Type)]
pub struct OutputResource {
    id: u32,
    winsys_id: i64,
    current_crtc: i32,
    possible_crtcs: Vec<u32>,
    name: String,
    modes: Vec<u32>,
    clones: Vec<u32>,
    properties: HashMap<String, OwnedValue>,
}

#[derive(Serialize, Type)]
pub struct ModeResource {
    id: u32,
    winsys_id: i64,
    width: u32,
    height: u32,
    frequency: f64,
    flags: u32,
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

// GetCurrentState
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
    async fn get_resources(
        &self,
    ) -> fdo::Result<(
        u32,
        Vec<CrtcResource>,
        Vec<OutputResource>,
        Vec<ModeResource>,
        i32,
        i32,
    )> {
        let mut crtcs: Vec<CrtcResource> = Vec::new();
        let mut outputs: Vec<OutputResource> = Vec::new();
        let mut modes: Vec<ModeResource> = Vec::new();

        for (id, output) in self.ipc_outputs.lock().unwrap().values().enumerate() {
            let c = &output.name;
            let is_laptop_panel = matches!(c.get(..4), Some("eDP-" | "LVDS" | "DSI-"));
            let display_name = make_display_name(output, is_laptop_panel);

            let modes = output
                .modes
                .iter()
                .map(|mode| {
                    let id = modes.len() as u32;
                    modes.push(ModeResource {
                        id,
                        winsys_id: id as i64,
                        width: mode.width as u32,
                        height: mode.height as u32,
                        frequency: mode.refresh_rate as f64 / 1000.0,
                        flags: 0,
                    });
                    id
                })
                .collect::<Vec<_>>();

            let mut properties = HashMap::new();
            properties.insert(
                String::from("vendor"),
                OwnedValue::from(zvariant::Str::from(output.make.clone())),
            );
            properties.insert(
                String::from("product"),
                OwnedValue::from(zvariant::Str::from(output.model.clone())),
            );
            if let Some(serial) = output.serial.as_ref() {
                properties.insert(
                    String::from("serial"),
                    OwnedValue::from(zvariant::Str::from(serial.clone())),
                );
            }
            if let Some((width, height)) = output.physical_size {
                properties.insert(String::from("width-mm"), OwnedValue::from(width as i32));
                properties.insert(String::from("height-mm"), OwnedValue::from(height as i32));
            }
            properties.insert(
                String::from("display-name"),
                OwnedValue::from(zvariant::Str::from(display_name)),
            );

            outputs.push(OutputResource {
                id: id as u32,
                winsys_id: id as i64,
                current_crtc: id as i32,
                possible_crtcs: vec![id as u32],
                name: output.name.clone(),
                modes,
                clones: vec![],
                properties,
            });

            // As we don't have access to actual CRTCs, simulate them
            if let Some(logical) = output.logical.as_ref() {
                crtcs.push(CrtcResource {
                    id: id as u32,
                    winsys_id: id as i64,
                    x: logical.x,
                    y: logical.y,
                    width: logical.width as i32,
                    height: logical.height as i32,
                    current_mode: output.current_mode.unwrap() as i32,
                    current_transform: logical.transform.to_wayland_id(),
                    transforms: (0..=8).collect(),
                    properties: HashMap::new(),
                });
            } else {
                crtcs.push(CrtcResource {
                    id: id as u32,
                    winsys_id: id as i64,
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                    current_mode: -1,
                    current_transform: 0,
                    transforms: (0..=8).collect(),
                    properties: HashMap::new(),
                });
            }
        }

        Ok((0, crtcs, outputs, modes, 65535, 65535))
    }

    #[dbus_interface(property)]
    fn power_save_mode(&self) -> i32 {
        -1
    }

    #[dbus_interface(property)]
    fn set_power_save_mode(&self, _mode: i32) -> zbus::Result<()> {
        Err(zbus::Error::Unsupported)
    }

    #[dbus_interface(property)]
    fn panel_orientation_managed(&self) -> bool {
        false
    }

    #[dbus_interface(property)]
    fn apply_monitors_config_allowed(&self) -> bool {
        true
    }

    #[dbus_interface(property)]
    fn night_light_supported(&self) -> bool {
        // TODO: actually add "whether gamma_length is non-zero" to
        // ipc_outputs and ensure at least one such output here
        true
    }

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
                logical_monitors.push(LogicalMonitor {
                    x: logical.x,
                    y: logical.y,
                    scale: logical.scale,
                    transform: logical.transform.to_wayland_id(),
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
