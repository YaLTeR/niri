use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{env, io, process};

use anyhow::Context;
use calloop::io::Async;
use directories::BaseDirs;
use futures_util::io::{AsyncReadExt, BufReader};
use futures_util::{AsyncBufReadExt, AsyncWriteExt, StreamExt};
use niri_ipc::{
    ActionRequest, Error, ErrorRequest, FocusedWindowRequest, MaybeJson, MaybeUnknown,
    OutputRequest, Reply, Request, RequestMessage, VersionRequest,
};
use smithay::desktop::Window;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::rustix::fs::unlink;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;

use crate::backend::IpcOutputMap;
use crate::niri::State;
use crate::utils::version;

pub struct IpcServer {
    pub socket_path: PathBuf,
}

struct ClientCtx {
    event_loop: LoopHandle<'static, State>,
    ipc_outputs: Arc<Mutex<IpcOutputMap>>,
    ipc_focused_window: Arc<Mutex<Option<Window>>>,
}

impl IpcServer {
    pub fn start(
        event_loop: &LoopHandle<'static, State>,
        wayland_socket_name: &str,
    ) -> anyhow::Result<Self> {
        let _span = tracy_client::span!("Ipc::start");

        let socket_name = format!("niri.{wayland_socket_name}.{}.sock", process::id());
        let mut socket_path = socket_dir();
        socket_path.push(socket_name);

        let listener = UnixListener::bind(&socket_path).context("error binding socket")?;
        listener
            .set_nonblocking(true)
            .context("error setting socket to non-blocking")?;

        let source = Generic::new(listener, Interest::READ, Mode::Level);
        event_loop
            .insert_source(source, |_, socket, state| {
                match socket.accept() {
                    Ok((stream, _)) => on_new_ipc_client(state, stream),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                    Err(e) => return Err(e),
                }

                Ok(PostAction::Continue)
            })
            .unwrap();

        Ok(Self { socket_path })
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = unlink(&self.socket_path);
    }
}

fn socket_dir() -> PathBuf {
    BaseDirs::new()
        .as_ref()
        .and_then(|x| x.runtime_dir())
        .map(|x| x.to_owned())
        .unwrap_or_else(env::temp_dir)
}

fn on_new_ipc_client(state: &mut State, stream: UnixStream) {
    let _span = tracy_client::span!("on_new_ipc_client");
    trace!("new IPC client connected");

    let stream = match state.niri.event_loop.adapt_io(stream) {
        Ok(stream) => stream,
        Err(err) => {
            warn!("error making IPC stream async: {err:?}");
            return;
        }
    };

    let ctx = ClientCtx {
        event_loop: state.niri.event_loop.clone(),
        ipc_outputs: state.backend.ipc_outputs(),
        ipc_focused_window: state.niri.ipc_focused_window.clone(),
    };

    let future = async move {
        if let Err(err) = handle_client(ctx, stream).await {
            warn!("error handling IPC client: {err:?}");
        }
    };
    if let Err(err) = state.niri.scheduler.schedule(future) {
        warn!("error scheduling IPC stream future: {err:?}");
    }
}

async fn handle_client(ctx: ClientCtx, stream: Async<'_, UnixStream>) -> anyhow::Result<()> {
    let (read, mut write) = stream.split();

    // note that we can't use the stream json deserializer here
    // because the stream is asynchronous and the deserializer doesn't support that
    // https://github.com/serde-rs/json/issues/575

    let mut lines = BufReader::new(read).lines();

    let line = match lines.next().await.unwrap_or(Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Unreachable; BufReader returned None but when the stream ends, the connection should be reset"))) {
        Ok(line) => line,
        Err(err) if err.kind() == io::ErrorKind::ConnectionReset => return Ok(()),
        Err(err) => return Err(err).context("error reading line"),
    };

    let reply = process(&ctx, line);

    if let Err(err) = &reply {
        warn!("error processing IPC request: {err:?}");
    }

    let mut buf = serde_json::to_vec(&reply).context("error formatting reply")?;
    writeln!(buf).unwrap();
    write.write_all(&buf).await.context("error writing reply")?;
    write.flush().await.context("error flushing reply")?;

    // We do not check for more lines at this moment.
    // Dropping the stream will reset the connection before we read them.
    // For now, a client should not be sending more than one request per connection.

    Ok(())
}

trait HandleRequest: Request {
    fn handle(self, ctx: &ClientCtx) -> Reply<Self::Response>;
}

fn process(ctx: &ClientCtx, line: String) -> Reply<serde_json::Value> {
    let deserialized: MaybeJson<RequestMessage> = serde_json::from_str(&line).map_err(|err| {
        warn!("error parsing IPC request: {err:?}");
        Error::ClientBadJson
    })?;

    match deserialized {
        MaybeUnknown::Known(request) => niri_ipc::dispatch!(request, |req| {
            req.handle(ctx).and_then(|v| {
                serde_json::to_value(v).map_err(|err| {
                    warn!("error serializing response to IPC request: {err:?}");
                    Error::InternalError
                })
            })
        }),
        MaybeUnknown::Unknown(payload) => {
            warn!("client sent an invalid payload: {payload}");
            Err(Error::ClientBadProtocol)
        }
    }
}

impl HandleRequest for ErrorRequest {
    fn handle(self, _ctx: &ClientCtx) -> Reply<Self::Response> {
        Err(Error::Other(self.0))
    }
}

impl HandleRequest for VersionRequest {
    fn handle(self, _ctx: &ClientCtx) -> Reply<Self::Response> {
        Ok(version())
    }
}

impl HandleRequest for OutputRequest {
    fn handle(self, ctx: &ClientCtx) -> Reply<Self::Response> {
        Ok(ctx
            .ipc_outputs
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .collect())
    }
}

impl HandleRequest for FocusedWindowRequest {
    fn handle(self, ctx: &ClientCtx) -> Reply<Self::Response> {
        Ok(ctx
            .ipc_focused_window
            .lock()
            .unwrap()
            .clone()
            .map(|window| {
                let wl_surface = window.toplevel().expect("no X11 support").wl_surface();
                with_states(wl_surface, |states| {
                    let role = states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap();

                    niri_ipc::Window {
                        title: role.title.clone(),
                        app_id: role.app_id.clone(),
                    }
                })
            }))
    }
}

impl HandleRequest for ActionRequest {
    fn handle(self, ctx: &ClientCtx) -> Reply<Self::Response> {
        ctx.event_loop.insert_idle(move |state| {
            state.do_action(self.0.into());
        });
        Ok(())
    }
}
