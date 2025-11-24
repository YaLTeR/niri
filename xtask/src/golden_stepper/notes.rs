//! Note-taking functionality for golden tests.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Local;

use crate::golden_stepper::types::TestConfig;

/// Save a note to the notes file
pub fn save_note(
    config: &TestConfig,
    step_idx: usize,
    note: &str,
    notes_file: &Path,
    rtl: bool,
) -> Result<()> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let mode = if rtl { "RTL" } else { "LTR" };
    let step_info = if step_idx < config.steps.len() {
        format!(
            "Step {}: {}",
            step_idx + 1,
            config.steps[step_idx].description
        )
    } else {
        "Final notes".to_string()
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(notes_file)
        .context("Failed to open notes file")?;

    writeln!(
        file,
        "═══════════════════════════════════════════════════════════════════"
    )?;
    writeln!(file, "Timestamp: {}", timestamp)?;
    writeln!(file, "Test: {} ({})", config.name, mode)?;
    writeln!(file, "{}", step_info)?;
    if step_idx < config.steps.len() {
        let step = &config.steps[step_idx];
        if let Some(ipc) = &step.ipc {
            writeln!(file, "IPC Action: {}", ipc.action_type)?;
        }
        if let Some(key) = &step.key {
            writeln!(file, "Key: {}", key)?;
        }
        if let Some(expected) = &step.expected {
            writeln!(file, "Expected:")?;
            for line in expected.lines() {
                writeln!(file, "  {}", line)?;
            }
        }
    }
    writeln!(
        file,
        "───────────────────────────────────────────────────────────────────"
    )?;
    writeln!(file, "Note: {}", note.trim())?;
    writeln!(file)?;

    Ok(())
}
