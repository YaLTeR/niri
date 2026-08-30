use fixture::Fixture;

mod client;
mod fixture;
mod server;

mod animations;
mod floating;
mod fullscreen;
#[cfg(feature = "dbus")]
mod keyboard;
mod layer_shell;
mod remove_output;
mod transactions;
mod window_opening;
