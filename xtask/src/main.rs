//! xtask for niri development tasks
//!
//! Usage:
//!   cargo xtask sync-golden    # Sync snapshot tests to golden tests
//!   cargo xtask sync-golden --dry-run  # Show what would be done without making changes

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use regex::Regex;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        print_help();
        return Ok(());
    }
    
    match args[1].as_str() {
        "sync-golden" => {
            let dry_run = args.iter().any(|a| a == "--dry-run");
            sync_golden(dry_run)?;
        }
        "help" | "--help" | "-h" => {
            print_help();
        }
        cmd => {
            eprintln!("Unknown command: {}", cmd);
            print_help();
            std::process::exit(1);
        }
    }
    
    Ok(())
}

fn print_help() {
    eprintln!("niri xtask - Development automation tasks");
    eprintln!();
    eprintln!("Usage: cargo xtask <command> [options]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  sync-golden       Sync snapshot tests to golden test folders");
    eprintln!("                    --dry-run  Show what would be done without making changes");
    eprintln!("  help              Show this help message");
}

/// Get the project root directory
fn project_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).parent().unwrap().to_path_buf()
}

/// Sync snapshot tests to golden test folders
fn sync_golden(dry_run: bool) -> Result<()> {
    let root = project_root();
    let snapshot_dir = root.join("src/layout/tests/snapshot_tests");
    let golden_dir = root.join("src/layout/tests/golden_tests");
    
    println!("📁 Snapshot dir: {}", snapshot_dir.display());
    println!("📁 Golden dir: {}", golden_dir.display());
    println!();
    
    if dry_run {
        println!("🔍 DRY RUN - no changes will be made\n");
    }
    
    // Step 1: Run cargo insta test with scrolling-original feature to capture known-good behavior
    println!("📸 Running cargo insta test with scrolling-original feature...");
    println!("   (This uses the original scrolling implementation to capture correct behavior)");
    if !dry_run {
        let status = Command::new("cargo")
            .args([
                "insta", "test", "--accept",
                "--package", "niri",
                "--lib",
                "--features", "scrolling-original",
                "--", "layout::tests::snapshot_tests"
            ])
            .current_dir(&root)
            .status()
            .context("Failed to run cargo insta test")?;
        
        if !status.success() {
            println!("⚠️  cargo insta test had some failures, but continuing...");
        }
    }
    println!();
    
    // Step 2: Find all snapshot test files
    let snapshot_files = find_snapshot_files(&snapshot_dir)?;
    println!("📄 Found {} snapshot test files", snapshot_files.len());
    
    // Step 3: Parse each file and extract snapshots
    let mut modules: BTreeMap<String, ModuleInfo> = BTreeMap::new();
    
    for file in &snapshot_files {
        let file_name = file.file_stem().unwrap().to_str().unwrap();
        
        // Extract module name: "000_ltr_spawning_single" -> "000_spawning_single"
        let module_name = extract_module_name(file_name);
        if module_name.is_none() {
            println!("⏭️  Skipping {} (doesn't match pattern)", file_name);
            continue;
        }
        let module_name = module_name.unwrap();
        
        println!("\n📖 Processing: {} -> {}", file_name, module_name);
        
        // Parse the file to extract snapshots
        let content = fs::read_to_string(file)?;
        let snapshots = extract_snapshots(&content)?;
        
        if snapshots.is_empty() {
            println!("   No snapshots found");
            continue;
        }
        
        println!("   Found {} snapshots", snapshots.len());
        
        let module = modules.entry(module_name.clone()).or_insert_with(|| ModuleInfo {
            name: module_name,
            snapshots: BTreeMap::new(),
        });
        
        for (fn_name, snapshot) in snapshots {
            println!("   - {}", fn_name);
            module.snapshots.insert(fn_name, snapshot);
        }
    }
    
    // Step 4: Create golden directories and files
    println!("\n📝 Creating golden files...");
    
    let mut new_modules = Vec::new();
    
    for (module_name, module_info) in &modules {
        let module_dir = golden_dir.join(module_name);
        let golden_subdir = module_dir.join("golden");
        
        // Check if this is a new module
        let is_new = !module_dir.exists();
        if is_new {
            new_modules.push(module_name.clone());
        }
        
        // Create directories
        if !dry_run {
            fs::create_dir_all(&golden_subdir)?;
        }
        println!("\n📂 {}/", module_name);
        
        // Write golden files
        for (fn_name, snapshot) in &module_info.snapshots {
            let golden_file = golden_subdir.join(format!("{}.txt", fn_name));
            if dry_run {
                println!("   Would write: golden/{}.txt", fn_name);
            } else {
                fs::write(&golden_file, snapshot)?;
                println!("   ✅ golden/{}.txt", fn_name);
            }
        }
        
        // Create mod.rs stub if it doesn't exist
        let mod_file = module_dir.join("mod.rs");
        if !mod_file.exists() {
            let stub = generate_mod_stub(module_name, &module_info.snapshots);
            if dry_run {
                println!("   Would create: mod.rs (stub)");
            } else {
                fs::write(&mod_file, stub)?;
                println!("   ✅ mod.rs (stub created)");
            }
        }
    }
    
    // Step 5: Update golden_tests/mod.rs if there are new modules
    if !new_modules.is_empty() {
        println!("\n📝 Updating golden_tests/mod.rs...");
        update_golden_mod_rs(&golden_dir, &new_modules, dry_run)?;
    }
    
    println!("\n✨ Done!");
    if dry_run {
        println!("   (This was a dry run - no changes were made)");
    }
    
    Ok(())
}

