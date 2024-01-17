//! Types for communicating with niri via IPC.
#![warn(missing_docs)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Name of the environment variable containing the niri IPC socket path.
pub const SOCKET_PATH_ENV: &str = "NIRI_SOCKET";

/// Request from client to niri.
#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    /// Request information about connected outputs.
    Outputs,
}

/// Response from niri to client.
#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    /// Information about connected outputs.
    ///
    /// Map from connector name to output info.
    Outputs(HashMap<String, Output>),
}

/// Connected output.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Output {
    /// Name of the output.
    pub name: String,
    /// Textual description of the manufacturer.
    pub make: String,
    /// Textual description of the model.
    pub model: String,
    /// Physical width and height of the output in millimeters, if known.
    pub physical_size: Option<(u32, u32)>,
    /// Available modes for the output.
    pub modes: Vec<Mode>,
    /// Index of the current mode in [`Self::modes`].
    ///
    /// `None` if the output is disabled.
    pub current_mode: Option<usize>,
}

/// Output mode.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct Mode {
    /// Width in physical pixels.
    pub width: u16,
    /// Height in physical pixels.
    pub height: u16,
    /// Refresh rate in millihertz.
    pub refresh_rate: u32,
}
