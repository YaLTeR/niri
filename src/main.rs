#[macro_use]
extern crate tracing;

mod handlers;

mod animation;
mod backend;
mod dbus;
mod frame_clock;
mod input;
mod layout;
mod niri;
mod utils;

use std::env;
use std::ffi::OsString;

use backend::{Backend, Tty, Winit};
use clap::Parser;
use niri::Niri;
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
    niri: Niri,
    backend: Backend,

    // Last so that it's dropped after the Smithay state in Niri and related state in Tty.
    // Otherwise it will segfault on quit.
    display: Display<Niri>,
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

    let has_display = env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("DISPLAY").is_some();

    let backend = if has_display {
        Backend::Winit(Winit::new(event_loop.handle()))
    } else {
        Backend::Tty(Tty::new(event_loop.handle()))
    };

    let mut display = Display::new().unwrap();
    let niri = Niri::new(
        event_loop.handle(),
        event_loop.get_signal(),
        &mut display,
        backend.seat_name(),
    );

    let mut data = LoopData {
        niri,
        display,

        backend,
    };

    data.backend.init(&mut data.niri);

    if let Some((command, args)) = cli.command.split_first() {
        if let Err(err) = std::process::Command::new(command).args(args).spawn() {
            warn!("error spawning command: {err:?}");
        }
    }

    event_loop
        .run(None, &mut data, move |data| {
            let _span = tracy_client::span!("loop callback");

            // These should be called periodically, before flushing the clients.
            data.niri.monitor_set.refresh();
            data.niri.popups.cleanup();
            data.niri.update_focus();

            {
                let _span = tracy_client::span!("flush_clients");
                data.display.flush_clients().unwrap();
            }
        })
        .unwrap();
}
