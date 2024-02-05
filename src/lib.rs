#[macro_use]
extern crate tracing;

pub mod animation;
pub mod backend;
pub mod config_error_notification;
pub mod cursor;
#[cfg(feature = "dbus")]
pub mod dbus;
pub mod exit_confirm_dialog;
pub mod frame_clock;
pub mod handlers;
pub mod hotkey_overlay;
pub mod input;
pub mod ipc;
pub mod layout;
pub mod niri;
pub mod protocols;
pub mod render_helpers;
pub mod screenshot_ui;
pub mod utils;
pub mod watcher;

#[cfg(not(feature = "xdp-gnome-screencast"))]
pub mod dummy_pw_utils;
#[cfg(feature = "xdp-gnome-screencast")]
pub mod pw_utils;

#[cfg(not(feature = "xdp-gnome-screencast"))]
pub use dummy_pw_utils as pw_utils;

#[derive(clap::Subcommand)]
pub enum Msg {
    /// List connected outputs.
    Outputs,
}
