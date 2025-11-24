//! Legacy mode for golden tests without steps files.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use regex::Regex;

/// Legacy run mode (no steps file)
pub fn run_legacy(
    root: &Path,
    golden_dir: &Path,
    config_dir: &Path,
    fn_name: &str,
    rtl: bool,
) -> Result<()> {
    // Find the function in golden test files and extract the niri_config attribute
    let config_name = find_config_for_function(golden_dir, fn_name, rtl)?;

    let config_path = config_dir.join(format!("{}.kdl", config_name));

    if !config_path.exists() {
        anyhow::bail!(
            "Config file not found: {}\n\nAvailable configs in {}:\n{}",
            config_path.display(),
            config_dir.display(),
            list_available_configs(config_dir)?
        );
    }

    let mode = if rtl { "RTL" } else { "LTR" };
    println!("🚀 Launching niri for golden test verification");
    println!("   Function: {}", fn_name);
    println!("   Mode: {}", mode);
    println!("   Config: {}", config_path.display());
    println!();

    // Try to find and display the ops
    if let Some(ops) = extract_ops_from_function(golden_dir, fn_name)? {
        println!("📝 Test operations:");
        for line in ops.lines() {
            println!("   {}", line);
        }
        println!();
    }

    // Try to find and display the golden file
    let golden_file_name = fn_name.trim_end_matches("_ops");
    let golden_suffix = if rtl { "_rtl" } else { "" };
    if let Some(golden_content) = find_golden_file(golden_dir, golden_file_name, golden_suffix)? {
        println!("🎯 Expected result (golden file):");
        // Show just the key position info
        for line in golden_content.lines() {
            if line.contains("column[")
                || line.contains("tile[")
                || line.starts_with("active_column_x=")
                || line.starts_with("active_tile_viewport_x=")
            {
                println!("   {}", line);
            }
        }
        println!();
    }

    println!("📋 Test instructions:");
    println!("   1. Open terminal windows (Mod+T) to match the test operations");
    println!("   2. Use Mod+R to cycle preset widths");
    println!("   3. Verify window positions match the golden file expectations");
    println!("   4. Press Mod+Shift+E to exit");
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

    // Launch niri with the config
    println!("🖥️  Starting niri...\n");
    let status = Command::new("cargo")
        .args(["run", "--package", "niri", "--", "-c"])
        .arg(&config_path)
        .current_dir(root)
        .status()
        .context("Failed to run niri")?;

    if !status.success() {
        eprintln!("\n⚠️  niri exited with non-zero status");
    }

    Ok(())
}

/// Find the config name for a function by parsing the // @niri_config(...) comment
///
/// Format: // @niri_config("ltr_config", "rtl_config")
///         fn function_name() -> Vec<Op> { ... }
fn find_config_for_function(golden_dir: &Path, fn_name: &str, rtl: bool) -> Result<String> {
    // Pattern to match // @niri_config("ltr", "rtl") followed by fn name
    // Supports both comment format and attribute format (for future proc macro)
    let comment_pattern = format!(
        r#"//\s*@niri_config\("([^"]+)",\s*"([^"]+)"\)\s*\n\s*fn\s+{}\s*\("#,
        regex::escape(fn_name)
    );
    let attr_pattern = format!(
        r#"#\[niri_config\("([^"]+)",\s*"([^"]+)"\)\]\s*fn\s+{}\s*\("#,
        regex::escape(fn_name)
    );
    let comment_re = Regex::new(&comment_pattern)?;
    let attr_re = Regex::new(&attr_pattern)?;

    // Search through all mod.rs files in golden test directories
    for entry in fs::read_dir(golden_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let mod_file = path.join("mod.rs");
            if mod_file.exists() {
                let content = fs::read_to_string(&mod_file)?;

                // Try comment format first, then attribute format
                let caps = comment_re
                    .captures(&content)
                    .or_else(|| attr_re.captures(&content));

                if let Some(caps) = caps {
                    let ltr_config = &caps[1];
                    let rtl_config = &caps[2];

                    return Ok(if rtl {
                        rtl_config.to_string()
                    } else {
                        ltr_config.to_string()
                    });
                }
            }
        }
    }

    // If no attribute found, try to guess from function name
    eprintln!(
        "⚠️  No // @niri_config(...) comment found for function: {}",
        fn_name
    );
    eprintln!("   Using default config naming convention...");

    // Default: try "default-1-3" / "default-1-3-rtl" pattern
    if rtl {
        Ok("default-1-3-rtl".to_string())
    } else {
        Ok("default-1-3".to_string())
    }
}

/// List available config files
fn list_available_configs(config_dir: &Path) -> Result<String> {
    let mut configs = Vec::new();

    if config_dir.exists() {
        for entry in fs::read_dir(config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "kdl").unwrap_or(false) {
                if let Some(name) = path.file_stem() {
                    configs.push(format!("  - {}", name.to_string_lossy()));
                }
            }
        }
    }

    configs.sort();
    Ok(configs.join("\n"))
}

/// Extract ops from a function body
fn extract_ops_from_function(golden_dir: &Path, fn_name: &str) -> Result<Option<String>> {
    // Pattern to match function definition and its body
    let fn_pattern = format!(
        r"fn\s+{}\s*\(\)\s*->\s*Vec<Op>\s*\{{",
        regex::escape(fn_name)
    );
    let fn_re = Regex::new(&fn_pattern)?;

    // Search through all mod.rs files
    for entry in fs::read_dir(golden_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let mod_file = path.join("mod.rs");
            if mod_file.exists() {
                let content = fs::read_to_string(&mod_file)?;

                if let Some(mat) = fn_re.find(&content) {
                    // Find the function body
                    let start = mat.end();
                    let rest = &content[start..];

                    // Find matching closing brace
                    let mut depth = 1;
                    let mut end = 0;
                    for (i, c) in rest.chars().enumerate() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = i;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }

                    if end > 0 {
                        let body = &rest[..end];
                        // Clean up and format
                        let cleaned: Vec<&str> = body
                            .lines()
                            .map(|l| l.trim())
                            .filter(|l| {
                                !l.is_empty() && !l.starts_with("//") && *l != "vec![" && *l != "]"
                            })
                            .collect();
                        return Ok(Some(cleaned.join("\n")));
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Find a golden file by test name
fn find_golden_file(
    golden_dir: &Path,
    test_name: &str,
    suffix: &str,
) -> Result<Option<String>> {
    let file_name = format!("{}{}.txt", test_name, suffix);

    // Search through all golden subdirectories
    for entry in fs::read_dir(golden_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let golden_subdir = path.join("golden");
            if golden_subdir.exists() {
                let golden_file = golden_subdir.join(&file_name);
                if golden_file.exists() {
                    return Ok(Some(fs::read_to_string(golden_file)?));
                }
            }
        }
    }

    Ok(None)
}
