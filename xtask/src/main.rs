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
        /// The ops function to test (e.g., spawn_single_column_one_third_ops)
        function_name: String,
        
        /// Use the RTL config variant
        #[arg(long)]
        rtl: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = project_root();
    
    match cli.command {
        Commands::SyncGolden { dry_run } => {
            golden_sync::sync_golden(&root, dry_run)?;
        }
        Commands::GoldenStepper { function_name, rtl } => {
            golden_stepper::run(&root, &function_name, rtl)?;
        }
    }
    
    Ok(())
}

/// Get the project root directory
fn project_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).parent().unwrap().to_path_buf()
}
