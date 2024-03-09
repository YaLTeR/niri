use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::{io, thread};

use niri_config::Environment;

use crate::utils::expand_home;

pub static REMOVE_ENV_RUST_BACKTRACE: AtomicBool = AtomicBool::new(false);
pub static REMOVE_ENV_RUST_LIB_BACKTRACE: AtomicBool = AtomicBool::new(false);
pub static CHILD_ENV: RwLock<Environment> = RwLock::new(Environment(Vec::new()));

/// Spawns the command to run independently of the compositor.
pub fn spawn<T: AsRef<OsStr> + Send + 'static>(command: Vec<T>) {
    let _span = tracy_client::span!();

    if command.is_empty() {
        return;
    }

    // Spawning and waiting takes some milliseconds, so do it in a thread.
    let res = thread::Builder::new()
        .name("Command Spawner".to_owned())
        .spawn(move || {
            let (command, args) = command.split_first().unwrap();
            spawn_sync(command, args);
        });

    if let Err(err) = res {
        warn!("error spawning a thread to spawn the command: {err:?}");
    }
}

fn spawn_sync(command: impl AsRef<OsStr>, args: impl IntoIterator<Item = impl AsRef<OsStr>>) {
    let _span = tracy_client::span!();

    let mut command = command.as_ref();

    // Expand `~` at the start.
    let expanded = expand_home(Path::new(command));
    match &expanded {
        Ok(Some(expanded)) => command = expanded.as_ref(),
        Ok(None) => (),
        Err(err) => {
            warn!("error expanding ~: {err:?}");
        }
    }

    let mut process = Command::new(command);
    process
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Remove RUST_BACKTRACE and RUST_LIB_BACKTRACE from the environment if needed.
    if REMOVE_ENV_RUST_BACKTRACE.load(Ordering::Relaxed) {
        process.env_remove("RUST_BACKTRACE");
    }
    if REMOVE_ENV_RUST_LIB_BACKTRACE.load(Ordering::Relaxed) {
        process.env_remove("RUST_LIB_BACKTRACE");
    }

    // Set configured environment.
    let env = CHILD_ENV.read().unwrap();
    for var in &env.0 {
        if let Some(value) = &var.value {
            process.env(&var.name, value);
        } else {
            process.env_remove(&var.name);
        }
    }
    drop(env);

    let Some(mut child) = do_spawn(command, process) else {
        return;
    };

    match child.wait() {
        Ok(status) => {
            if !status.success() {
                warn!("child did not exit successfully: {status:?}");
            }
        }
        Err(err) => {
            warn!("error waiting for child: {err:?}");
        }
    }
}

#[cfg(not(feature = "systemd"))]
fn do_spawn(command: &OsStr, mut process: Command) -> Option<Child> {
    unsafe {
        // Double-fork to avoid having to waitpid the child.
        process.pre_exec(move || {
            match libc::fork() {
                -1 => return Err(io::Error::last_os_error()),
                0 => (),
                _ => libc::_exit(0),
            }

            Ok(())
        });
    }

    let child = match process.spawn() {
        Ok(child) => child,
        Err(err) => {
            warn!("error spawning {command:?}: {err:?}");
            return None;
        }
    };

    Some(child)
}

#[cfg(feature = "systemd")]
use systemd::do_spawn;

