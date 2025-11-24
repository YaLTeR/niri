//! Sync snapshot tests to golden test folders
//!
//! This module handles:
//! 1. Running snapshot tests with `scrolling-original` feature to capture correct behavior
//! 2. Extracting snapshots from test files
//! 3. Writing LTR golden files with headers
//! 4. Generating RTL golden files by mirroring LTR positions
//! 5. Creating mod.rs stubs for new test modules
//!
//! # Generation Chain
//!
//! ```text
//! scrolling_original.rs
//!         │
//!         ▼ (cargo insta test)
//! snapshot_tests/*.rs
//!         │
//!         ▼ (extract_snapshots)
//! LTR golden files
//!         │
//!         ▼ (rtl_transform)
//! RTL golden files
//! ```

mod codegen;
mod rtl_transform;
mod snapshot_parser;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use codegen::{generate_mod_stub, update_golden_mod_rs};
use rtl_transform::{format_header, generate_rtl_snapshot, LTR_HEADER};
use snapshot_parser::{extract_module_name, extract_snapshots, find_snapshot_files};

/// Module info collected from snapshot tests
struct ModuleInfo {
    #[allow(dead_code)]
    name: String,
    snapshots: BTreeMap<String, String>,
}

/// Sync snapshot tests to golden test folders
pub fn sync_golden(root: &Path, dry_run: bool) -> Result<()> {
    let snapshot_dir = root.join("src/layout/tests/snapshot_tests");
    let golden_dir = root.join("src/layout/tests/golden_tests");
    
    println!("📁 Snapshot dir: {}", snapshot_dir.display());
    println!("📁 Golden dir: {}", golden_dir.display());
    println!();
    
    if dry_run {
        println!("🔍 DRY RUN - no changes will be made\n");
    }
    
    // Step 1: Run cargo insta test with scrolling-original feature
    run_snapshot_tests(root, dry_run)?;
    
    // Step 2: Find and parse snapshot test files
    let modules = collect_snapshots(&snapshot_dir)?;
    
    // Step 3: Create golden directories and files
    let new_modules = write_golden_files(&golden_dir, &modules, dry_run)?;
    
    // Step 4: Update golden_tests/mod.rs if there are new modules
    if !new_modules.is_empty() {
        println!("\n📝 Updating golden_tests/mod.rs...");
        update_golden_mod_rs(&golden_dir, &new_modules, dry_run)?;
    }
    
    // Step 5: Regenerate RTL files for all existing golden files
    println!("\n🔄 Regenerating RTL golden files...");
    regenerate_rtl_files(&golden_dir, dry_run)?;
    
    println!("\n✨ Done!");
    if dry_run {
        println!("   (This was a dry run - no changes were made)");
    }
    
    Ok(())
}

/// Run cargo insta test with scrolling-original feature to capture correct behavior
fn run_snapshot_tests(root: &Path, dry_run: bool) -> Result<()> {
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
            .current_dir(root)
            .status()
            .context("Failed to run cargo insta test")?;
        
        if !status.success() {
            println!("⚠️  cargo insta test had some failures, but continuing...");
        }
    }
    println!();
    
    Ok(())
}

/// Collect snapshots from all snapshot test files
fn collect_snapshots(snapshot_dir: &Path) -> Result<BTreeMap<String, ModuleInfo>> {
    let snapshot_files = find_snapshot_files(snapshot_dir)?;
    println!("📄 Found {} snapshot test files", snapshot_files.len());
    
    let mut modules: BTreeMap<String, ModuleInfo> = BTreeMap::new();
    
    for file in &snapshot_files {
        let file_name = file.file_stem().unwrap().to_str().unwrap();
        
        // Extract module name: "000_ltr_spawning_single" -> "000_spawning_single"
        let module_name = match extract_module_name(file_name) {
            Some(name) => name,
            None => {
                println!("⏭️  Skipping {} (doesn't match pattern)", file_name);
                continue;
            }
        };
        
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
    
    Ok(modules)
}

/// Write golden files (LTR and RTL) for all modules
fn write_golden_files(
    golden_dir: &Path,
    modules: &BTreeMap<String, ModuleInfo>,
    dry_run: bool,
) -> Result<Vec<String>> {
    println!("\n📝 Creating golden files...");
    
    let mut new_modules = Vec::new();
    
    for (module_name, module_info) in modules {
        let module_dir = golden_dir.join(module_name);
        let golden_subdir = module_dir.join("golden");
        
        // Check if this is a new module
        if !module_dir.exists() {
            new_modules.push(module_name.clone());
        }
        
        // Create directories
        if !dry_run {
            fs::create_dir_all(&golden_subdir)?;
        }
        println!("\n📂 {}/", module_name);
        
        // Write golden files (LTR and RTL)
        for (fn_name, snapshot) in &module_info.snapshots {
            // Write LTR golden file with header
            let ltr_content = format!("{}{}", format_header(LTR_HEADER), snapshot);
            let golden_file = golden_subdir.join(format!("{}.txt", fn_name));
            
            if dry_run {
                println!("   Would write: golden/{}.txt", fn_name);
            } else {
                fs::write(&golden_file, &ltr_content)?;
                println!("   ✅ golden/{}.txt", fn_name);
            }
            
            // Generate and write RTL golden file
            let rtl_snapshot = generate_rtl_snapshot(snapshot);
            let rtl_golden_file = golden_subdir.join(format!("{}_rtl.txt", fn_name));
            
            if dry_run {
                println!("   Would write: golden/{}_rtl.txt", fn_name);
            } else {
                fs::write(&rtl_golden_file, &rtl_snapshot)?;
                println!("   ✅ golden/{}_rtl.txt", fn_name);
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
    
    Ok(new_modules)
}

/// Regenerate RTL files for all existing LTR golden files
fn regenerate_rtl_files(golden_dir: &Path, dry_run: bool) -> Result<()> {
    for entry in fs::read_dir(golden_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            let golden_subdir = path.join("golden");
            if golden_subdir.exists() {
                for file_entry in fs::read_dir(&golden_subdir)? {
                    let file_entry = file_entry?;
                    let file_path = file_entry.path();
                    
                    if file_path.is_file() {
                        let file_name = file_path.file_name().unwrap().to_str().unwrap();
                        
                        // Skip if it's already an RTL file or not a .txt file
                        if file_name.ends_with("_rtl.txt") || !file_name.ends_with(".txt") {
                            continue;
                        }
                        
                        // Generate/regenerate RTL version
                        let rtl_name = file_name.replace(".txt", "_rtl.txt");
                        let rtl_path = golden_subdir.join(&rtl_name);
                        
                        // Always regenerate RTL files from LTR source
                        let ltr_content = fs::read_to_string(&file_path)?;
                        let rtl_content = generate_rtl_snapshot(&ltr_content);
                        
                        if dry_run {
                            println!("   Would regenerate: {}", rtl_name);
                        } else {
                            fs::write(&rtl_path, &rtl_content)?;
                            println!("   ✅ {}", rtl_name);
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}