/// Find all snapshot test files
fn find_snapshot_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().map(|e| e == "rs").unwrap_or(false) {
            let name = path.file_stem().unwrap().to_str().unwrap();
            // Skip mod.rs
            if name != "mod" {
                files.push(path);
            }
        }
    }
    
    files.sort();
    Ok(files)
}

/// Extract module name from file name
/// "000_ltr_spawning_single" -> "000_spawning_single"
/// "010_ltr_spawning_multiple" -> "010_spawning_multiple"
fn extract_module_name(file_name: &str) -> Option<String> {
    // Pattern: NNN_ltr_rest or NNN_rtl_rest
    let re = Regex::new(r"^(\d+)_(ltr|rtl)_(.+)$").unwrap();
    
    if let Some(caps) = re.captures(file_name) {
        let num = &caps[1];
        let rest = &caps[3];
        Some(format!("{}_{}", num, rest))
    } else {
        None
    }
}

/// Extract snapshots from a Rust test file
/// Returns a map of snapshot_name -> snapshot_content
/// 
/// For functions with multiple snapshots, they are named:
/// - Single snapshot: `fn_name`
/// - Multiple snapshots: `fn_name_1`, `fn_name_2`, etc.
fn extract_snapshots(content: &str) -> Result<BTreeMap<String, String>> {
    let mut snapshots = BTreeMap::new();
    
    // Regex to find test functions - handles #[test] on separate line from fn
    let fn_re = Regex::new(r#"fn\s+(\w+)\s*\(\)"#).unwrap();
    
    // Find all test functions
    let mut saw_test_attr = false;
    let mut current_fn: Option<String> = None;
    let mut brace_depth = 0;
    let mut in_function = false;
    let mut function_content = String::new();
    
    for line in content.lines() {
        // Check for #[test] attribute
        if line.trim() == "#[test]" {
            saw_test_attr = true;
            continue;
        }
        
        // Check for function start after #[test]
        if saw_test_attr {
            if let Some(caps) = fn_re.captures(line) {
                current_fn = Some(caps[1].to_string());
                in_function = false;
                function_content.clear();
                brace_depth = 0;
            }
            saw_test_attr = false;
        }
        
        if current_fn.is_some() {
            function_content.push_str(line);
            function_content.push('\n');
            
            // Track braces
            for c in line.chars() {
                match c {
                    '{' => {
                        brace_depth += 1;
                        in_function = true;
                    }
                    '}' => {
                        brace_depth -= 1;
                        if in_function && brace_depth == 0 {
                            // End of function - extract all snapshots
                            if let Some(fn_name) = current_fn.take() {
                                let fn_snapshots = extract_all_snapshots_from_function(&function_content);
                                if fn_snapshots.len() == 1 {
                                    // Single snapshot - use function name directly
                                    snapshots.insert(fn_name, fn_snapshots.into_iter().next().unwrap());
                                } else if !fn_snapshots.is_empty() {
                                    // Multiple snapshots - number them
                                    for (i, snapshot) in fn_snapshots.into_iter().enumerate() {
                                        let name = format!("{}_{}", fn_name, i + 1);
                                        snapshots.insert(name, snapshot);
                                    }
                                }
                            }
                            in_function = false;
                            function_content.clear();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    
    Ok(snapshots)
}

/// Extract ALL snapshots from a function body
/// Returns a Vec of snapshot contents in order
fn extract_all_snapshots_from_function(content: &str) -> Vec<String> {
    let mut snapshots = Vec::new();
    let mut remaining = content;
    
    while let Some(snapshot) = extract_next_snapshot(remaining) {
        snapshots.push(snapshot.0);
        remaining = snapshot.1;
    }
    
    snapshots
}

/// Extract the next snapshot from content, returning (snapshot, remaining_content)
fn extract_next_snapshot(content: &str) -> Option<(String, &str)> {
    // Look for assert_snapshot! with @r" or @r#"
    // The snapshot is a multi-line raw string
    
    // Find the start of the snapshot
    let start_patterns = [("@r\"", "\""), ("@r#\"", "\"#")];
    let mut best_match: Option<(usize, usize, &str)> = None; // (start_idx, content_start, end_pattern)
    
    for (start_pat, end_pat) in &start_patterns {
        if let Some(idx) = content.find(start_pat) {
            let content_start = idx + start_pat.len();
            if best_match.is_none() || idx < best_match.unwrap().0 {
                best_match = Some((idx, content_start, end_pat));
            }
        }
    }
    
    let (_, content_start, end_pattern) = best_match?;
    
    // Find the end of the snapshot
    let rest = &content[content_start..];
    let end = rest.find(end_pattern)?;
    
    let snapshot_raw = &rest[..end];
    let remaining = &rest[end + end_pattern.len()..];
    
    // Clean up the snapshot - remove leading newline and trailing whitespace per line
    let lines: Vec<&str> = snapshot_raw.lines().collect();
    
    // Skip empty first line if present
    let lines: Vec<&str> = if lines.first().map(|l| l.is_empty()).unwrap_or(false) {
        lines[1..].to_vec()
    } else {
        lines
    };
    
    // Find minimum indentation (excluding empty lines)
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    
    // Remove common indentation
    let cleaned: Vec<String> = lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                l[min_indent..].to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    
    Some((cleaned.join("\n"), remaining))
}

struct ModuleInfo {
    #[allow(dead_code)]
    name: String,
    snapshots: BTreeMap<String, String>,
}

/// Generate a stub mod.rs for a new golden test module
fn generate_mod_stub(module_name: &str, snapshots: &BTreeMap<String, String>) -> String {
    let mut stub = String::new();
    
    stub.push_str(&format!("// Golden tests for {}\n", module_name));
    stub.push_str("//\n");
    stub.push_str("// Auto-generated stub. Customize as needed.\n\n");
    stub.push_str("use super::*;\n\n");
    
    // Generate test operations placeholder
    stub.push_str("// ============================================================================\n");
    stub.push_str("// Test Operations\n");
    stub.push_str("// ============================================================================\n\n");
    
    for fn_name in snapshots.keys() {
        stub.push_str(&format!("fn {}_ops() -> Vec<Op> {{\n", fn_name));
        stub.push_str("    // TODO: Define test operations\n");
        stub.push_str("    vec![]\n");
        stub.push_str("}\n\n");
    }
    
    // Generate LTR tests
    stub.push_str("// ============================================================================\n");
    stub.push_str("// LTR Tests\n");
    stub.push_str("// ============================================================================\n\n");
    
    for fn_name in snapshots.keys() {
        stub.push_str("#[test]\n");
        stub.push_str(&format!("fn {}() {{\n", fn_name));
        stub.push_str("    let mut layout = set_up_empty();\n");
        stub.push_str(&format!("    check_ops_on_layout(&mut layout, {}_ops());\n", fn_name));
        stub.push_str(&format!("    assert_golden!(layout.snapshot(), \"{}\");\n", fn_name));
        stub.push_str("}\n\n");
    }
    
    // Generate RTL tests (ignored)
    stub.push_str("// ============================================================================\n");
    stub.push_str("// RTL Tests\n");
    stub.push_str("// ============================================================================\n\n");
    
    stub.push_str("fn make_options_rtl() -> Options {\n");
    stub.push_str("    let mut options = make_options();\n");
    stub.push_str("    options.layout.right_to_left = true;\n");
    stub.push_str("    options\n");
    stub.push_str("}\n\n");
    
    stub.push_str("fn set_up_empty_rtl() -> Layout<TestWindow> {\n");
    stub.push_str("    let ops = [Op::AddOutput(1)];\n");
    stub.push_str("    check_ops_with_options(make_options_rtl(), ops)\n");
    stub.push_str("}\n\n");
    
    for fn_name in snapshots.keys() {
        stub.push_str("#[test]\n");
        stub.push_str("#[ignore = \"RTL scrolling not yet implemented\"]\n");
        stub.push_str(&format!("fn {}_rtl() {{\n", fn_name));
        stub.push_str("    let mut layout = set_up_empty_rtl();\n");
        stub.push_str(&format!("    check_ops_on_layout(&mut layout, {}_ops());\n", fn_name));
        stub.push_str(&format!("    assert_golden_rtl!(layout, \"{}\");\n", fn_name));
        stub.push_str("}\n");
    }
    
    stub
}

/// Update golden_tests/mod.rs to include new modules
fn update_golden_mod_rs(golden_dir: &Path, new_modules: &[String], dry_run: bool) -> Result<()> {
    let mod_file = golden_dir.join("mod.rs");
    let content = fs::read_to_string(&mod_file)?;
    
    // Find existing module declarations
    let existing_re = Regex::new(r#"#\[path = "(\d+_[^/]+)/mod\.rs"\]"#).unwrap();
    let existing: BTreeSet<String> = existing_re
        .captures_iter(&content)
        .map(|c| c[1].to_string())
        .collect();
    
    // Find modules to add
    let to_add: Vec<&String> = new_modules
        .iter()
        .filter(|m| !existing.contains(*m))
        .collect();
    
    if to_add.is_empty() {
        println!("   No new modules to add");
        return Ok(());
    }
    
    // Generate new module declarations
    let mut additions = String::new();
    for module in &to_add {
        // Convert module name to a valid Rust identifier
        let mod_ident = module.replace('-', "_");
        // Remove leading digits for the module name
        let mod_ident = mod_ident.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
        
        additions.push_str(&format!("\n#[path = \"{}/mod.rs\"]\n", module));
        additions.push_str(&format!("mod {};\n", mod_ident));
    }
    
    // Append to the file
    if dry_run {
        println!("   Would add to mod.rs:");
        for module in &to_add {
            println!("     - {}", module);
        }
    } else {
        let mut new_content = content;
        new_content.push_str(&additions);
        fs::write(&mod_file, new_content)?;
        println!("   ✅ Added {} new module(s)", to_add.len());
    }
    
    Ok(())
}
