#[macro_use]
extern crate tracing;

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::FromRawFd;
use std::path::PathBuf;
use std::process::Command;
use std::{env, mem};

use clap::Parser;
use directories::ProjectDirs;
use niri::animation;
use niri::cli::{Cli, Sub};
#[cfg(feature = "dbus")]
use niri::dbus;
use niri::ipc::client::handle_msg;
use niri::niri::State;
use niri::utils::spawning::{
    spawn, CHILD_ENV, REMOVE_ENV_RUST_BACKTRACE, REMOVE_ENV_RUST_LIB_BACKTRACE,
};
use niri::utils::watcher::Watcher;
use niri::utils::{cause_panic, version, IS_SYSTEMD_SERVICE};
use niri_config::Config;
use portable_atomic::Ordering;
use sd_notify::NotifyState;
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
use tracing_subscriber::EnvFilter;

const DEFAULT_LOG_FILTER: &str = "niri=debug,smithay::backend::renderer::gles=error";

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

    if env::var_os("NOTIFY_SOCKET").is_some() {
        IS_SYSTEMD_SERVICE.store(true, Ordering::Relaxed);

        #[cfg(not(feature = "systemd"))]
        warn!(
            "running as a systemd service, but systemd support is compiled out. \
             Are you sure you did not forget to set `--features systemd`?"
        );
    }

    let directives = env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_FILTER.to_owned());
    let env_filter = EnvFilter::builder().parse_lossy(directives);
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(env_filter)
        .init();

    let cli = Cli::parse();

    if cli.session {
        // If we're starting as a session, assume that the intention is to start on a TTY. Remove
        // DISPLAY or WAYLAND_DISPLAY from our environment if they are set, since they will cause
        // the winit backend to be selected instead.
        if env::var_os("DISPLAY").is_some() {
            warn!("running as a session but DISPLAY is set, removing it");
            env::remove_var("DISPLAY");
        }
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            warn!("running as a session but WAYLAND_DISPLAY is set, removing it");
            env::remove_var("WAYLAND_DISPLAY");
        }

        // Set the current desktop for xdg-desktop-portal.
        env::set_var("XDG_CURRENT_DESKTOP", "niri");
        // Ensure the session type is set to Wayland for xdg-autostart and Qt apps.
        env::set_var("XDG_SESSION_TYPE", "wayland");
    }

    let _client = tracy_client::Client::start();

    // Set a better error printer for config loading.
    niri_config::set_miette_hook().unwrap();

    // Handle subcommands.
    if let Some(subcommand) = cli.subcommand {
        match subcommand {
            Sub::Validate { config } => {
                let path = config
                    .or_else(default_config_path)
                    .expect("error getting config path");
                Config::load(&path)?;
                info!("config is valid");
                return Ok(());
            }
            Sub::Msg { msg, json } => {
                handle_msg(msg, json)?;
                return Ok(());
            }
            Sub::Panic => cause_panic(),
        }
    }

    info!("starting version {}", &version());

    // Load the config.
    let mut config_created = false;
    let path = cli.config.or_else(|| {
        let default_path = default_config_path()?;
        let default_parent = default_path.parent().unwrap();

        if let Err(err) = fs::create_dir_all(default_parent) {
            warn!(
                "error creating config directories {:?}: {err:?}",
                default_parent
            );
            return Some(default_path);
        }

        // Create the config and fill it with the default config if it doesn't exist.
        let new_file = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&default_path);
        match new_file {
            Ok(mut new_file) => {
                let default = include_bytes!("../resources/default-config.kdl");
                match new_file.write_all(default) {
                    Ok(()) => {
                        config_created = true;
                        info!("wrote default config to {:?}", &default_path);
                    }
                    Err(err) => {
                        warn!("error writing config file at {:?}: {err:?}", &default_path)
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => warn!("error creating config file at {:?}: {err:?}", &default_path),
        }

        Some(default_path)
    });

    let mut config_errored = false;
    let mut config = path
        .as_deref()
        .and_then(|path| match Config::load(path) {
            Ok(config) => Some(config),
            Err(err) => {
                warn!("{err:?}");
                config_errored = true;
                None
            }
        })
        .unwrap_or_default();

    let slowdown = if config.animations.off {
        0.
    } else {
        config.animations.slowdown.clamp(0., 100.)
    };
    animation::ANIMATION_SLOWDOWN.store(slowdown, Ordering::Relaxed);

    let spawn_at_startup = mem::take(&mut config.spawn_at_startup);
    *CHILD_ENV.write().unwrap() = mem::take(&mut config.environment);

    // Create the compositor.
    let mut event_loop = EventLoop::try_new().unwrap();
    let display = Display::new().unwrap();
    let mut state = State::new(
        config,
        event_loop.handle(),
        event_loop.get_signal(),
        display,
    )
    .unwrap();

    // Set WAYLAND_DISPLAY for children.
    let socket_name = &state.niri.socket_name;
    env::set_var("WAYLAND_DISPLAY", socket_name);
    info!(
        "listening on Wayland socket: {}",
        socket_name.to_string_lossy()
    );

    // Set NIRI_SOCKET for children.
    if let Some(ipc) = &state.niri.ipc_server {
        env::set_var(niri_ipc::SOCKET_PATH_ENV, &ipc.socket_path);
        info!("IPC listening on: {}", ipc.socket_path.to_string_lossy());
    }

    if cli.session {
        // We're starting as a session. Import our variables.
        import_environment();

        // Inhibit power key handling so we can suspend on it.
        #[cfg(feature = "dbus")]
        if !state.niri.config.borrow().input.disable_power_key_handling {
            if let Err(err) = state.niri.inhibit_power_key() {
                warn!("error inhibiting power key: {err:?}");
            }
        }
    }

    #[cfg(feature = "dbus")]
    dbus::DBusServers::start(&mut state, cli.session);

    // Notify systemd we're ready.
    if let Err(err) = sd_notify::notify(true, &[NotifyState::Ready]) {
        warn!("error notifying systemd: {err:?}");
    };

    // Send ready notification to the NOTIFY_FD file descriptor.
    if let Err(err) = notify_fd() {
        warn!("error notifying fd: {err:?}");
    }

    // Set up config file watcher.
    let _watcher = if let Some(path) = path.clone() {
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

    // Show the config error notification right away if needed.
    if config_errored {
        state.niri.config_error_notification.show();
    } else if config_created {
        state.niri.config_error_notification.show_created(path);
    }

    // Run the compositor.
    event_loop
        .run(None, &mut state, |state| state.refresh_and_flush_clients())
        .unwrap();

    Ok(())
}

fn import_environment() {
    let variables = [
        "WAYLAND_DISPLAY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        niri_ipc::SOCKET_PATH_ENV,
    ]
    .join(" ");

    let mut init_system_import = String::new();
    if cfg!(feature = "systemd") {
        write!(
            init_system_import,
            "systemctl --user import-environment {variables};"
        )
        .unwrap();
    }
    if cfg!(feature = "dinit") {
        write!(init_system_import, "dinitctl setenv {variables};").unwrap();
    }

    let rv = Command::new("/bin/sh")
        .args([
            "-c",
            &format!(
                "{init_system_import}\
                 hash dbus-update-activation-environment 2>/dev/null && \
                 dbus-update-activation-environment {variables}"
            ),
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
            warn!("error spawning shell to import environment: {err:?}");
        }
    }
}

fn default_config_path() -> Option<PathBuf> {
    let Some(dirs) = ProjectDirs::from("", "", "niri") else {
        warn!("error retrieving home directory");
        return None;
    };

    let mut path = dirs.config_dir().to_owned();
    path.push("config.kdl");
    Some(path)
}

fn notify_fd() -> anyhow::Result<()> {
    let fd = match env::var("NOTIFY_FD") {
        Ok(notify_fd) => notify_fd.parse()?,
        Err(env::VarError::NotPresent) => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    env::remove_var("NOTIFY_FD");
    let mut notif = unsafe { File::from_raw_fd(fd) };
    notif.write_all(b"READY=1\n")?;
    Ok(())
}
