//! Helper for blocking communication over the niri socket.

use std::env;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::{ffi::OsStrExt, net::UnixStream};
use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::{Event, Reply, Request};

/// Name of the environment variable containing the niri IPC socket path.
pub const SOCKET_PATH_ENV: &str = "NIRI_SOCKET";

/// Helper for blocking communication over the niri socket.
///
/// This struct is used to communicate with the niri IPC server. It handles the socket connection
/// and serialization/deserialization of messages.
pub struct Socket {
    stream: BufReader<UnixStream>,
}

/// Helper for finding the directory in which the niri
/// socket will appear.
pub fn socket_dir() -> PathBuf {
    BaseDirs::new()
        .as_ref()
        .and_then(|x| x.runtime_dir())
        .map(|x| x.to_owned())
        .unwrap_or_else(env::temp_dir)
}

impl Socket {
    /// Connects to the default niri IPC socket.
    ///
    /// If [`SOCKET_PATH_ENV`] is not defined, this method will attempt to find
    /// a valid niri IPC socket by walking the [`socket_dir`] and optionally
    /// filtering by the current `WAYLAND_DISPLAY` if present. If there are
    /// multiple options for what the niri IPC socket could be, this method
    /// returns an error.
    ///
    /// This is equivalent to calling [`Self::connect_to`] with the path taken from the
    /// [`SOCKET_PATH_ENV`] environment variable.
    pub fn connect() -> io::Result<Self> {
        let socket_path = env::var_os(SOCKET_PATH_ENV)
            .or_else(|| {
                let mut niri_socket_pattern = OsString::from("niri.");
                if let Some(wayland_display) = std::env::var_os("WAYLAND_DISPLAY") {
                    niri_socket_pattern.push(&wayland_display);
                    niri_socket_pattern.push(".");
                }
                let mut socket_dir_iter =
                    std::fs::read_dir(socket_dir()).ok()?.flatten().filter(|d| {
                        d.path()
                            .file_name()
                            .map(|n| n.as_bytes().starts_with(niri_socket_pattern.as_bytes()))
                            .unwrap_or_default()
                    });
                let socket_dir_result = socket_dir_iter.next()?;
                if socket_dir_iter.next().is_some() {
                    None
                } else {
                    Some(socket_dir_result.path().into_os_string())
                }
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{SOCKET_PATH_ENV} is not set, are you running this within niri?"),
                )
            })?;
        Self::connect_to(socket_path)
    }

    /// Connects to the niri IPC socket at the given path.
    pub fn connect_to(path: impl AsRef<Path>) -> io::Result<Self> {
        let stream = UnixStream::connect(path.as_ref())?;
        let stream = BufReader::new(stream);
        Ok(Self { stream })
    }

    /// Sends a request to niri and returns the response.
    ///
    /// Return values:
    ///
    /// * `Ok(Ok(response))`: successful [`Response`](crate::Response) from niri
    /// * `Ok(Err(message))`: error message from niri
    /// * `Err(error)`: error communicating with niri
    pub fn send(&mut self, request: Request) -> io::Result<Reply> {
        let mut buf = serde_json::to_string(&request).unwrap();
        buf.push('\n');
        self.stream.get_mut().write_all(buf.as_bytes())?;

        buf.clear();
        self.stream.read_line(&mut buf)?;

        let reply = serde_json::from_str(&buf)?;
        Ok(reply)
    }

    /// Starts reading event stream [`Event`]s from the socket.
    ///
    /// The returned function will block until the next [`Event`] arrives, then return it.
    ///
    /// Use this only after requesting an [`EventStream`][Request::EventStream].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use niri_ipc::{Request, Response};
    /// use niri_ipc::socket::Socket;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     let mut socket = Socket::connect()?;
    ///
    ///     let reply = socket.send(Request::EventStream)?;
    ///     if matches!(reply, Ok(Response::Handled)) {
    ///         let mut read_event = socket.read_events();
    ///         while let Ok(event) = read_event() {
    ///             println!("Received event: {event:?}");
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn read_events(self) -> impl FnMut() -> io::Result<Event> {
        let Self { mut stream } = self;
        let _ = stream.get_mut().shutdown(Shutdown::Write);

        let mut buf = String::new();
        move || {
            buf.clear();
            stream.read_line(&mut buf)?;
            let event = serde_json::from_str(&buf)?;
            Ok(event)
        }
    }
}
