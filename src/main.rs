#[macro_use]
extern crate tracing;

mod animation;
mod backend;
mod dbus;
mod frame_clock;
mod handlers;
mod input;
mod layout;
mod niri;
mod utils;

use std::env;
use std::ffi::OsString;

use clap::Parser;
use niri::{Niri, State};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Command to run upon compositor startup.
    #[arg(last = true)]
    command: Vec<OsString>,
}

pub struct LoopData {
    display: Display<State>,
    state: State,
}

fn main() {
    env::set_var("RUST_BACKTRACE", "1");

    let directives = env::var("RUST_LOG").unwrap_or_else(|_| "niri=debug,info".to_owned());
    let env_filter = EnvFilter::builder().parse_lossy(directives);
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(env_filter)
        .init();

    let cli = Cli::parse();

    let _client = tracy_client::Client::start();

    let mut event_loop = EventLoop::try_new().unwrap();
    let mut display = Display::new().unwrap();
    let state = State::new(event_loop.handle(), event_loop.get_signal(), &mut display);
    let mut data = LoopData { display, state };

    if let Some((command, args)) = cli.command.split_first() {
        if let Err(err) = std::process::Command::new(command).args(args).spawn() {
            warn!("error spawning command: {err:?}");
        }
    }

    event_loop
        .run(None, &mut data, move |data| {
            let _span = tracy_client::span!("loop callback");

            // These should be called periodically, before flushing the clients.
            data.state.niri.monitor_set.refresh();
            data.state.niri.popups.cleanup();
            data.state.update_focus();

            {
                let _span = tracy_client::span!("flush_clients");
                data.display.flush_clients().unwrap();
            }
        })
        .unwrap();
}
