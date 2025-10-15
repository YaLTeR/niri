//! `org.gnome.Mutter.RemoteDesktop` implementation. `xdg-desktop-portal-gnome` implements the
//! Remote Desktop portal on top of this.

use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "xdp-gnome-screencast")]
use std::sync::Arc;

use bitflags::bitflags;
use enumflags2::BitFlags;
#[cfg(feature = "xdp-gnome-screencast")]
use futures_util::lock::Mutex;
use serde::{Deserialize, Serialize};
use smithay::backend::input::{AxisSource, KeyState, Keycode};
use smithay::utils::{Logical, Point, Rectangle, Size};
use zbus::fdo::{self, RequestNameFlags};
#[cfg(feature = "xdp-gnome-screencast")]
use zbus::object_server::InterfaceRef;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{self, DeserializeDict, OwnedObjectPath, SerializeDict, Type};
use zbus::{interface, ObjectServer};

use super::Start;
use crate::backend::IpcOutputMap;
use crate::input::remote_desktop_backend::{
    RdAbsolutePosition, RdEventAdapter, RdInputBackend, RdKeyboardKeyEvent, RdPointerAxisEvent,
    RdPointerButtonEvent, RdPointerMotionAbsoluteEvent, RdPointerMotionEvent, RdTouchEvent,
    UnitIntervalPointKind,
};

#[cfg(feature = "xdp-gnome-screencast")]
pub(super) mod shared {
    use std::collections::HashMap;
    use std::sync::Arc;

    use futures_util::lock::Mutex;
    use zbus::object_server::InterfaceRef;

    /// Data shared between `org.gnome.Mutter.ScreenCast` and `org.gnome.Mutter.RemoteDesktop`
    #[derive(Default)]
    pub struct RemoteDesktopShared {
        pub(in super::super) sessions: HashMap<usize, InterfaceRef<super::Session>>,
    }
    impl RemoteDesktopShared {
        pub fn new_arc_mutex() -> Arc<Mutex<Self>> {
            Arc::new(Mutex::new(Self::default()))
        }
    }
}

type InputEvent = smithay::backend::input::InputEvent<RdInputBackend>;

pub enum RemoteDesktopDBusToCalloop {
    RemoveEisHandler {
        session_id: usize,
    },
    NewEisContext {
        session_id: usize,
        ctx: reis::eis::Context,
        exposed_device_types: BitFlags<MutterXdpDeviceType>,
    },
    EmulateInput(InputEvent),
    EmulateKeysym {
        /// X11 keysym, like in the `xkeysym` crate
        keysym: u32,
        state: KeyState,
        session_id: usize,
        time: u64,
    },
}

// == MAIN INTERFACE ==

/// D-Bus object for the remote desktop portal's implementation
pub(super) struct RemoteDesktop {
    pub(super) to_calloop: calloop::channel::Sender<RemoteDesktopDBusToCalloop>,
    #[cfg(feature = "xdp-gnome-screencast")]
    pub(super) shared: Arc<Mutex<shared::RemoteDesktopShared>>,
}

impl Start for RemoteDesktop {
    fn start(self) -> anyhow::Result<zbus::blocking::Connection> {
        let conn = zbus::blocking::Connection::session()?;
        let flags = RequestNameFlags::AllowReplacement
            | RequestNameFlags::ReplaceExisting
            | RequestNameFlags::DoNotQueue;

        conn.object_server()
            .at("/org/gnome/Mutter/RemoteDesktop", self)?;
        conn.request_name_with_flags("org.gnome.Mutter.RemoteDesktop", flags)?;

        Ok(conn)
    }
}

