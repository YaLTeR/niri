//! Helper for blocking communication over the niri socket.

use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use crate::{Event, Reply, Request};

/// Name of the environment variable containing the niri IPC socket path.
pub const SOCKET_PATH_ENV: &str = "NIRI_SOCKET";

/// Helper for blocking communication over the niri socket.
///
/// This struct is used to communicate with the niri IPC server. It handles the socket connection
/// and serialization/deserialization of messages. This class allows you to use `send` only once.
/// After first call of `send` you have to call `connect` again to communicate with server.
pub struct Socket {
    stream: UnixStream,
}

impl Socket {
    /// Get path to the default niri IPC socket.
    ///
    /// This returns path taken from the [`SOCKET_PATH_ENV`] environment variable.
    pub fn default_socket_path() -> io::Result<impl AsRef<Path>> {
        env::var_os(SOCKET_PATH_ENV).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{SOCKET_PATH_ENV} is not set, are you running this within niri?"),
            )
        })
    }
    /// Connects to the default niri IPC socket.
    ///
    /// This is equivalent to calling [`Self::connect_to`] with the path taken from the
    /// [`SOCKET_PATH_ENV`] environment variable.
    pub fn connect() -> io::Result<Self> {
        Self::connect_to(Self::default_socket_path()?)
    }

    /// Connects to the niri IPC socket at the given path.
    pub fn connect_to(path: impl AsRef<Path>) -> io::Result<Self> {
        let stream = UnixStream::connect(path.as_ref())?;
        Ok(Self { stream })
    }

    /// Sends a request to niri and returns the response.
    ///
    /// Return values:
    ///
    /// * `Ok(Ok(response))`: successful [`Response`](crate::Response) from niri
    /// * `Ok(Err(message))`: error message from niri
    /// * `Err(error)`: error communicating with niri
    ///
    /// This method also returns a blocking function that you can call to keep reading [`Event`]s
    /// after requesting an [`EventStream`][Request::EventStream]. This function is not useful
    /// otherwise.
    pub fn send(self, request: Request) -> io::Result<(Reply, impl FnMut() -> io::Result<Event>)> {
        let Self { mut stream } = self;

        let mut buf = serde_json::to_string(&request).unwrap();
        stream.write_all(buf.as_bytes())?;
        stream.shutdown(Shutdown::Write)?;

        let mut reader = BufReader::new(stream);

        buf.clear();
        reader.read_line(&mut buf)?;

        let reply = serde_json::from_str(&buf)?;

        let events = move || {
            buf.clear();
            reader.read_line(&mut buf)?;
            let event = serde_json::from_str(&buf)?;
            Ok(event)
        };

        Ok((reply, events))
    }
}

/// Wrapper on [Socket] which allows to reuse single object for many `send` calls.
pub struct MultiSocket {
    path: PathBuf,
}

impl MultiSocket {
    /// Equivalent to [Socket::connect]
    ///
    /// This stores path taken from [`SOCKET_PATH_ENV`] environment variable.
    pub fn connect() -> io::Result<Self> {
        Ok(Self::connect_to(Socket::default_socket_path()?))
    }

    /// Equivalent to [Socket::connect_to]
    ///
    /// This stores path passed from argument.
    pub fn connect_to(path_ref: impl AsRef<Path>) -> Self {
        let mut path = PathBuf::new();
        path.push(path_ref);
        Self { path }
    }

    /// Wrapper on [Socket::send]
    ///
    /// Creates temporary [Socket] object, calls its [`send`](Socket::send) method
    /// and returns its result
    pub fn send(&self, request: Request) -> io::Result<(Reply, impl FnMut() -> io::Result<Event>)> {
        self.get_socket()?.send(request)
    }

    /// Returns [Socket]
    ///
    /// Uses stored socket path to create and return [Socket] object
    pub fn get_socket(&self) -> io::Result<Socket> {
        Socket::connect_to(&self.path)
    }
}
