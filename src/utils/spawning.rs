use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};
use std::{io, thread};

use anyhow::Context;
use atomic::Atomic;
use libc::{getrlimit, rlim_t, rlimit, setrlimit, RLIMIT_NOFILE};
use niri_config::Environment;
use smithay::reexports::wayland_server::{DisplayHandle, Resource};
use smithay::wayland::compositor;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData};
use zbus::zvariant::Value;

use crate::utils::expand_home;

pub static REMOVE_ENV_RUST_BACKTRACE: AtomicBool = AtomicBool::new(false);
pub static REMOVE_ENV_RUST_LIB_BACKTRACE: AtomicBool = AtomicBool::new(false);
pub static CHILD_ENV: RwLock<Environment> = RwLock::new(Environment(Vec::new()));

static ORIGINAL_NOFILE_RLIMIT_CUR: Atomic<rlim_t> = Atomic::new(0);
static ORIGINAL_NOFILE_RLIMIT_MAX: Atomic<rlim_t> = Atomic::new(0);

/// Increases the nofile rlimit to the maximum and stores the original value.
pub fn store_and_increase_nofile_rlimit() {
    let mut rlim = rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { getrlimit(RLIMIT_NOFILE, &mut rlim) } != 0 {
        let err = io::Error::last_os_error();
        warn!("error getting nofile rlimit: {err:?}");
        return;
    }

    ORIGINAL_NOFILE_RLIMIT_CUR.store(rlim.rlim_cur, Ordering::SeqCst);
    ORIGINAL_NOFILE_RLIMIT_MAX.store(rlim.rlim_max, Ordering::SeqCst);

    trace!(
        "changing nofile rlimit from {} to {}",
        rlim.rlim_cur,
        rlim.rlim_max
    );
    rlim.rlim_cur = rlim.rlim_max;

    if unsafe { setrlimit(RLIMIT_NOFILE, &rlim) } != 0 {
        let err = io::Error::last_os_error();
        warn!("error setting nofile rlimit: {err:?}");
    }
}