#[interface(
    name = "org.gnome.Mutter.RemoteDesktop",
    spawn = false,
    introspection_docs = false
)]
impl RemoteDesktop {
    async fn create_session(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        static NUMBER: AtomicUsize = AtomicUsize::new(0);

        let session_id = NUMBER.fetch_add(1, Ordering::SeqCst);
        let path = format!("/org/gnome/Mutter/RemoteDesktop/Session/u{}", session_id);
        let path = OwnedObjectPath::try_from(path).unwrap();

        debug!("Created new RemoteDesktop.Session with ID {}", session_id);

        let session = Session {
            id: session_id,
            id_str: session_id.to_string(),
            to_calloop: self.to_calloop.clone(),
            active: false,
            #[cfg(feature = "xdp-gnome-screencast")]
            screen_cast_session: None,
            using_eis: false,
        };

        match server.at(&path, session).await {
            Ok(true) => {
                #[cfg(feature = "xdp-gnome-screencast")]
                {
                    let iface = server.interface(&path).await.unwrap();
                    self.shared.lock().await.sessions.insert(session_id, iface);
                }
            }
            Ok(false) => return Err(fdo::Error::Failed("session path already exists".to_owned())),
            Err(err) => {
                return Err(fdo::Error::Failed(format!(
                    "error creating session object: {err:?}"
                )))
            }
        }

        Ok(path)
    }

    /// Bitmask of supported device types
    #[zbus(property)]
    async fn supported_device_types(&self) -> u32 {
        BitFlags::<MutterXdpDeviceType>::all().bits()
    }

    #[zbus(property)]
    async fn version(&self) -> i32 {
        1
    }
}

// == SESSION ==

/// D-Bus object for a remote desktop session
pub(super) struct Session {
    id: usize,
    id_str: String,
    to_calloop: calloop::channel::Sender<RemoteDesktopDBusToCalloop>,
    pub active: bool,
    using_eis: bool,
    #[cfg(feature = "xdp-gnome-screencast")]
    pub screen_cast_session: Option<(
        InterfaceRef<super::mutter_screen_cast::Session>,
        ObjectServer,
    )>,
}

impl Session {
    fn emulate_input(&self, event: InputEvent) {
        if let Err(err) = self
            .to_calloop
            .send(RemoteDesktopDBusToCalloop::EmulateInput(event))
        {
            warn!("error sending EmulateInput to niri: {err:?}");
        }
    }
    fn wrap_event<Ev>(&self, inner: Ev) -> RdEventAdapter<Ev> {
        RdEventAdapter {
            session_id: self.id,
            time: Self::time_now(),
            inner,
        }
    }
    fn time_now() -> u64 {
        // TODO: do we really need CLOCK_MONOTONIC?
        0
    }

    /// - `stream_path`: D-Bus object path like `/org/gnome/Mutter/ScreenCast/Stream/u7`
    async fn convert_stream_coordinate_space(
        &self,
        stream_path: &str,
        x: f64,
        y: f64,
    ) -> fdo::Result<Point<f64, UnitIntervalPointKind>> {
        fn global_bounding_rectangle(
            ipc_outputs: &IpcOutputMap,
        ) -> Option<Rectangle<i32, Logical>> {
            ipc_outputs
                .values()
                .filter_map(|output| output.logical)
                .fold(None, |acc, l| {
                    let geo = Rectangle::new(
                        Point::new(l.x, l.y),
                        Size::new(l.width as i32, l.height as i32),
                    );
                    Some(acc.map_or(geo, |acc| acc.merge(geo)))
                })
        }

        let in_point = Point::new(x, y);

        let Some((screen_cast_iface, screen_cast_object_server)) = &self.screen_cast_session else {
            return Err(fdo::Error::Failed(
                "Must have screencast session for absolute coordinates".to_owned(),
            ));
        };

        let Ok(stream) = screen_cast_object_server
            .interface::<_, super::mutter_screen_cast::Stream>(stream_path)
            .await
        else {
            return Err(fdo::Error::Failed("Unknown stream".to_owned()));
        };

        let stream_position = {
            let params = stream.get().await.parameters();
            Point::new(params.position.0, params.position.1)
        };

        let Some(output_geo) = ({
            let screen_cast_state = screen_cast_iface.get().await;
            let ipc_outputs = screen_cast_state.ipc_outputs.lock().unwrap();
            global_bounding_rectangle(&ipc_outputs)
        }) else {
            return Err(fdo::Error::Failed("Missing outputs for getting global bounding rectangle".to_owned()));
        };

        let out_point = (stream_position.to_f64() + in_point - output_geo.loc.to_f64()).to_size()
            / output_geo.size.to_f64();
        let out_point = Point::new(out_point.x, out_point.y);

        debug!(stream_path, ?in_point, ?out_point, ?output_geo, ?stream_position);

        Ok(out_point)
    }

