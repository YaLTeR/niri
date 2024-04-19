use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use niri_ipc::Action;

use crate::utils::version;

#[derive(Parser)]
#[command(author, version = version(), about, long_about = None)]
#[command(args_conflicts_with_subcommands = true)]
#[command(subcommand_value_name = "SUBCOMMAND")]
#[command(subcommand_help_heading = "Subcommands")]
pub struct Cli {
    /// Path to config file (default: `$XDG_CONFIG_HOME/niri/config.kdl`).
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    /// Import environment globally to systemd and D-Bus, run D-Bus services.
    ///
    /// Set this flag in a systemd service started by your display manager, or when running
    /// manually as your main compositor instance. Do not set when running as a nested window, or
    /// on a TTY as your non-main compositor instance, to avoid messing up the global environment.
    #[arg(long)]
    pub session: bool,
    /// Command to run upon compositor startup.
    #[arg(last = true)]
    pub command: Vec<OsString>,

    #[command(subcommand)]
    pub subcommand: Option<Sub>,
}

#[derive(Subcommand)]
pub enum Sub {
    /// Validate the config file.
    Validate {
        /// Path to config file (default: `$XDG_CONFIG_HOME/niri/config.kdl`).
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Communicate with the running niri instance.
    Msg {
        #[command(subcommand)]
        msg: Msg,
        /// Format output as JSON.
        #[arg(short, long)]
        json: bool,
    },
    /// Cause a panic to check if the backtraces are good.
    Panic,
}

#[derive(Subcommand)]
pub enum Msg {
    /// Print the version of the running niri instance.
    Version,
    /// List connected outputs.
    Outputs,
    /// Print information about the focused window.
    FocusedWindow,
    /// Perform an action.
    Action {
        #[command(subcommand)]
        action: Action,
    },
    /// Request an error from the running niri instance.
    RequestError,
}
