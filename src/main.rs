#[macro_use]
extern crate tracing;

mod animation;
mod backend;
mod config;
mod cursor;
mod dbus;
mod frame_clock;
mod handlers;
mod input;
mod layout;
mod niri;
mod utils;
mod watcher;

#[cfg(not(feature = "xdp-gnome-screencast"))]
mod dummy_pw_utils;
#[cfg(feature = "xdp-gnome-screencast")]
mod pw_utils;
use std::ffi::OsString;
use std::path::PathBuf;
use std::{env, mem};

use clap::Parser;
use config::Config;
#[cfg(not(feature = "xdp-gnome-screencast"))]
use dummy_pw_utils as pw_utils;
use miette::{Context, NarratableReportHandler};
use niri::{Niri, State};
use portable_atomic::Ordering;
use smithay::reexports::calloop::{self, EventLoop};
use smithay::reexports::wayland_server::Display;
use tracing_subscriber::EnvFilter;
use utils::spawn;
use watcher::Watcher;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to config file (default: `$XDG_CONFIG_HOME/niri/config.kdl`).
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Command to run upon compositor startup.
    #[arg(last = true)]
    command: Vec<OsString>,
}

fn main() {
    env::set_var("RUST_BACKTRACE", "1");
    if env::var_os("RUST_LIB_BACKTRACE").is_none() {
        env::set_var("RUST_LIB_BACKTRACE", "0");
    }

    let directives = env::var("RUST_LOG").unwrap_or_else(|_| "niri=debug,info".to_owned());
    let env_filter = EnvFilter::builder().parse_lossy(directives);
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(env_filter)
        .init();

    let cli = Cli::parse();

    let _client = tracy_client::Client::start();

    miette::set_hook(Box::new(|_| Box::new(NarratableReportHandler::new()))).unwrap();
    let (mut config, path) = match Config::load(cli.config).context("error loading config") {
        Ok((config, path)) => (config, Some(path)),
        Err(err) => {
            warn!("{err:?}");
            (Config::default(), None)
        }
    };
    animation::ANIMATION_SLOWDOWN.store(config.debug.animation_slowdown, Ordering::Relaxed);
    let spawn_at_startup = mem::take(&mut config.spawn_at_startup);

    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Display::new().unwrap();
    let mut state = State::new(
        config,
        event_loop.handle(),
        event_loop.get_signal(),
        display,
    );

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
    if let Some((command, args)) = cli.command.split_first() {
        spawn(command, args);
    }

    for elem in spawn_at_startup {
        if let Some((command, args)) = elem.command.split_first() {
            spawn(command, args);
        }
    }

    event_loop
        .run(None, &mut state, |state| state.refresh_and_flush_clients())
        .unwrap();
}
