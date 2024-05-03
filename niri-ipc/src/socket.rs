use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use serde_json::de::IoRead;
use serde_json::StreamDeserializer;

use crate::{MaybeJson, Reply, Request};

/// Name of the environment variable containing the niri IPC socket path.
pub const SOCKET_PATH_ENV: &str = "NIRI_SOCKET";

/// A client for the niri IPC server.
///
/// This struct is used to communicate with the niri IPC server. It handles the socket connection
/// and serialization/deserialization of messages.
pub struct NiriSocket {
    stream: UnixStream,
    responses: StreamDeserializer<'static, IoRead<UnixStream>, serde_json::Value>,
}

impl TryFrom<UnixStream> for NiriSocket {
    type Error = io::Error;
    fn try_from(stream: UnixStream) -> io::Result<Self> {
        let responses = serde_json::Deserializer::from_reader(stream.try_clone()?).into_iter();
        Ok(Self { stream, responses })
    }
}

impl NiriSocket {
    /// Connects to the default niri IPC socket
    ///
    /// This is equivalent to calling [Self::connect] with the value of the [SOCKET_PATH_ENV]
    /// environment variable.
    pub fn new() -> io::Result<Self> {
        let socket_path = std::env::var_os(SOCKET_PATH_ENV).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{SOCKET_PATH_ENV} is not set, are you running this within niri?"),
            )
        })?;
        Self::connect(socket_path)
    }

    /// Connect to the socket at the given path
    ///
    /// See also: [UnixStream::connect]
    pub fn connect(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::try_from(UnixStream::connect(path.as_ref())?)
    }

    /// Handle a request to the niri IPC server
    ///
    /// # Returns
    /// - Ok(Ok([Response](crate::Response))) corresponds to a successful response from the running
    /// niri instance.
    /// - Ok(Err([String])) corresponds to an error received from the running niri
    /// instance.
    /// - Err([std::io::Error]) corresponds to an error in the IPC communication.
    pub fn send_request<R: Request>(mut self, request: R) -> io::Result<Reply<R::Response>> {
        let mut buf = serde_json::to_vec(&request.into_message()).unwrap();
        writeln!(buf).unwrap();
        self.stream.write_all(&buf)?; // .context("error writing IPC request")?;
        self.stream.flush()?;

        if let Some(next) = self.responses.next() {
            next.and_then(serde_json::from_value)
                .map(|v| match v {
                    MaybeJson::Known(reply) => reply,
                    MaybeJson::Unknown(_) => Err(crate::Error::CompositorBadProtocol),
                })
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "no response from server",
            ))
        }
    }
}
