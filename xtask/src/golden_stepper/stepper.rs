//! Interactive stepper loop and UI.

use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::Result;

use crate::golden_stepper::ipc::{send_ipc_action, send_key_with_wtype};
use crate::golden_stepper::notes::save_note;
use crate::golden_stepper::types::{Step, TestConfig};

/// Run the interactive step-by-step execution
pub fn run_interactive_stepper(
    config: &TestConfig,
    has_wtype: bool,
    socket_path: Option<&Path>,
    notes_file: &Path,
    rtl: bool,
) -> Result<()> {
    let mut stdout = io::stdout();

    println!();
    println!("════════════════════════════════════════════════════════════════");
    println!("  Type notes/bugs and press Enter to save & continue");
    println!("  Or just press Enter to continue without notes");
    println!("  Type 'q' to quit");
    println!("════════════════════════════════════════════════════════════════");
    println!();

    run_stepper_loop(config, has_wtype, socket_path, notes_file, rtl, &mut stdout)
}

fn run_stepper_loop(
    config: &TestConfig,
    has_wtype: bool,
    socket_path: Option<&Path>,
    notes_file: &Path,
    rtl: bool,
    stdout: &mut io::Stdout,
) -> Result<()> {
    let steps = &config.steps;

    for (idx, step) in steps.iter().enumerate() {
        // Show step info
        print_step_info(idx, steps)?;

        // Execute the action FIRST
        if let Some(ipc) = &step.ipc {
            println!("   📡 Sending IPC: {}", ipc.action_type);
            stdout.flush()?;
            match send_ipc_action(ipc, socket_path) {
                Ok(()) => {
                    println!("   ✓ IPC action completed");
                }
                Err(e) => {
                    println!("   ⚠️  IPC error: {}", e);
                }
            }
            thread::sleep(Duration::from_millis(500));
        } else if let Some(key) = &step.key {
            if has_wtype && step.action != "verify" {
                println!("   ⌨️  Sending key: {}", key);
                stdout.flush()?;
                send_key_with_wtype(key)?;
                thread::sleep(Duration::from_millis(500));
            } else {
                println!("   👆 Press in niri: {}", key);
            }
        }

        // Skip prompt for exit action
        if step.action == "exit" {
            println!("   Exiting...");
            break;
        }

        // Now prompt for notes
        println!();
        println!("   📝 Verify the result above. Type notes or press Enter to continue:");
        print!("> ");
        stdout.flush()?;

        let note = read_input_line(stdout)?;

        if note.trim() == "q" || note.trim() == "quit" {
            println!("Quitting...");
            break;
        }

        if !note.trim().is_empty() {
            save_note(config, idx, &note, notes_file, rtl)?;
            println!("   ✓ Note saved");
        }
    }

    println!();
    println!("✅ All steps completed!");

    Ok(())
}

/// Read a line of input from the user
fn read_input_line(stdout: &mut io::Stdout) -> Result<String> {
    stdout.flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

fn print_step_info(idx: usize, steps: &[Step]) -> Result<()> {
    let step = &steps[idx];

    println!();
    println!(
        "┌─ Step {}/{} ─────────────────────────────────────────────────",
        idx + 1,
        steps.len()
    );
    println!("│ Action: {}", step.action);
    println!("│ {}", step.description);
    if let Some(ipc) = &step.ipc {
        println!("│ IPC: {}", ipc.action_type);
        for (k, v) in &ipc.args {
            println!("│   {}: {}", k, v);
        }
    }
    if let Some(key) = &step.key {
        println!("│ Key to send: {}", key);
    }
    if let Some(expected) = &step.expected {
        println!("│ Expected:");
        for line in expected.lines() {
            if !line.trim().is_empty() {
                println!("│   {}", line.trim());
            }
        }
    }
    println!("└──────────────────────────────────────────────────────────────");

    io::stdout().flush()?;
    Ok(())
}
