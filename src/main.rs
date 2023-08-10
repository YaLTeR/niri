#[macro_use]
extern crate tracing;

mod handlers;

mod backend;
mod grabs;
mod input;
mod niri;
mod tty;
mod winit;

use std::env;
use std::ffi::OsString;

use backend::Backend;
use clap::Parser;
use niri::Niri;
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use tracing_subscriber::EnvFilter;
use tty::Tty;
use winit::Winit;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(last = true)]
    command: Option<OsString>,
}

pub struct LoopData {
    niri: Niri,
    display_handle: DisplayHandle,

    // Last so that it's dropped after the Smithay state in Niri.
    display: Display<Niri>,

    tty: Option<Tty>,
    winit: Option<Winit>,
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

    let mut winit = None;
    let mut tty = None;
    let backend: &mut dyn Backend = if has_display {
        winit = Some(Winit::new(event_loop.handle()));
        winit.as_mut().unwrap()
    } else {
        tty = Some(Tty::new(event_loop.handle()));
        tty.as_mut().unwrap()
    };

    let mut display = Display::new().unwrap();
    let display_handle = display.handle();
    let niri = Niri::new(
        event_loop.handle(),
        event_loop.get_signal(),
        &mut display,
        backend.seat_name(),
    );

    let mut data = LoopData {
        niri,
        display_handle,
        display,

        tty,
        winit,
    };

    if let Some(tty) = data.tty.as_mut() {
        tty.init(&mut data.niri);
    }
    if let Some(winit) = data.winit.as_mut() {
        winit.init(&mut data.niri);
    }

    let res = if let Some(command) = &cli.command {
        std::process::Command::new(command).spawn()
    } else {
        std::process::Command::new("weston-terminal").spawn()
    };
    if let Err(err) = res {
        warn!("error spawning command: {err}");
    }

    event_loop
        .run(None, &mut data, move |data| {
            // niri is running.
            let _span = tracy_client::span!("flush_clients");
            data.display.flush_clients().unwrap();
        })
        .unwrap();
}
