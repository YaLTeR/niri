//! `org.gnome.Mutter.RemoteDesktop` implementation. `xdg-desktop-portal-gnome` implements the
//! Remote Desktop portal on top of this.

use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use zbus::fdo::{self, RequestNameFlags};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{self, DeserializeDict, OwnedObjectPath, SerializeDict, Type};
use zbus::{interface, ObjectServer};

use super::Start;

#[derive(Clone)]
pub struct RemoteDesktop {
    to_niri: calloop::channel::Sender<RemoteDesktopDBusToCalloop>,
}

#[derive(Clone)]
pub struct Session {
    id: usize,
    id_str: String,
    to_niri: calloop::channel::Sender<RemoteDesktopDBusToCalloop>,
    active: bool,
    attached_screen_cast_session: Option<()>,
}

#[derive(Debug, DeserializeDict, Type)]
#[zvariant(signature = "dict")]
struct ClipboardOptions {
    #[zvariant(rename = "mime-types")]
    mime_types: Option<Vec<String>>,
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
    /// Bitflags of device types to filter in EIS.
    /// Must be in `SupportedDeviceTypes` and is based on user's choice via portal
    #[zvariant(rename = "device-types")]
    device_types: Option<MutterXdpDeviceTypes>,
}

#[derive(Serialize, Deserialize, Debug, Type, PartialEq, Eq, Clone, Copy)]
pub struct MutterXdpDeviceTypes(u32);

bitflags! {
    impl MutterXdpDeviceTypes: u32 {
        const KEYBOARD = 1;
        const POINTER = 1 << 1;
        const TOUCHSCREEN = 1 << 2;
    }
}

impl MutterXdpDeviceTypes {
    pub fn has_smithay_capability(
        &self,
        capability: smithay::backend::input::DeviceCapability,
    ) -> bool {
        // TODO: actually unused since the caps exposed to smithay come are the ones the EIS client
        // selected
        use smithay::backend::input::DeviceCapability;
        match capability {
            DeviceCapability::Keyboard => self.contains(Self::KEYBOARD),
            DeviceCapability::Pointer => self.contains(Self::POINTER),
            DeviceCapability::Touch => self.contains(Self::TOUCHSCREEN),
            _ => false,
        }
    }
    /// To a list of [`reis`] capabilities for exposing only portal-selected device capabilities
    pub fn to_reis_capabilities(&self) -> Vec<reis::event::DeviceCapability> {
        use reis::event::DeviceCapability;
        self.iter()
            .filter_map(|flag| {
                Some(match flag {
                    MutterXdpDeviceTypes::KEYBOARD => DeviceCapability::Keyboard,
                    // TODO: pointerabsolute, scroll, etc.
                    MutterXdpDeviceTypes::POINTER => DeviceCapability::Pointer,
                    MutterXdpDeviceTypes::TOUCHSCREEN => DeviceCapability::Touch,
                    _ => return None,
                })
            })
            .collect()
    }
}

impl From<MutterXdpDeviceTypes> for zvariant::Value<'_> {
    fn from(value: MutterXdpDeviceTypes) -> Self {
        Self::U32(value.bits())
    }
}

pub enum RemoteDesktopDBusToCalloop {
    StopSession {
        session_id: usize,
    },
    NewEisContext {
        session_id: usize,
        ctx: reis::eis::Context,
        exposed_device_types: MutterXdpDeviceTypes,
    },
}

#[interface(name = "org.gnome.Mutter.RemoteDesktop")]
impl RemoteDesktop {
    async fn create_session(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        static NUMBER: AtomicUsize = AtomicUsize::new(0);
        let session_id = NUMBER.fetch_add(1, Ordering::SeqCst);
        let path = format!("/org/gnome/Mutter/RemoteDesktop/Session/u{}", session_id);
        let path = OwnedObjectPath::try_from(path).unwrap();

        let session = Session::new(session_id, self.to_niri.clone());
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
    async fn supported_device_types(&self) -> MutterXdpDeviceTypes {
        MutterXdpDeviceTypes::all()
    }

    #[zbus(property)]
    async fn version(&self) -> i32 {
        1
    }
}

#[interface(name = "org.gnome.Mutter.RemoteDesktop.Session")]
impl Session {
    #[zbus(property)]
    async fn session_id(&self) -> &str {
        &self.id_str
    }

