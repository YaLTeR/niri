//! Golden test stepper - step-by-step visual verification
//!
//! Launch niri and execute test steps one at a time.
//!
//! Usage:
//!   cargo xtask golden-stepper <function_name> [--rtl]
//!   cargo xtask golden-stepper <directory_path> --all
//!
//! Each test has a `steps/<test_name>.toml` file defining the sequence of actions.

mod ipc;
mod legacy;
mod notes;
mod parser;
mod runner;
mod stepper;
mod types;

use std::fs;
use std::path::Path;

use anyhow::Result;

use parser::find_steps_file;

/// Launch niri with config for manual golden test verification
pub fn run(root: &Path, fn_name: &str, rtl: bool, all: bool) -> Result<()> {
    let golden_dir = root.join("src/layout/tests/golden_tests");
    let manual_tests_dir = root.join("src/layout/tests/manual_tests");
    let config_dir = manual_tests_dir.join(".config");

    // Check if fn_name is an absolute directory path
    let path = Path::new(fn_name);
    if path.is_absolute() && path.is_dir() {
        return run_all_in_directory(root, path);
    }

    // Check if fn_name is a prefix like "000" that matches a module directory in manual_tests
    if let Some(module_dir) = find_module_by_prefix(&manual_tests_dir, fn_name)? {
        return run_all_in_directory(root, &module_dir);
    }

    // Try to find a steps file for this test in manual_tests
    let test_name = fn_name.trim_end_matches("_ops");
    if let Some((module_dir, steps_file)) = find_steps_file(&manual_tests_dir, test_name)? {
        return runner::run_with_steps(root, &module_dir, &steps_file, rtl);
    }

    // Fall back to legacy mode (no steps file)
    legacy::run_legacy(root, &golden_dir, &config_dir, fn_name, rtl)
}

/// Find a module directory by prefix (e.g., "000" -> "000_spawning_single")
fn find_module_by_prefix(golden_dir: &Path, prefix: &str) -> Result<Option<std::path::PathBuf>> {
    for entry in fs::read_dir(golden_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Match if directory starts with the prefix
                if name.starts_with(prefix) {
                    // Verify it has a steps directory
                    if path.join("steps").exists() {
                        return Ok(Some(path));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// Run all step files in a directory, LTR then RTL for each
fn run_all_in_directory(root: &Path, module_dir: &Path) -> Result<()> {
    let steps_dir = module_dir.join("steps");
    
    if !steps_dir.exists() {
        anyhow::bail!("Steps directory not found: {}", steps_dir.display());
    }

    // Collect all .toml files and sort alphabetically
    let mut step_files: Vec<_> = fs::read_dir(&steps_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "toml").unwrap_or(false))
        .collect();
    
    step_files.sort();

    if step_files.is_empty() {
        anyhow::bail!("No .toml step files found in: {}", steps_dir.display());
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  🎯 Golden Test Stepper - Batch Mode                         ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Directory: {:<49} ║", module_dir.file_name().unwrap().to_string_lossy());
    println!("║  Tests: {:<53} ║", step_files.len());
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // Show the execution order
    println!("Execution order:");
    for (i, file) in step_files.iter().enumerate() {
        let name = file.file_stem().unwrap().to_string_lossy();
        let idx = i * 2 + 1;
        println!("  {}. {} (LTR)", idx, name);
        println!("  {}. {} (RTL)", idx + 1, name);
    }
    println!();

    // Run each file: LTR then RTL
    for step_file in &step_files {
        let name = step_file.file_stem().unwrap().to_string_lossy();
        
        // Run LTR
        println!("\n{}", "=".repeat(66));
        println!("  Running: {} (LTR)", name);
        println!("{}\n", "=".repeat(66));
        runner::run_with_steps(root, module_dir, step_file, false)?;

        // Run RTL
        println!("\n{}", "=".repeat(66));
        println!("  Running: {} (RTL)", name);
        println!("{}\n", "=".repeat(66));
        runner::run_with_steps(root, module_dir, step_file, true)?;
    }

    println!("\n✅ All tests completed!");
    Ok(())
}