/// Restores the original nofile rlimit.
pub fn restore_nofile_rlimit() {
    let rlim_cur = ORIGINAL_NOFILE_RLIMIT_CUR.load(Ordering::SeqCst);
    let rlim_max = ORIGINAL_NOFILE_RLIMIT_MAX.load(Ordering::SeqCst);

    if rlim_cur == 0 {
        return;
    }

    let rlim = rlimit { rlim_cur, rlim_max };
    unsafe { setrlimit(RLIMIT_NOFILE, &rlim) };
}

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

            restore_nofile_rlimit();

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

    use serde::{Deserialize, Serialize};
    use smithay::reexports::rustix;
    use smithay::reexports::rustix::io::{close, read, retry_on_intr, write};
    use smithay::reexports::rustix::pipe::{pipe_with, PipeFlags};
    use zbus::dbus_proxy;
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Type, Value};

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

                restore_nofile_rlimit();

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

        let mut slice_name = format!("app-niri-");

        // Escape for systemd similarly to libgnome-desktop, which says it had adapted this from
        // systemd source.
        for &c in name.as_bytes() {
            if c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'.') {
                slice_name.push(char::from(c));
            } else {
                let _ = write!(slice_name, "\\x{c:02x}");
            }
        }

        let _ = write!(slice_name, ".slice");

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
            ("Slice", Value::new(&slice_name)),
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

    #[dbus_proxy(
        interface = "org.freedesktop.systemd1.Manager",
        default_service = "org.freedesktop.systemd1",
        default_path = "/org/freedesktop/systemd1"
    )]
    trait Manager {
        #[dbus_proxy(name = "GetUnitByPID")]
        fn get_unit_by_pid(&self, pid: u32) -> zbus::Result<OwnedObjectPath>;

        #[dbus_proxy(name = "StartTransientUnit")]
        fn start_transient_unit(
            &self,
            name: &str,
            mode: &str,
            properties: &[(&str, Value<'_>)],
            aux: &[(&str, &[(&str, Value<'_>)])],
        ) -> zbus::Result<OwnedObjectPath>;

        #[dbus_proxy(signal)]
        fn job_removed(
            &self,
            id: u32,
            job: zbus::zvariant::ObjectPath<'_>,
            unit: &str,
            result: &str,
        ) -> zbus::Result<()>;
    }

    /// A process spawned by systemd for a unit.
    #[derive(Debug, PartialEq, Eq, Clone, Type, Serialize, Deserialize, Value, OwnedValue)]
    pub struct Process {
        /// The cgroup controller of the process.
        pub cgroup_controller: String,

        /// The PID of the process.
        pub pid: u32,

        /// The command line of the process.
        pub command_line: String,
    }

    #[dbus_proxy(
        interface = "org.freedesktop.systemd1.Scope",
        default_service = "org.freedesktop.systemd1"
    )]
    trait Scope {
        #[dbus_proxy(property)]
        fn control_group(&self) -> zbus::Result<String>;

        fn get_processes(&self) -> zbus::Result<Vec<Process>>;
    }

    #[dbus_proxy(
        interface = "org.freedesktop.systemd1.Unit",
        default_service = "org.freedesktop.systemd1",
        default_path = "/org/freedesktop/systemd1/unit"
    )]
    trait Unit {
        fn freeze(&self) -> zbus::Result<()>;
        fn thaw(&self) -> zbus::Result<()>;
    }
}

pub fn test_scope(toplevel: &ToplevelSurface, dh: &DisplayHandle) -> anyhow::Result<()> {
    static CONNECTION: OnceLock<zbus::Result<zbus::blocking::Connection>> = OnceLock::new();
    let conn = CONNECTION
        .get_or_init(zbus::blocking::Connection::session)
        .clone()
        .context("error connecting to session bus")?;

    let manager = systemd::ManagerProxyBlocking::new(&conn).context("error creating a Proxy")?;

    let wl_surface = toplevel.wl_surface();
    let Some(client) = wl_surface.client() else {
        return Ok(());
    };

    let credentials = client.get_credentials(dh)?;
    let pid = credentials.pid as u32;

    let Some(app_id) = compositor::with_states(&wl_surface, |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|surface_data| surface_data.lock().unwrap().app_id.clone())
    }) else {
        return Ok(());
    };

    let unit_path = manager.get_unit_by_pid(pid)?;

    use std::fmt::Write;
    let mut expected_scope_name = format!("app-niri-");

    // Escape for systemd similarly to libgnome-desktop, which says it had adapted this from
    // systemd source.
    for &c in app_id.as_bytes() {
        if c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'.') {
            expected_scope_name.push(char::from(c));
        } else {
            let _ = write!(expected_scope_name, "\\x{c:02x}");
        }
    }

    let _ = write!(expected_scope_name, "-{pid}.scope");

    let scope = systemd::ScopeProxyBlocking::builder(&conn)
        .path(&unit_path)?
        .build()
        .with_context(|| format!("failed to get scope for: {unit_path:?}"))?;
    let control_group = scope.control_group()?;

    let existing_scope_name = control_group.split_terminator('/').last().unwrap();

    if existing_scope_name.eq_ignore_ascii_case(&expected_scope_name)
        || !existing_scope_name.starts_with("app-niri-")
    {
        return Ok(());
    }

    let unit = systemd::UnitProxyBlocking::builder(&conn)
        .path(&unit_path)?
        .build()?;

    let frozen = match unit.freeze() {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!(?unit_path, ?err, "failed to freeze unit");
            false
        }
    };

    let apply = || {
        let processes = scope.get_processes()?;

        let mut pids = processes
            .iter()
            .map(|process| process.pid)
            .collect::<Vec<_>>();

        let mut ppid_map: HashMap<u32, u32> = HashMap::new();

        let mut i = 0;
        while i < pids.len() {
            // self check
            if pids[i] == pid {
                i += 1;
                continue;
            }

            let pid: u32 = pids[i];

            let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid))
                .context("failed to parse stat")?;
            let ppid_start = stat.rfind(')').unwrap_or_default() + 4;
            let ppid_end = ppid_start + stat[ppid_start..].find(' ').unwrap_or(0);
            let ppid = &stat[ppid_start..ppid_end];
            let ppid = ppid
                .parse::<u32>()
                .with_context(|| format!("failed to parse ppid from stat: {stat}"))?;

            if !pids.contains(&ppid) {
                pids.remove(i);
            } else {
                ppid_map.insert(pid, ppid);
                i += 1;
            }
        }

        let mut i = 0;
        while i < pids.len() {
            // self check
            if pids[i] == pid {
                i += 1;
                continue;
            }

            let mut root_pid = pids[i];
            while let Some(&ppid) = ppid_map.get(&root_pid) {
                root_pid = ppid;
            }

            if root_pid != pid {
                pids.remove(i);
            } else {
                i += 1;
            }
        }

        let mut slice_name = format!("app-niri-");

        // Escape for systemd similarly to libgnome-desktop, which says it had adapted this from
        // systemd source.
        for &c in app_id.as_bytes() {
            if c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'.') {
                slice_name.push(char::from(c));
            } else {
                let _ = write!(slice_name, "\\x{c:02x}");
            }
        }

        let _ = write!(slice_name, ".slice");

        let properties: &[_] = &[
            ("PIDs", Value::new(pids)),
            ("CollectMode", Value::new("inactive-or-failed")),
            ("Slice", Value::new(&slice_name)),
        ];

        tracing::info!(
            ?expected_scope_name,
            ?existing_scope_name,
            "trying to move to different scope"
        );

        manager.start_transient_unit(&expected_scope_name, "fail", properties, &[])?;
        Result::<(), anyhow::Error>::Ok(())
    };

    let res = apply();

    if frozen {
        let _ = unit.thaw();
    }

    res?;
    Ok(())
}