    async fn start(&mut self) -> fdo::Result<()> {
        debug!("start");

        if self.active {
            return Err(fdo::Error::Failed("Already started".to_owned()));
        }

        if let Some(_screen_cast_session) = self.attached_screen_cast_session {
            // TODO: start, close remote desktop session when screen cast session
            // closes
        }

        // TODO: if (self.eis) initialize_viewports

        // TODO: init_remote_access_handle

        self.active = true;

        Ok(())
    }

    pub async fn stop(
        &mut self,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(signal_context)] ctxt: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        debug!("stop");

        if !self.active {
            return Err(fdo::Error::Failed("Session not started".to_owned()));
        }
        self.active = false;

        if let Some(_screen_cast_session) = self.attached_screen_cast_session {
            // TODO: stop
        }

        if let Err(err) = self.to_niri.send(RemoteDesktopDBusToCalloop::StopSession {
            session_id: self.id,
        }) {
            warn!("error sending StopSession to niri: {err:?}");
        }

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
        keycode: u32, // Evdev keycode
        state: bool,
    ) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_keyboard_keysym(&mut self, keysym: u32, state: bool) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_pointer_button(&mut self, button: i32, state: bool) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    /// Flags:
    /// 1: finish (scroll motion was finished)
    /// 2: source_wheel (scroll event by a mouse wheel)
    /// 4: source_finger (scroll event by one or more fingers (e.g. touchpads))
    /// 8: source_continuous (scroll event by the motion of some device)
    async fn notify_pointer_axis(&mut self, dx: f64, dy: f64, flags: u32) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_pointer_axis_discrete(&mut self, axis: u32, steps: i32) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_pointer_motion_relative(&mut self, dx: f64, dy: f64) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_pointer_motion_absolute(
        &mut self,
        stream: &str,
        x: f64,
        y: f64,
    ) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_touch_down(
        &mut self,
        stream: &str,
        slot: u32,
        x: f64,
        y: f64,
    ) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_touch_motion(
        &mut self,
        stream: &str,
        slot: u32,
        x: f64,
        y: f64,
    ) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn notify_touch_up(&mut self, slot: u32) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn enable_clipboard(&mut self, options: ClipboardOptions) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn disable_clipbard(&mut self) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn set_selection(&mut self, options: ClipboardOptions) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    /// Answer to the [`selection_transfer`] signal
    async fn selection_write(&mut self, serial: u32) -> fdo::Result<zvariant::OwnedFd> {
        // TODO!
        todo!()
    }

    /// Notifies that the transfer of clipboard data through the file descriptor returned in
    /// [`selection_write`] has either completed successfully, or failed
    async fn selection_write_done(&mut self, serial: u32, success: bool) -> fdo::Result<()> {
        // TODO!
        Ok(())
    }

    async fn selection_read(&mut self, mime_type: &str) -> fdo::Result<zvariant::OwnedFd> {
        // TODO!
        todo!()
    }

    // TODO: fdo::Result vs zbus::Result
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
    async fn caps_lock_state(&self) -> bool {
        todo!()
    }
    #[zbus(property)]
    async fn num_lock_state(&self) -> bool {
        todo!()
    }

    // TODO: make Notify* methods return errors once EIS is enabled (or is it done by xdp-gnome?)
    // TODO: screencast mapping_id and libei device region
    #[zbus(name = "ConnectToEIS")]
    async fn connect_to_eis(&self, options: ConnectToEisOptions) -> fdo::Result<zvariant::OwnedFd> {
        let (a, b) = UnixStream::pair().map_err(zbus::Error::from)?;
        let ctx = reis::eis::Context::new(a).map_err(zbus::Error::from)?;

        if let Err(err) = self.to_niri.send(RemoteDesktopDBusToCalloop::NewEisContext {
            session_id: self.id,
            ctx,
            exposed_device_types: options
                .device_types
                .unwrap_or_else(|| MutterXdpDeviceTypes::all()),
        }) {
            warn!("error sending NewEisContext to niri: {err:?}");
        }

        Ok(OwnedFd::from(b).into())
    }
}

impl RemoteDesktop {
    pub fn new(to_niri: calloop::channel::Sender<RemoteDesktopDBusToCalloop>) -> Self {
        Self { to_niri }
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
    pub fn new(id: usize, to_niri: calloop::channel::Sender<RemoteDesktopDBusToCalloop>) -> Self {
        Self {
            id,
            id_str: id.to_string(),
            to_niri,
            active: false,
            attached_screen_cast_session: None,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.to_niri.send(RemoteDesktopDBusToCalloop::StopSession {
            session_id: self.id,
        });
    }
}
