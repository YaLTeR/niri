//! TOML parsing and file discovery for golden tests.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::golden_stepper::types::{IpcAction, Step, TestConfig};

/// Find a steps file for a test
pub fn find_steps_file(golden_dir: &Path, test_name: &str) -> Result<Option<(PathBuf, PathBuf)>> {
    for entry in fs::read_dir(golden_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let steps_dir = path.join("steps");
            if steps_dir.exists() {
                let steps_file = steps_dir.join(format!("{}.toml", test_name));
                if steps_file.exists() {
                    return Ok(Some((path, steps_file)));
                }
            }
        }
    }
    Ok(None)
}

/// Parse a steps TOML file
pub fn parse_steps_file(path: &Path) -> Result<TestConfig> {
    let content = fs::read_to_string(path)?;
    let value: toml::Value = content.parse().context("Failed to parse TOML")?;

    let test = value.get("test").context("Missing [test] section")?;
    let config = value.get("config").context("Missing [config] section")?;
    let steps_array = value.get("steps").context("Missing [[steps]] section")?;

    let name = test
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let description = test
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let ltr_config = config
        .get("ltr")
        .and_then(|v| v.as_str())
        .context("Missing config.ltr")?
        .to_string();

    let rtl_config = config
        .get("rtl")
        .and_then(|v| v.as_str())
        .context("Missing config.rtl")?
        .to_string();

    let mut steps = Vec::new();
    if let Some(arr) = steps_array.as_array() {
        for step_value in arr {
            let action = step_value
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let step_description = step_value
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let key = step_value
                .get("key")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Parse IPC action if present
            let ipc = step_value.get("ipc").and_then(|v| {
                let table = v.as_table()?;
                let action_type = table.get("action")?.as_str()?.to_string();
                let mut args = toml::Table::new();
                for (k, v) in table.iter() {
                    if k != "action" {
                        args.insert(k.clone(), v.clone());
                    }
                }
                Some(IpcAction { action_type, args })
            });

            let expected = step_value
                .get("expected")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            steps.push(Step {
                action,
                description: step_description,
                key,
                ipc,
                expected,
            });
        }
    }

    Ok(TestConfig {
        name,
        description,
        ltr_config,
        rtl_config,
        steps,
    })
}
