//! xtask for niri development tasks

mod golden_stepper;
mod golden_sync;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "niri development automation tasks", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync snapshot tests to golden test folders
    SyncGolden {
        /// Show what would be done without making changes
        #[arg(long)]
        dry_run: bool,
    },
    
    /// Launch niri with config for manual golden test verification
    GoldenStepper {
        /// The ops function to test, or a directory containing step files
        function_name: String,
        
        /// Use the RTL config variant (ignored when running a directory)
        #[arg(long)]
        rtl: bool,
        
        /// Run all step files in a directory (LTR then RTL for each)
        #[arg(long)]
        all: bool,
    },
    
    /// Generate .kdl config files from test_configs presets
    GenerateTestConfigs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = project_root();
    
    match cli.command {
        Commands::SyncGolden { dry_run } => {
            golden_sync::sync_golden(&root, dry_run)?;
        }
        Commands::GoldenStepper { function_name, rtl, all } => {
            golden_stepper::run(&root, &function_name, rtl, all)?;
        }
        Commands::GenerateTestConfigs => {
            generate_test_configs(&root)?;
        }
    }
    
    Ok(())
}

/// Get the project root directory
fn project_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).parent().unwrap().to_path_buf()
}

/// Generate test config .kdl files by running the ignored test
fn generate_test_configs(root: &std::path::Path) -> Result<()> {
    use std::process::Command;
    
    println!("Generating test config files...");
    
    let status = Command::new("cargo")
        .args(["test", "-p", "niri", "generate_test_configs", "--", "--ignored", "--nocapture"])
        .current_dir(root)
        .status()?;
    
    if !status.success() {
        anyhow::bail!("Failed to generate test configs");
    }
    
    Ok(())
}