#[cfg(feature = "systemd")]
mod systemd {
    use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};

    use smithay::reexports::rustix;
    use smithay::reexports::rustix::io::{close, read, retry_on_intr, write};
    use smithay::reexports::rustix::pipe::{pipe_with, PipeFlags};

    use super::*;

    pub fn do_spawn(command: &OsStr, mut process: Command) -> Option<Child> {
        use libc::close_range;

        // When running as a systemd session, we want to put children into their own transient
        // scopes in order to separate them from the niri process. This is helpful for
        // example to prevent the OOM killer from taking down niri together with a
        // misbehaving client.
        //
        // Putting a child into a scope is done by calling systemd's StartTransientUnit D-Bus method
        // with a PID. Unfortunately, there seems to be a race in systemd where if the child exits
        // at just the right time, the transient unit will be created but empty, so it will
        // linger around forever.
        //
        // To prevent this, we'll use our double-fork (done for a separate reason) to help. In our
        // intermediate child we will send back the grandchild PID, and in niri we will create a
        // transient scope with both our intermediate child and the grandchild PIDs set. Only then
        // we will signal our intermediate child to exit. This way, even if the grandchild
        // exits quickly, a non-empty scope will be created (with just our intermediate
        // child), then cleaned up when our intermediate child exits.

        // Make a pipe to receive the grandchild PID.

        let (pipe_pid_read, pipe_pid_write) = pipe_with(PipeFlags::CLOEXEC)
            .map_err(|err| {
                warn!("error creating a pipe to transfer child PID: {err:?}");
            })
            .ok()
            .unzip();
        // Make a pipe to wait in the intermediate child.
        let (pipe_wait_read, pipe_wait_write) = pipe_with(PipeFlags::CLOEXEC)
            .map_err(|err| {
                warn!("error creating a pipe for child to wait on: {err:?}");
            })
            .ok()
            .unzip();

        unsafe {
            // The fds will be duplicated after a fork and closed on exec or exit automatically. Get
            // the raw fd inside so that it's not closed any extra times.
            let mut pipe_pid_read_fd = pipe_pid_read.as_ref().map(|fd| fd.as_raw_fd());
            let mut pipe_pid_write_fd = pipe_pid_write.as_ref().map(|fd| fd.as_raw_fd());
            let mut pipe_wait_read_fd = pipe_wait_read.as_ref().map(|fd| fd.as_raw_fd());
            let mut pipe_wait_write_fd = pipe_wait_write.as_ref().map(|fd| fd.as_raw_fd());

            // Double-fork to avoid having to waitpid the child.
            process.pre_exec(move || {
                // Close FDs that we don't need. Especially important for the write ones to unblock
                // the readers.
                if let Some(fd) = pipe_pid_read_fd.take() {
                    close(fd);
                }
                if let Some(fd) = pipe_wait_write_fd.take() {
                    close(fd);
                }

                // Convert the our FDs to OwnedFd, which will close them in all of our fork paths.
                let pipe_pid_write = pipe_pid_write_fd.take().map(|fd| OwnedFd::from_raw_fd(fd));
                let pipe_wait_read = pipe_wait_read_fd.take().map(|fd| OwnedFd::from_raw_fd(fd));

                match libc::fork() {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => (),
                    grandchild_pid => {
                        // Send back the PID.
                        if let Some(pipe) = pipe_pid_write {
                            let _ = write_all(pipe, &grandchild_pid.to_ne_bytes());
                        }

                        // Wait until the parent signals us to exit.
                        if let Some(pipe) = pipe_wait_read {
                            // We're going to exit afterwards. Close all other FDs to allow
                            // Command::spawn() to return in the parent process.
                            let raw = pipe.as_raw_fd() as u32;
                            let _ = close_range(0, raw - 1, 0);
                            let _ = close_range(raw + 1, !0, 0);

                            let _ = read_all(pipe, &mut [0]);
                        }

                        libc::_exit(0)
                    }
                }

                Ok(())
            });
        }

        let child = match process.spawn() {
            Ok(child) => child,
            Err(err) => {
                warn!("error spawning {command:?}: {err:?}");
                return None;
            }
        };

        drop(pipe_pid_write);
        drop(pipe_wait_read);

        // Wait for the grandchild PID.
        if let Some(pipe) = pipe_pid_read {
            let mut buf = [0; 4];
            match read_all(pipe, &mut buf) {
                Ok(()) => {
                    let pid = i32::from_ne_bytes(buf);
                    trace!("spawned PID: {pid}");

                    // Start a systemd scope for the grandchild.
                    #[cfg(feature = "systemd")]
                    if let Err(err) = start_systemd_scope(command, child.id(), pid as u32) {
                        trace!("error starting systemd scope for spawned command: {err:?}");
                    }
                }
                Err(err) => {
                    warn!("error reading child PID: {err:?}");
                }
            }
        }

        // Signal the intermediate child to exit now that we're done trying to creating a systemd
        // scope.
        trace!("signaling child to exit");
        drop(pipe_wait_write);

        Some(child)
    }

    #[cfg(feature = "systemd")]
    fn write_all(fd: impl AsFd, buf: &[u8]) -> rustix::io::Result<()> {
        let mut written = 0;
        loop {
            let n = retry_on_intr(|| write(&fd, &buf[written..]))?;
            if n == 0 {
                return Err(rustix::io::Errno::CANCELED);
            }

            written += n;
            if written == buf.len() {
                return Ok(());
            }
        }
    }

    #[cfg(feature = "systemd")]
    fn read_all(fd: impl AsFd, buf: &mut [u8]) -> rustix::io::Result<()> {
        let mut start = 0;
        loop {
            let n = retry_on_intr(|| read(&fd, &mut buf[start..]))?;
            if n == 0 {
                return Err(rustix::io::Errno::CANCELED);
            }

            start += n;
            if start == buf.len() {
                return Ok(());
            }
        }
    }

    /// Puts a (newly spawned) pid into a transient systemd scope.
    ///
    /// This separates the pid from the compositor scope, which for example prevents the OOM killer
    /// from bringing down the compositor together with a misbehaving client.
    #[cfg(feature = "systemd")]
    fn start_systemd_scope(
        name: &OsStr,
        intermediate_pid: u32,
        child_pid: u32,
    ) -> anyhow::Result<()> {
        use std::fmt::Write as _;
        use std::os::unix::ffi::OsStrExt;
        use std::sync::OnceLock;

        use anyhow::Context;
        use zbus::zvariant::{OwnedObjectPath, Value};

        use crate::utils::IS_SYSTEMD_SERVICE;

        // We only start transient scopes if we're a systemd service ourselves.
        if !IS_SYSTEMD_SERVICE.load(Ordering::Relaxed) {
            return Ok(());
        }

        let _span = tracy_client::span!();

        // Extract the basename.
        let name = Path::new(name).file_name().unwrap_or(name);

        let mut scope_name = String::from("app-niri-");

        // Escape for systemd similarly to libgnome-desktop, which says it had adapted this from
        // systemd source.
        for &c in name.as_bytes() {
            if c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'.') {
                scope_name.push(char::from(c));
            } else {
                let _ = write!(scope_name, "\\x{c:02x}");
            }
        }

        let _ = write!(scope_name, "-{child_pid}.scope");

        // Ask systemd to start a transient scope.
        static CONNECTION: OnceLock<zbus::Result<zbus::blocking::Connection>> = OnceLock::new();
        let conn = CONNECTION
            .get_or_init(zbus::blocking::Connection::session)
            .clone()
            .context("error connecting to session bus")?;

        let proxy = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )
        .context("error creating a Proxy")?;

        let signals = proxy
            .receive_signal("JobRemoved")
            .context("error creating a signal iterator")?;

        let pids: &[_] = &[intermediate_pid, child_pid];
        let properties: &[_] = &[
            ("PIDs", Value::new(pids)),
            ("CollectMode", Value::new("inactive-or-failed")),
        ];
        let aux: &[(&str, &[(&str, Value)])] = &[];

        let job: OwnedObjectPath = proxy
            .call("StartTransientUnit", &(scope_name, "fail", properties, aux))
            .context("error calling StartTransientUnit")?;

        trace!("waiting for JobRemoved");
        for message in signals {
            let body: (u32, OwnedObjectPath, &str, &str) =
                message.body().context("error parsing signal")?;

            if body.1 == job {
                // Our transient unit had started, we're good to exit the intermediate child.
                break;
            }
        }

        Ok(())
    }
}
