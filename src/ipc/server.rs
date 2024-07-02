use std::cell::RefCell;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::{env, io, process};

use anyhow::Context;
use async_channel::{Receiver, Sender, TrySendError};
use calloop::futures::Scheduler;
use calloop::io::Async;
use directories::BaseDirs;
use futures_util::io::{AsyncReadExt, BufReader};
use futures_util::{select_biased, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, FutureExt as _};
use niri_ipc::{Event, OutputConfigChanged, Reply, Request, Response};
use smithay::desktop::Window;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::rustix::fs::unlink;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;

use crate::backend::IpcOutputMap;
use crate::niri::State;
use crate::utils::version;

// If an event stream client fails to read events fast enough that we accumulate more than this
// number in our buffer, we drop that event stream client.
const EVENT_STREAM_BUFFER_SIZE: usize = 64;

pub struct IpcServer {
    pub socket_path: PathBuf,
    event_streams: Rc<RefCell<Vec<EventStreamSender>>>,
    focused_window: Arc<Mutex<Option<Window>>>,
}

struct ClientCtx {
    event_loop: LoopHandle<'static, State>,
    scheduler: Scheduler<()>,
    ipc_outputs: Arc<Mutex<IpcOutputMap>>,
    focused_window: Arc<Mutex<Option<Window>>>,
    event_streams: Rc<RefCell<Vec<EventStreamSender>>>,
}

struct EventStreamClient {
    events: Receiver<Event>,
    disconnect: Receiver<()>,
    write: Box<dyn AsyncWrite + Unpin>,
}

struct EventStreamSender {
    events: Sender<Event>,
    disconnect: Sender<()>,
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

