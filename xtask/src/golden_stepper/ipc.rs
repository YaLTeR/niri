//! IPC communication with niri.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use niri_ipc::socket::Socket;
use niri_ipc::{Action, Request, Response};

use crate::golden_stepper::types::IpcAction;

/// Send an IPC action to niri
pub fn send_ipc_action(ipc: &IpcAction, socket_path: Option<&Path>) -> Result<()> {
    let action = parse_ipc_action(ipc)?;

    let socket_path = socket_path
        .context("No socket path provided - cannot send IPC to dev niri")?;
    
    let mut socket = Socket::connect_to(socket_path)
        .context(format!("Failed to connect to niri socket at {}", socket_path.display()))?;

    let reply = socket
        .send(Request::Action(action))
        .context("Failed to send IPC request")?;

    match reply {
        Ok(Response::Handled) => Ok(()),
        Ok(other) => anyhow::bail!("Unexpected response: {:?}", other),
        Err(msg) => anyhow::bail!("niri error: {}", msg),
    }
}

/// Parse an IpcAction into a niri_ipc::Action
fn parse_ipc_action(ipc: &IpcAction) -> Result<Action> {
    // Convert the IpcAction to JSON and then parse it as a niri_ipc::Action
    // This allows us to use the same format as the niri CLI
    let mut json_obj = serde_json::Map::new();
    json_obj.insert(
        ipc.action_type.clone(),
        toml_to_json_value(&toml::Value::Table(ipc.args.clone())),
    );

    let json_value = serde_json::Value::Object(json_obj);
    let action: Action = serde_json::from_value(json_value).context(format!(
        "Failed to parse IPC action '{}'. Check action name and arguments.",
        ipc.action_type
    ))?;

    Ok(action)
}

/// Convert a TOML value to a JSON value
fn toml_to_json_value(toml_val: &toml::Value) -> serde_json::Value {
    match toml_val {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_to_json_value).collect())
        }
        toml::Value::Table(table) => {
            let map: serde_json::Map<String, serde_json::Value> = table
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json_value(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
    }
}

/// Send a key using wtype
pub fn send_key_with_wtype(key: &str) -> Result<()> {
    // Convert our key format to wtype format
    // "Mod+T" -> "super t"
    // "Mod+Shift+E" -> "super shift e"
    let wtype_key = key
        .replace("Mod+", "super ")
        .replace("Shift+", "shift ")
        .replace("Ctrl+", "ctrl ")
        .replace("Alt+", "alt ")
        .to_lowercase();

    let parts: Vec<&str> = wtype_key.split_whitespace().collect();

    let mut cmd = Command::new("wtype");
    cmd.arg("-M"); // Press modifiers
    for part in &parts[..parts.len() - 1] {
        cmd.arg(part);
    }
    cmd.arg("-k"); // Press key
    cmd.arg(parts.last().unwrap());
    cmd.arg("-m"); // Release modifiers
    for part in &parts[..parts.len() - 1] {
        cmd.arg(part);
    }

    cmd.status().context("Failed to run wtype")?;
    Ok(())
}
