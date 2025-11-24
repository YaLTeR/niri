//! Data types for the golden test stepper.

/// A single step in a visual test
#[derive(Debug)]
pub struct Step {
    pub action: String,
    pub description: String,
    pub key: Option<String>,
    pub ipc: Option<IpcAction>,
    pub expected: Option<String>,
}

/// An IPC action to send to niri
#[derive(Debug, Clone)]
pub struct IpcAction {
    pub action_type: String,
    pub args: toml::Table,
}

/// Parsed test configuration
#[derive(Debug)]
pub struct TestConfig {
    pub name: String,
    pub description: String,
    pub ltr_config: String,
    pub rtl_config: String,
    pub steps: Vec<Step>,
}