        Ok(Self {
            socket_path,
            event_streams: Rc::new(RefCell::new(Vec::new())),
            focused_window: Arc::new(Mutex::new(None)),
        })
    }

    fn send_event(&self, event: Event) {
        let mut streams = self.event_streams.borrow_mut();
        let mut to_remove = Vec::new();
        for (idx, stream) in streams.iter_mut().enumerate() {
            match stream.events.try_send(event.clone()) {
                Ok(()) => (),
                Err(TrySendError::Closed(_)) => to_remove.push(idx),
                Err(TrySendError::Full(_)) => {
                    warn!(
                        "disconnecting IPC event stream client \
                         because it is reading events too slowly"
                    );
                    to_remove.push(idx);
                }
            }
        }

        for idx in to_remove.into_iter().rev() {
            let stream = streams.swap_remove(idx);
            let _ = stream.disconnect.send_blocking(());
        }
    }

    pub fn focused_window_changed(&self, focused_window: Option<Window>) {
        let mut guard = self.focused_window.lock().unwrap();
        if *guard == focused_window {
            return;
        }

        guard.clone_from(&focused_window);
        drop(guard);

        let window = focused_window.map(|window| {
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
        });
        self.send_event(Event::WindowFocused { window })
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

    let ipc_server = state.niri.ipc_server.as_ref().unwrap();

    let ctx = ClientCtx {
        event_loop: state.niri.event_loop.clone(),
        scheduler: state.niri.scheduler.clone(),
        ipc_outputs: state.backend.ipc_outputs(),
        focused_window: ipc_server.focused_window.clone(),
        event_streams: ipc_server.event_streams.clone(),
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

async fn handle_client(ctx: ClientCtx, stream: Async<'static, UnixStream>) -> anyhow::Result<()> {
    let (read, mut write) = stream.split();
    let mut buf = String::new();

    // Read a single line to allow extensibility in the future to keep reading.
    BufReader::new(read)
        .read_line(&mut buf)
        .await
        .context("error reading request")?;

    let request = serde_json::from_str(&buf)
        .context("error parsing request")
        .map_err(|err| err.to_string());
    let requested_error = matches!(request, Ok(Request::ReturnError));
    let requested_event_stream = matches!(request, Ok(Request::EventStream));

    let reply = match request {
        Ok(request) => process(&ctx, request).await,
        Err(err) => Err(err),
    };

    if let Err(err) = &reply {
        if !requested_error {
            warn!("error processing IPC request: {err:?}");
        }
    }

    let mut buf = serde_json::to_vec(&reply).context("error formatting reply")?;
    buf.push(b'\n');
    write.write_all(&buf).await.context("error writing reply")?;

    if requested_event_stream {
        let (events_tx, events_rx) = async_channel::bounded(EVENT_STREAM_BUFFER_SIZE);
        let (disconnect_tx, disconnect_rx) = async_channel::bounded(1);

        // Spawn a task for the client.
        let client = EventStreamClient {
            events: events_rx,
            disconnect: disconnect_rx,
            write: Box::new(write) as _,
        };
        let future = async move {
            if let Err(err) = handle_event_stream_client(client).await {
                warn!("error handling IPC event stream client: {err:?}");
            }
        };
        if let Err(err) = ctx.scheduler.schedule(future) {
            warn!("error scheduling IPC event stream future: {err:?}");
        }

        // Send the initial state.
        let window = ctx.focused_window.lock().unwrap().clone();
        let window = window.map(|window| {
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
        });
        events_tx.try_send(Event::WindowFocused { window }).unwrap();

        // Add it to the list.
        {
            let mut streams = ctx.event_streams.borrow_mut();
            let sender = EventStreamSender {
                events: events_tx,
                disconnect: disconnect_tx,
            };
            streams.push(sender);
        }
    }

    Ok(())
}

async fn process(ctx: &ClientCtx, request: Request) -> Reply {
    let response = match request {
        Request::ReturnError => return Err(String::from("example compositor error")),
        Request::Version => Response::Version(version()),
        Request::Outputs => {
            let ipc_outputs = ctx.ipc_outputs.lock().unwrap().clone();
            Response::Outputs(ipc_outputs)
        }
        Request::FocusedWindow => {
            let window = ctx.focused_window.lock().unwrap().clone();
            let window = window.map(|window| {
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
            });
            Response::FocusedWindow(window)
        }
        Request::Action(action) => {
            let (tx, rx) = async_channel::bounded(1);

            let action = niri_config::Action::from(action);
            ctx.event_loop.insert_idle(move |state| {
                state.do_action(action, false);
                let _ = tx.send_blocking(());
            });

            // Wait until the action has been processed before returning. This is important for a
            // few actions, for instance for DoScreenTransition this wait ensures that the screen
            // contents were sampled into the texture.
            let _ = rx.recv().await;
            Response::Handled
        }
        Request::Output { output, action } => {
            let ipc_outputs = ctx.ipc_outputs.lock().unwrap();
            let found = ipc_outputs
                .keys()
                .any(|name| name.eq_ignore_ascii_case(&output));
            let response = if found {
                OutputConfigChanged::Applied
            } else {
                OutputConfigChanged::OutputWasMissing
            };
            drop(ipc_outputs);

            ctx.event_loop.insert_idle(move |state| {
                state.apply_transient_output_config(&output, action);
            });

            Response::OutputConfigChanged(response)
        }
        Request::Workspaces => {
            let (tx, rx) = async_channel::bounded(1);
            ctx.event_loop.insert_idle(move |state| {
                let workspaces = state.niri.layout.ipc_workspaces();
                let _ = tx.send_blocking(workspaces);
            });
            let result = rx.recv().await;
            let workspaces = result.map_err(|_| String::from("error getting workspace info"))?;
            Response::Workspaces(workspaces)
        }
        Request::FocusedOutput => {
            let (tx, rx) = async_channel::bounded(1);
            ctx.event_loop.insert_idle(move |state| {
                let active_output = state
                    .niri
                    .layout
                    .active_output()
                    .map(|output| output.name());

                let output = active_output.and_then(|active_output| {
                    state
                        .backend
                        .ipc_outputs()
                        .lock()
                        .unwrap()
                        .get(&active_output)
                        .cloned()
                });

                let _ = tx.send_blocking(output);
            });
            let result = rx.recv().await;
            let output = result.map_err(|_| String::from("error getting active output info"))?;
            Response::FocusedOutput(output)
        }
        Request::EventStream => Response::Handled,
    };

    Ok(response)
}

async fn handle_event_stream_client(client: EventStreamClient) -> anyhow::Result<()> {
    let EventStreamClient {
        events,
        disconnect,
        mut write,
    } = client;

    while let Ok(event) = events.recv().await {
        let mut buf = serde_json::to_vec(&event).context("error formatting event")?;
        buf.push(b'\n');

        let res = select_biased! {
            _ = disconnect.recv().fuse() => return Ok(()),
            res = write.write_all(&buf).fuse() => res,
        };

        match res {
            Ok(()) => (),
            // Normal client disconnection.
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
            res @ Err(_) => res.context("error writing event")?,
        }
    }

    Ok(())
}
