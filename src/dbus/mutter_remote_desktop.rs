//! `org.gnome.Mutter.RemoteDesktop` implementation. `xdg-desktop-portal-gnome` implements the
//! Remote Desktop portal on top of this.

use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};

use bitflags::bitflags;
use enumflags2::BitFlags;
use serde::{Deserialize, Serialize};
use smithay::backend::input::{AxisSource, KeyState, Keycode};
use zbus::fdo::{self, RequestNameFlags};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{self, DeserializeDict, OwnedObjectPath, SerializeDict, Type};
use zbus::{interface, ObjectServer};

use super::Start;
use crate::input::remote_desktop_backend::{
    RdEventAdapter, RdKeyboardKeyEvent, RdPointerAxisEvent, RdPointerButtonEvent,
    RdPointerMotionAbsoluteEvent, RdPointerMotionEvent, RdInputBackend,
};

type InputEvent = smithay::backend::input::InputEvent<RdInputBackend>;

#[derive(Clone)]
pub struct RemoteDesktop {
    to_calloop: calloop::channel::Sender<RemoteDesktopDBusToCalloop>,
}

#[derive(Clone)]
pub struct Session {
    id: usize,
    id_str: String,
    to_calloop: calloop::channel::Sender<RemoteDesktopDBusToCalloop>,
    active: bool,
    attached_screen_cast_session: Option<()>,
    using_eis: bool,
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
    /// To a list of [`reis`] capabilities for exposing only portal-selected device capabilities
    pub fn to_reis_capabilities(flags: BitFlags<Self>) -> Vec<reis::event::DeviceCapability> {
        use reis::event::DeviceCapability;
        let mut vec = Vec::new();
        for flag in flags {
            match flag {
                MutterXdpDeviceType::Keyboard => vec.push(DeviceCapability::Keyboard),
                MutterXdpDeviceType::Pointer => {
                    vec.extend_from_slice(&[
                        DeviceCapability::Pointer,
                        DeviceCapability::Scroll,
                        DeviceCapability::Button,
                        DeviceCapability::PointerAbsolute,
                    ]);
                }
                MutterXdpDeviceType::Touchscreen => vec.push(DeviceCapability::Touch),
            }
        }
        vec
    }

    pub fn to_reis_capability_mask(flags: BitFlags<Self>) -> u64 {
        use reis::event::DeviceCapability;
        let mut mask = 0;
        for flag in flags {
            mask |= match flag {
                MutterXdpDeviceType::Keyboard => 2 << DeviceCapability::Keyboard as u64,
                MutterXdpDeviceType::Pointer => {
                    2 << DeviceCapability::Pointer as u64
                        | 2 << DeviceCapability::Scroll as u64
                        | 2 << DeviceCapability::Button as u64
                        | 2 << DeviceCapability::PointerAbsolute as u64
                }
                MutterXdpDeviceType::Touchscreen => 2 << DeviceCapability::Touch as u64,
            }
        }
        mask
    }
}

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
        let session = Session::new(session_id, self.to_calloop.clone());
        match server.at(&path, session.clone()).await {
            Ok(true) => {}
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
        debug!("RemoteDesktop.Start {}", self.id);

        if self.active {
            return Err(fdo::Error::Failed("Already started".to_owned()));
        }
        self.active = true;

        if let Some(_screen_cast_session) = self.attached_screen_cast_session {
            // TODO: start, close remote desktop session when screen cast session
            // closes
        }

        // TODO: if (self.eis) initialize_viewports

        // TODO: init_remote_access_handle

        Ok(())
    }

    // TODO: stop when EIS disconnects
    pub async fn stop(
        &mut self,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(signal_context)] ctxt: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        debug!("RemoteDesktop.Stop {}", self.id);

        if !self.active {
            return Err(fdo::Error::Failed("Session not started".to_owned()));
        }
        self.active = false;

        if let Some(_screen_cast_session) = self.attached_screen_cast_session {
            // TODO: stop
        }

        // TODO: make sure it can no longer be used after closing
        Self::closed(&ctxt).await.unwrap();

        server.remove::<Session, _>(ctxt.path()).await.unwrap();

        // TODO: Do we ever need to close the session object from elsewhere? In that case, we
        // should add a list of sessions in either the `RemoteDesktop` struct or the `Niri` struct

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

        if finish {
            // Niri detects axis stop based on this
            dx = 0.0;
            dy = 0.0;
        }

        let source = if flags.contains(MutterXdpPointerAxisFlags::SOURCE_WHEEL) {
            AxisSource::Wheel
        } else if flags.contains(MutterXdpPointerAxisFlags::SOURCE_FINGER) {
            AxisSource::Finger
        } else if flags.contains(MutterXdpPointerAxisFlags::SOURCE_CONTINUOUS) {
            AxisSource::Continuous
        } else {
            AxisSource::Wheel
        };

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
        _stream: &str,
        x: f64,
        y: f64,
    ) -> fdo::Result<()> {
        // FIXME: convert screencast-local coordinates to global coordinates?
        // NiriInputDevice::output?

        self.emulate_input(InputEvent::PointerMotionAbsolute {
            event: self.wrap_event(RdPointerMotionAbsoluteEvent { x, y }),
        });
        Ok(())
    }

    async fn notify_touch_down(
        &mut self,
        _stream: &str,
        _slot: u32,
        _x: f64,
        _y: f64,
    ) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_touch_motion(
        &mut self,
        _stream: &str,
        _slot: u32,
        _x: f64,
        _y: f64,
    ) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_touch_up(&mut self, _slot: u32) -> fdo::Result<()> {
        // TODO!
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

impl RemoteDesktop {
    pub fn new(to_calloop: calloop::channel::Sender<RemoteDesktopDBusToCalloop>) -> Self {
        Self { to_calloop }
    }
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

impl Session {
    pub fn new(
        id: usize,
        to_calloop: calloop::channel::Sender<RemoteDesktopDBusToCalloop>,
    ) -> Self {
        Self {
            id,
            id_str: id.to_string(),
            to_calloop,
            active: false,
            attached_screen_cast_session: None,
            using_eis: true,
        }
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