    /// Stops the session.
    pub async fn stop(&mut self, server: &ObjectServer, ctxt: &SignalEmitter<'_>) {
        if !self.active {
            return;
        }
        self.active = false;

        #[cfg(feature = "xdp-gnome-screencast")]
        if let Some((iface, server)) = &self.screen_cast_session {
            iface
                .get()
                .await
                .stop_no_remote_desktop(server, iface.signal_emitter())
                .await;
        }

        Self::closed(ctxt).await.unwrap();

        server.remove::<Session, _>(ctxt.path()).await.unwrap();

        // TODO: Do we ever need to close the session object from elsewhere? In that case, we
        // should add a list of sessions in either the `RemoteDesktop` struct or the `Niri` struct
    }
}

#[derive(Debug, DeserializeDict, Type)]
#[zvariant(signature = "dict")]
struct ClipboardOptions {
    #[zvariant(rename = "mime-types")]
    _mime_types: Option<Vec<String>>,
}

#[derive(Debug, SerializeDict, Type)]
#[zvariant(signature = "dict")]
struct SelectionOwnerChangedOptions {
    #[zvariant(rename = "mime-types")]
    mime_types: Option<Vec<String>>,
    #[zvariant(rename = "session-is-owner")]
    session_is_owner: Option<bool>,
}

#[derive(Debug, DeserializeDict, Type)]
#[zvariant(signature = "dict")]
struct ConnectToEisOptions {
    /// Bitflags of device types to expose and filter in EIS.
    ///
    /// Must be in `SupportedDeviceTypes` and is based on user's choice via portal
    #[zvariant(rename = "device-types")]
    device_types: Option<BitFlags<MutterXdpDeviceType>>,
}
#[derive(Serialize, Deserialize, Debug, Type, PartialEq, Eq, Clone, Copy)]
pub struct MutterXdpPointerAxisFlags(u32);

bitflags! {
    impl MutterXdpPointerAxisFlags: u32 {
        /// Note: only this is currently provided by xdp-gnome
        const FINISH = 1;
        const SOURCE_WHEEL = 1 << 1;
        const SOURCE_FINGER = 1 << 2;
        const SOURCE_CONTINUOUS = 1 << 3;
    }
}

#[derive(Serialize, Deserialize, Debug, Type, PartialEq, Eq, Clone, Copy)]
#[enumflags2::bitflags]
#[repr(u32)]
pub enum MutterXdpDeviceType {
    Keyboard = 1,
    Pointer = 1 << 1,
    Touchscreen = 1 << 2,
}

impl MutterXdpDeviceType {
    /// To [`reis`] capabilities for exposing only portal-selected device capabilities
    pub fn to_reis_capabilities(flags: BitFlags<Self>) -> BitFlags<reis::event::DeviceCapability> {
        use reis::event::DeviceCapability;
        let mut out_flags = BitFlags::empty();
        for flag in flags {
            match flag {
                MutterXdpDeviceType::Keyboard => out_flags |= DeviceCapability::Keyboard,
                MutterXdpDeviceType::Pointer => {
                    out_flags |= DeviceCapability::Pointer
                        | DeviceCapability::Scroll
                        | DeviceCapability::Button
                        | DeviceCapability::PointerAbsolute
                }
                MutterXdpDeviceType::Touchscreen => out_flags |= DeviceCapability::Touch,
            }
        }
        out_flags
    }
}

#[interface(
    name = "org.gnome.Mutter.RemoteDesktop.Session",
    spawn = false,
    introspection_docs = false
)]
impl Session {
    #[zbus(property)]
    async fn session_id(&self) -> &str {
        &self.id_str
    }

    async fn start(&mut self) -> fdo::Result<()> {
        debug!("RemoteDesktop.Start id={}", self.id);

        if self.active {
            return Err(fdo::Error::Failed("Already started".to_owned()));
        }
        self.active = true;

        #[cfg(feature = "xdp-gnome-screencast")]
        if let Some((iface, _server)) = &self.screen_cast_session {
            iface.get().await.start().await;
            debug!("RemoteDesktop.Start started screencast");
        }

        // TODO: if (self.eis) initialize_viewports

        // TODO: init_remote_access_handle

        Ok(())
    }

    // TODO: stop when EIS disconnects
    #[zbus(name = "Stop")]
    pub async fn stop_dbus(
        &mut self,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(signal_context)] ctxt: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        debug!("RemoteDesktop.Stop id={}", self.id);

