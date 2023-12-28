#[macro_use]
extern crate tracing;

mod animation;
mod backend;
mod config;
mod cursor;
#[cfg(feature = "dbus")]
mod dbus;
mod frame_clock;
mod handlers;
mod input;
mod layout;
mod niri;
mod screenshot_ui;
mod utils;
mod watcher;

#[cfg(not(feature = "xdp-gnome-screencast"))]
mod dummy_pw_utils;
#[cfg(feature = "xdp-gnome-screencast")]
mod pw_utils;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::{env, mem};

use clap::{Parser, Subcommand};
use config::Config;
#[cfg(not(feature = "xdp-gnome-screencast"))]
use dummy_pw_utils as pw_utils;
use git_version::git_version;
use miette::{Context, NarratableReportHandler};
use niri::{Niri, State};
use portable_atomic::Ordering;
use sd_notify::NotifyState;
use smithay::reexports::calloop::{self, EventLoop};
use smithay::reexports::wayland_server::Display;
use tracing_subscriber::EnvFilter;
use utils::spawn;
use watcher::Watcher;

use crate::utils::{REMOVE_ENV_RUST_BACKTRACE, REMOVE_ENV_RUST_LIB_BACKTRACE};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(args_conflicts_with_subcommands = true)]
#[command(subcommand_value_name = "SUBCOMMAND")]
#[command(subcommand_help_heading = "Subcommands")]
struct Cli {
    /// Path to config file (default: `$XDG_CONFIG_HOME/niri/config.kdl`).
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Command to run upon compositor startup.
    #[arg(last = true)]
    command: Vec<OsString>,

    #[command(subcommand)]
    subcommand: Option<Sub>,
}

#[derive(Subcommand)]
enum Sub {
    /// Validate the config file.
    Validate {
        /// Path to config file (default: `$XDG_CONFIG_HOME/niri/config.kdl`).
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set backtrace defaults if not set.
    if env::var_os("RUST_BACKTRACE").is_none() {
        env::set_var("RUST_BACKTRACE", "1");
        REMOVE_ENV_RUST_BACKTRACE.store(true, Ordering::Relaxed);
    }
    if env::var_os("RUST_LIB_BACKTRACE").is_none() {
        env::set_var("RUST_LIB_BACKTRACE", "0");
        REMOVE_ENV_RUST_LIB_BACKTRACE.store(true, Ordering::Relaxed);
    }

    let is_systemd_service = env::var_os("NOTIFY_SOCKET").is_some();

    let directives = env::var("RUST_LOG").unwrap_or_else(|_| "niri=debug".to_owned());
    let env_filter = EnvFilter::builder().parse_lossy(directives);
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(env_filter)
        .init();

    if is_systemd_service {
        // If we're starting as a systemd service, assume that the intention is to start on a TTY.
        // Remove DISPLAY or WAYLAND_DISPLAY from our environment if they are set, since they will
        // cause the winit backend to be selected instead.
        if env::var_os("DISPLAY").is_some() {
            debug!("we're running as a systemd service but DISPLAY is set, removing it");
            env::remove_var("DISPLAY");
        }
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            debug!("we're running as a systemd service but WAYLAND_DISPLAY is set, removing it");
            env::remove_var("WAYLAND_DISPLAY");
        }
    }

    let cli = Cli::parse();

    let _client = tracy_client::Client::start();

    // Set a better error printer for config loading.
    miette::set_hook(Box::new(|_| Box::new(NarratableReportHandler::new()))).unwrap();

    // Handle subcommands.
    if let Some(subcommand) = cli.subcommand {
        match subcommand {
            Sub::Validate { config } => {
                Config::load(config).context("error loading config")?;
                info!("config is valid");
                return Ok(());
            }
        }
    }

    info!(
        "starting version {} ({})",
        env!("CARGO_PKG_VERSION"),
        git_version!(fallback = "unknown commit"),
    );

    // Load the config.
    let (mut config, path) = match Config::load(cli.config).context("error loading config") {
        Ok((config, path)) => (config, Some(path)),
        Err(err) => {
            warn!("{err:?}");
            (Config::default(), None)
        }
    };
    animation::ANIMATION_SLOWDOWN.store(config.debug.animation_slowdown, Ordering::Relaxed);
    let spawn_at_startup = mem::take(&mut config.spawn_at_startup);

    // Create the compositor.
    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Display::new().unwrap();
    let mut state = State::new(
        config,
        event_loop.handle(),
        event_loop.get_signal(),
        display,
    );

    // Set WAYLAND_DISPLAY for children.
    let socket_name = &state.niri.socket_name;
    env::set_var("WAYLAND_DISPLAY", socket_name);
    info!(
        "listening on Wayland socket: {}",
        socket_name.to_string_lossy()
    );

    if is_systemd_service {
        // We're starting as a systemd service. Export our variables.
        import_env_to_systemd();

        // Inhibit power key handling so we can suspend on it.
        #[cfg(feature = "dbus")]
        if !state.niri.config.borrow().input.disable_power_key_handling {
            if let Err(err) = state.niri.inhibit_power_key() {
                warn!("error inhibiting power key: {err:?}");
            }
        }
    }

    #[cfg(feature = "dbus")]
    dbus::DBusServers::start(&mut state, is_systemd_service);

    // Notify systemd we're ready.
    if let Err(err) = sd_notify::notify(true, &[NotifyState::Ready]) {
        warn!("error notifying systemd: {err:?}");
    };

    // Set up config file watcher.
    let _watcher = if let Some(path) = path {
        let (tx, rx) = calloop::channel::sync_channel(1);
        let watcher = Watcher::new(path.clone(), tx);
        event_loop
            .handle()
            .insert_source(rx, move |event, _, state| match event {
                calloop::channel::Event::Msg(()) => state.reload_config(path.clone()),
                calloop::channel::Event::Closed => (),
            })
            .unwrap();
        Some(watcher)
    } else {
        None
    };

    // Spawn commands from cli and auto-start.
    spawn(cli.command);

    for elem in spawn_at_startup {
        spawn(elem.command);
    }

    // Run the compositor.
    event_loop
        .run(None, &mut state, |state| state.refresh_and_flush_clients())
        .unwrap();

    Ok(())
}

fn import_env_to_systemd() {
    let rv = Command::new("/bin/sh")
        .args([
            "-c",
            "systemctl --user import-environment WAYLAND_DISPLAY && \
             hash dbus-update-activation-environment 2>/dev/null && \
             dbus-update-activation-environment WAYLAND_DISPLAY",
        ])
        .spawn();
    // Wait for the import process to complete, otherwise services will start too fast without
    // environment variables available.
    match rv {
        Ok(mut child) => match child.wait() {
            Ok(status) => {
                if !status.success() {
                    warn!("import environment shell exited with {status}");
                }
            }
            Err(err) => {
                warn!("error waiting for import environment shell: {err:?}");
            }
        },
        Err(err) => {
            warn!("error spawning shell to import environment into systemd: {err:?}");
        }
    }
}
