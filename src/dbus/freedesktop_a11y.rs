use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Context;
use futures_util::StreamExt;
use smithay::input::keyboard::{xkb, KeysymHandle};
use zbus::blocking::object_server::InterfaceRef;
use zbus::fdo::{self, RequestNameFlags};
use zbus::interface;
use zbus::message::Header;
use zbus::names::{BusName, OwnedUniqueName, UniqueName};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::NoneValue;

use super::Start;

#[derive(Debug, Default)]
struct ClientState {
    watched: bool,
    grabbed: bool,
    modifiers: Vec<u32>,
    keystrokes: Vec<(u32, u32)>,
}

#[derive(Clone)]
pub struct KeyboardMonitor {
    clients: Arc<Mutex<HashMap<OwnedUniqueName, ClientState>>>,
    iface: Arc<OnceLock<InterfaceRef<Self>>>,
}

/// Interface for monitoring of keyboard input by assistive technologies.
///
/// This interface is used by assistive technologies to monitor keyboard input of the compositor.
/// The compositor is expected to listen on the well-known bus name "org.freedesktop.a11y.Manager"
/// at the object path "/org/freedesktop/a11y/Manager".
#[interface(name = "org.freedesktop.a11y.KeyboardMonitor")]
impl KeyboardMonitor {
    // Starts grabbing all key events. The client receives the events through the KeyEvent signal,
    // and in addition, the events aren't handled normally by the compositor. This includes changes
    // to the state of toggles like Caps Lock, Num Lock, and Scroll Lock.
    //
    // This behavior stays in effect until the same client calls UngrabKeyboard or closes its D-Bus
    // connection.
    async fn grab_keyboard(&self, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<()> {
        let Some(sender) = hdr.sender() else {
            return Err(fdo::Error::Failed("no sender".to_owned()));
        };
        let sender = OwnedUniqueName::from(sender.to_owned());
        debug!("enabling keyboard grab for {sender}");

        let mut clients = self.clients.lock().unwrap();
        let client = clients.entry(sender).or_default();
        client.grabbed = true;

        Ok(())
    }

    // Reverses the effect of calling GrabKeyboard. If GrabKeyboard wasn't previously called, this
    // method does nothing.
    //
    // After calling this method, the key grabs specified in the last call to SetKeyGrabs, if any,
    // are still in effect. Also, the client will still receive key events through the KeyEvent
    // signal, if it has called WatchKeyboard.
    async fn ungrab_keyboard(&self, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<()> {
        let Some(sender) = hdr.sender() else {
            return Err(fdo::Error::Failed("no sender".to_owned()));
        };
        let sender = OwnedUniqueName::from(sender.to_owned());

        let mut clients = self.clients.lock().unwrap();
        if let Some(client) = clients.get_mut(&sender) {
            debug!("disabling keyboard grab for {sender}");
            client.grabbed = false;
        }

        Ok(())
    }

    // Starts watching all key events. The client receives the events through the KeyEvent signal,
    // but the events are still handled normally by the compositor. This includes changes to the
    // state of toggles like Caps Lock, Num Lock, and Scroll Lock.
    //
    // This behavior stays in effect until the same client calls UnwatchKeyboard or closes its D-Bus
    // connection.
    async fn watch_keyboard(&self, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<()> {
        let Some(sender) = hdr.sender() else {
            return Err(fdo::Error::Failed("no sender".to_owned()));
        };
        let sender = OwnedUniqueName::from(sender.to_owned());
        debug!("enabling keyboard watch for {sender}");

        let mut clients = self.clients.lock().unwrap();
        let client = clients.entry(sender).or_default();
        client.watched = true;

        Ok(())
    }

    // Reverses the effect of calling WatchKeyboard. If WatchKeyboard wasn't previously called, this
    // method does nothing.
    //
    // After calling this method, the key grabs specified in the last call to SetKeyGrabs, if any,
    // are still in effect, but other key events are no longer reported to this client.
    async fn unwatch_keyboard(&self, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<()> {
        let Some(sender) = hdr.sender() else {
            return Err(fdo::Error::Failed("no sender".to_owned()));
        };
        let sender = OwnedUniqueName::from(sender.to_owned());

        let mut clients = self.clients.lock().unwrap();
        if let Some(client) = clients.get_mut(&sender) {
            debug!("disabling keyboard watch for {sender}");
            client.watched = false;
        }

        Ok(())
    }

    // Sets the current key grabs for the calling client, overriding any previous call to this
    // method. For grabbed key events, the KeyEvent signal is emitted, and normal key event handling
    // is suppressed, including state changes for toggles like Caps Lock and Num Lock.
    //
    // The grabs set by this method stay in effect until the same client calls this method again, or
    // until that client closes its D-Bus connection.
    //
    // Each item in `modifiers` is an XKB keysym. All keys in this list will be grabbed, and keys
    // pressed while any of these keys are down will also be grabbed.
    //
    // Each item in `keystrokes` is a struct with the following fields:
    //
    // - the XKB keysym of the non-modifier key
    // - the XKB modifier mask of the modifiers, if any, for this keystroke
    //
    // If any of the keys in `modifiers` is pressed alone, the compositor is required to ignore the
    // key press and release event if a second key press of the same modifier is not received within
    // a reasonable time frame, for example, the key repeat delay. If such event is received, this
    // second event is processed normally.
    async fn set_key_grabs(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        modifiers: Vec<u32>,
        keystrokes: Vec<(u32, u32)>,
    ) -> fdo::Result<()> {
        let Some(sender) = hdr.sender() else {
            return Err(fdo::Error::Failed("no sender".to_owned()));
        };
        let sender = OwnedUniqueName::from(sender.to_owned());
        debug!("updating key grabs for {sender}");

        let mut clients = self.clients.lock().unwrap();
        let client = clients.entry(sender).or_default();
        client.modifiers = modifiers;
        client.keystrokes = keystrokes;

        Ok(())
    }

    // The compositor emits this signal for each key press or release.
    //
    // - `released`: whether this is a key-up event
    // - `state`: XKB modifier mask for currently pressed modifiers
    // - `keysym`: XKB keysym for this key
    // - `unichar`: Unicode character for this key, or 0 if none
    // - `keycode`: hardware-dependent keycode for this key
    #[zbus(signal)]
    pub async fn key_event(
        ctxt: &SignalEmitter<'_>,
        released: bool,
        state: u32,
        keysym: u32,
        unichar: u32,
        keycode: u16,
    ) -> zbus::Result<()>;
}

impl KeyboardMonitor {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            iface: Arc::new(OnceLock::new()),
        }
    }

    pub fn process_key(&self, pressed: bool, keysym: &KeysymHandle) -> bool {
        let _span = tracy_client::span!("KeyboardMonitor::process_key");

        let mut ctxt = self.iface.get().unwrap().signal_emitter().clone();
        let released = !pressed;

        let (mods, keysym, unichar, keycode) = {
            let modified = keysym.modified_sym();
            let keycode = keysym.raw_code();

            let xkb = keysym.xkb().lock().unwrap();
            // SAFETY: we're not changing the ref count.
            let state = unsafe { xkb.state() };

            let mods = state.serialize_mods(xkb::STATE_MODS_EFFECTIVE);
            let unichar = state.key_get_utf32(keycode);

            (mods, modified.raw(), unichar, keycode.raw() as u16)
        };

        let clients = self.clients.lock().unwrap();

        for (name, state) in &*clients {
            // TODO: grabs, double modifier, repeat (?)
            if state.grabbed || state.watched {
                // Emit to that client only.
                ctxt = ctxt.set_destination(BusName::Unique(name.as_ref()));
                let ctxt = &ctxt;
                async_io::block_on(async move {
                    if let Err(err) =
                        KeyboardMonitor::key_event(ctxt, released, mods, keysym, unichar, keycode)
                            .await
                    {
                        warn!("error emitting key_event: {err:?}");
                    }
                });
            }
        }

        false
    }
}

async fn monitor_disappeared_clients(
    conn: &zbus::Connection,
    clients: Arc<Mutex<HashMap<OwnedUniqueName, ClientState>>>,
) -> anyhow::Result<()> {
    let proxy = fdo::DBusProxy::new(conn)
        .await
        .context("error creating a DBusProxy")?;

    let mut stream = proxy
        .receive_name_owner_changed_with_args(&[(2, UniqueName::null_value())])
        .await
        .context("error creating a NameOwnerChanged stream")?;

    while let Some(signal) = stream.next().await {
        let args = signal
            .args()
            .context("error retrieving NameOwnerChanged args")?;

        let Some(name) = &**args.old_owner() else {
            continue;
        };

        if args.new_owner().is_none() {
            debug!("keyboard monitor client disconnected: {name}");

            let name = OwnedUniqueName::from(name.to_owned());
            let mut clients = clients.lock().unwrap();
            clients.remove(&name);
        } else {
            error!("non-null new_owner should've been filtered out");
        }
    }

    Ok(())
}

impl Start for KeyboardMonitor {
    fn start(self) -> anyhow::Result<zbus::blocking::Connection> {
        let clients = self.clients.clone();

        let conn = zbus::blocking::Connection::session()?;
        let flags = RequestNameFlags::AllowReplacement
            | RequestNameFlags::ReplaceExisting
            | RequestNameFlags::DoNotQueue;

        conn.object_server()
            .at("/org/freedesktop/a11y/Manager", self.clone())?;
        conn.request_name_with_flags("org.freedesktop.a11y.Manager", flags)?;

        let iface = conn
            .object_server()
            .interface("/org/freedesktop/a11y/Manager")?;
        let _ = self.iface.set(iface);

        let async_conn = conn.inner().clone();
        let future = async move {
            if let Err(err) = monitor_disappeared_clients(&async_conn, clients.clone()).await {
                warn!("error monitoring keyboard monitor clients: {err:?}");

                // Since the monitor is now broken, prevent any further communication.
                if let Err(err) = async_conn.close().await {
                    warn!("error closing connection: {err:?}");
                }

                clients.lock().unwrap().clear();
            }
        };
        let task = conn
            .inner()
            .executor()
            .spawn(future, "monitor disappearing keyboard clients");
        task.detach();

        Ok(conn)
    }
}