        if !self.active {
            return Err(fdo::Error::Failed("Session not started".to_owned()));
        }

        self.stop(server, &ctxt).await;

        Ok(())
    }

    /// "A session doesn't have to have been started before it may be closed. After it being
    /// closed, it can no longer be used."
    #[zbus(signal)]
    async fn closed(ctxt: &SignalEmitter<'_>) -> zbus::Result<()>;

    async fn notify_keyboard_keycode(
        &mut self,
        keycode: u32,
        state_is_pressed: bool,
    ) -> fdo::Result<()> {
        self.emulate_input(InputEvent::Keyboard {
            event: self.wrap_event(RdKeyboardKeyEvent {
                // Offset from evdev keycodes (where KEY_ESCAPE is 1) to X11 keycodes
                keycode: Keycode::new(keycode + 8),
                state: if state_is_pressed {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                },
            }),
        });
        Ok(())
    }

    async fn notify_keyboard_keysym(
        &mut self,
        keysym: u32,
        state_is_pressed: bool,
    ) -> fdo::Result<()> {
        if let Err(err) = self
            .to_calloop
            .send(RemoteDesktopDBusToCalloop::EmulateKeysym {
                keysym,
                state: if state_is_pressed {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                },
                session_id: self.id,
                time: Self::time_now(),
            })
        {
            warn!("error sending EmulateKeysym to niri: {err:?}");
        }
        Ok(())
    }

    async fn notify_pointer_button(&mut self, button: i32, state: bool) -> fdo::Result<()> {
        self.emulate_input(InputEvent::PointerButton {
            event: self.wrap_event(RdPointerButtonEvent { button, state }),
        });
        Ok(())
    }

    async fn notify_pointer_axis(
        &mut self,
        mut dx: f64,
        mut dy: f64,
        flags: MutterXdpPointerAxisFlags,
    ) -> fdo::Result<()> {
        let finish = flags.contains(MutterXdpPointerAxisFlags::FINISH);

        let source = if flags.contains(MutterXdpPointerAxisFlags::SOURCE_WHEEL) {
            AxisSource::Wheel
        } else if flags.contains(MutterXdpPointerAxisFlags::SOURCE_FINGER) {
            AxisSource::Finger
        } else if flags.contains(MutterXdpPointerAxisFlags::SOURCE_CONTINUOUS) {
            AxisSource::Continuous
        } else {
            AxisSource::Wheel
        };

        if finish && source == AxisSource::Finger {
            // Niri detects axis stop based on this
            dx = 0.0;
            dy = 0.0;
        }

        self.emulate_input(InputEvent::PointerAxis {
            event: self.wrap_event(RdPointerAxisEvent {
                source,
                discrete: None,
                delta: Some((dx, dy)),
            }),
        });

        Ok(())
    }

    async fn notify_pointer_axis_discrete(&mut self, axis: u32, steps: i32) -> fdo::Result<()> {
        debug!(axis, steps);
        self.emulate_input(InputEvent::PointerAxis {
            event: self.wrap_event(RdPointerAxisEvent {
                source: AxisSource::Wheel,
                delta: None,
                discrete: Some(match axis {
                    0 => (0, steps),
                    _ => (steps, 0),
                }),
            }),
        });

        Ok(())
    }

    async fn notify_pointer_motion_relative(&mut self, dx: f64, dy: f64) -> fdo::Result<()> {
        self.emulate_input(InputEvent::PointerMotion {
            event: self.wrap_event(RdPointerMotionEvent { dx, dy }),
        });
        Ok(())
    }

    async fn notify_pointer_motion_absolute(
        &mut self,
        stream: &str,
        x: f64,
        y: f64,
    ) -> fdo::Result<()> {
        let pos = self.convert_stream_coordinate_space(stream, x, y).await?;

        self.emulate_input(InputEvent::PointerMotionAbsolute {
            event: self.wrap_event(RdPointerMotionAbsoluteEvent(RdAbsolutePosition { pos })),
        });
        Ok(())
    }

    async fn notify_touch_down(
        &mut self,
        stream: &str,
        slot: u32,
        x: f64,
        y: f64,
    ) -> fdo::Result<()> {
        let pos = self.convert_stream_coordinate_space(stream, x, y).await?;

        self.emulate_input(InputEvent::TouchDown {
            event: self.wrap_event(RdTouchEvent {
                slot,
                extra: RdAbsolutePosition { pos },
            }),
        });
        Ok(())
    }

    async fn notify_touch_motion(
        &mut self,
        stream: &str,
        slot: u32,
        x: f64,
        y: f64,
    ) -> fdo::Result<()> {
        let pos = self.convert_stream_coordinate_space(stream, x, y).await?;

        self.emulate_input(InputEvent::TouchMotion {
            event: self.wrap_event(RdTouchEvent {
                slot,
                extra: RdAbsolutePosition { pos },
            }),
        });
        Ok(())
    }

    async fn notify_touch_up(&mut self, slot: u32) -> fdo::Result<()> {
        self.emulate_input(InputEvent::TouchUp {
            event: self.wrap_event(RdTouchEvent { slot, extra: () }),
        });
        Ok(())
    }

    /// Enables calling *Selection* and DisableClipboard
    async fn enable_clipboard(&mut self, _options: ClipboardOptions) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn disable_clipbard(&mut self) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn set_selection(&mut self, _options: ClipboardOptions) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    /// Answer to the [`selection_transfer`] signal
    async fn selection_write(&mut self, _serial: u32) -> fdo::Result<zvariant::OwnedFd> {
        // TODO!
        todo!()
    }

    /// Notifies that the transfer of clipboard data through the file descriptor returned in
    /// [`selection_write`] has either completed successfully, or failed
    async fn selection_write_done(&mut self, _serial: u32, _successs: bool) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn selection_read(&mut self, _mime_type: &str) -> fdo::Result<zvariant::OwnedFd> {
        // TODO!
        todo!()
    }

    #[zbus(signal)]
    async fn selection_owner_changed(
        ctxt: &SignalEmitter<'_>,
        options: SelectionOwnerChangedOptions,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn selection_transfer(
        ctxt: &SignalEmitter<'_>,
        mime_type: &str,
        serial: u32,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    async fn caps_lock_state(&self) -> fdo::Result<bool> {
        Err(fdo::Error::Failed("CapsLockState is deprecated and not used by xdg-desktop-portal-gnome. Because of that it's not implemented by Niri.".to_owned()))
    }
    #[zbus(property)]
    async fn num_lock_state(&self) -> fdo::Result<bool> {
        Err(fdo::Error::Failed("NumLockState is deprecated and not used by xdg-desktop-portal-gnome. Because of that it's not implemented by Niri.".to_owned()))
    }

    // TODO: make Notify* methods return errors once EIS is enabled (or is it done by xdp-gnome?)
    // TODO: screencast mapping_id and libei device region
    // TODO: Start and ConnectToEIS interactions?
    #[zbus(name = "ConnectToEIS")]
    async fn connect_to_eis(
        &mut self,
        options: ConnectToEisOptions,
    ) -> fdo::Result<zvariant::OwnedFd> {
        // TODO: xdp RemoteDesktop portal API specifies the below requirements, but what
        // does xdp-gnome do?

        // Mutter supports calling meta_eis_add_client_get_fd multiple times and also
        // checks for EIS existence in the Start handler.

        if !self.active {
            return Err(fdo::Error::Failed("Session not started".to_owned()));
        }
        if self.using_eis {
            return Err(fdo::Error::Failed("Already gave EIS socket".to_owned()));
        }
        self.using_eis = true;

        let (a, b) = UnixStream::pair().map_err(zbus::Error::from)?;
        let ctx = reis::eis::Context::new(a).map_err(zbus::Error::from)?;

        debug!("RemoteDesktop.ConnectToEIS");

        if let Err(err) = self
            .to_calloop
            .send(RemoteDesktopDBusToCalloop::NewEisContext {
                session_id: self.id,
                ctx,
                exposed_device_types: options.device_types.unwrap_or_else(BitFlags::all),
            })
        {
            warn!("error sending NewEisContext to niri: {err:?}");
        }

        Ok(OwnedFd::from(b).into())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.using_eis {
            let _ = self
                .to_calloop
                .send(RemoteDesktopDBusToCalloop::RemoveEisHandler {
                    session_id: self.id,
                });
        }
    }
}
