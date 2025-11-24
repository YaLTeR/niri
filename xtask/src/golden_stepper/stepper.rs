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
    let mut current_step = 0;

    // Show first step
    print_step_with_prompt(current_step, steps)?;

    loop {
        // Prompt for input (type note + Enter, or just Enter to continue)
        let note = read_input_line(stdout)?;

        // Save note if not empty
        if !note.trim().is_empty() {
            if note.trim() == "q" || note.trim() == "quit" {
                println!("Quitting...");
                break;
            }
            save_note(config, current_step, &note, notes_file, rtl)?;
            println!("✓ Note saved\n");
        }

        // Execute current step
        if current_step < steps.len() {
            let step = &steps[current_step];

            // Execute IPC action if present
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
                thread::sleep(Duration::from_millis(300));
            }
            // Execute key press if present (and no IPC action)
            else if let Some(key) = &step.key {
                if has_wtype && step.action != "verify" {
                    println!("   ⌨️  Sending key: {}", key);
                    stdout.flush()?;
                    send_key_with_wtype(key)?;
                    thread::sleep(Duration::from_millis(300));
                } else {
                    println!("   👆 Press in niri: {}", key);
                }
            }

            current_step += 1;

            if current_step >= steps.len() {
                println!();
                println!("✅ All steps completed!");
                println!("Type any final notes and press Enter (or just Enter to exit):");
                print!("> ");
                stdout.flush()?;

                let final_note = read_input_line(stdout)?;
                if !final_note.trim().is_empty() {
                    save_note(config, current_step - 1, &final_note, notes_file, rtl)?;
                    println!("✓ Final note saved");
                }
                break;
            } else {
                print_step_with_prompt(current_step, steps)?;
            }
        }
    }

    Ok(())
}

/// Read a line of input from the user
fn read_input_line(stdout: &mut io::Stdout) -> Result<String> {
    stdout.flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

fn print_step_with_prompt(idx: usize, steps: &[Step]) -> Result<()> {
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
    print!("> ");

    io::stdout().flush()?;
    Ok(())
}
