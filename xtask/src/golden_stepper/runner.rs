//! Main runner for golden test stepper with steps files.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::golden_stepper::parser::parse_steps_file;
use crate::golden_stepper::stepper::run_interactive_stepper;

/// Run with a steps file - interactive step-by-step execution
pub fn run_with_steps(root: &Path, module_dir: &Path, steps_file: &Path, rtl: bool) -> Result<()> {
    let config = parse_steps_file(steps_file)?;
    let config_dir = module_dir.parent().unwrap().join(".config");

    let config_name = if rtl {
        &config.rtl_config
    } else {
        &config.ltr_config
    };
    let config_path = config_dir.join(format!("{}.kdl", config_name));

    if !config_path.exists() {
        anyhow::bail!("Config file not found: {}", config_path.display());
    }

    let mode = if rtl { "RTL" } else { "LTR" };

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  🎯 Golden Test Stepper                                      ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Test: {:<54} ║", config.name);
    println!("║  Mode: {:<54} ║", mode);
    println!("║  Steps: {:<53} ║", config.steps.len());
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("📝 {}", config.description);
    println!();

    // Show all steps
    println!("Steps to execute:");
    for (i, step) in config.steps.iter().enumerate() {
        let key_info = step
            .key
            .as_ref()
            .map(|k| format!(" [{}]", k))
            .unwrap_or_default();
        println!("  {}. {}{}", i + 1, step.description, key_info);
    }
    println!();

    // Build niri first
    println!("🔨 Building niri...");
    let status = Command::new("cargo")
        .args(["build", "--package", "niri"])
        .current_dir(root)
        .status()
        .context("Failed to build niri")?;

    if !status.success() {
        anyhow::bail!("Failed to build niri");
    }

    // Check for wtype (Wayland key sender)
    let has_wtype = Command::new("which")
        .arg("wtype")
        .stdout(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !has_wtype {
        println!("⚠️  'wtype' not found. Install it for automatic key sending:");
        println!("   sudo pacman -S wtype  # Arch");
        println!("   sudo apt install wtype  # Debian/Ubuntu");
        println!();
        println!("Without wtype, you'll need to press the keys manually.");
        println!();
    }

    // Launch niri in background, capturing stderr to find socket path
    println!("🖥️  Starting niri...");
    let mut niri_process = Command::new("cargo")
        .args(["run", "--package", "niri", "--", "-c"])
        .arg(&config_path)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start niri")?;

    // Capture stderr and look for socket path
    let socket_path = Arc::new(Mutex::new(None::<PathBuf>));
    let socket_path_clone = Arc::clone(&socket_path);

    let stderr = niri_process.stderr.take().unwrap();
    let _stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                if line.contains("IPC listening on:") {
                    if let Some(path_str) = line.split("IPC listening on:").nth(1) {
                        let path = PathBuf::from(path_str.trim());
                        *socket_path_clone.lock().unwrap() = Some(path);
                    }
                }
            }
        }
    });

    // Wait for niri to start and socket to be available
    println!("   Waiting for niri socket...");
    let mut socket_found = false;
    for _ in 0..50 {
        thread::sleep(Duration::from_millis(100));
        if socket_path.lock().unwrap().is_some() {
            socket_found = true;
            break;
        }
    }

    let niri_socket = if socket_found {
        let path = socket_path.lock().unwrap().clone().unwrap();
        println!("   Found socket: {}", path.display());
        Some(path)
    } else {
        println!("   ⚠️  Could not find niri socket, IPC may not work");
        None
    };

    // Notes file path
    let notes_file = module_dir.join("notes.txt");

    // Run the interactive stepper
    let result = run_interactive_stepper(&config, has_wtype, niri_socket.as_deref(), &notes_file, rtl);

    // Clean up niri
    let _ = niri_process.kill();
    let _ = niri_process.wait();

    result
}
