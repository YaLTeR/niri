use smithay::backend::input::{
    Device, DeviceCapability, Event, InputBackend, KeyState, KeyboardKeyEvent, Keycode, UnusedEvent,
};
use smithay::output::Output;

use crate::input::backend_ext::NiriInputDevice;
use crate::niri::State;

pub struct RemoteDesktopInputBackend;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct RemoteDesktopVirtualDevice {
    /// Device ID unique to the remote desktop session
    device_id: usize,
    /// Remote desktop session ID in `Niri` struct
    session_id: usize,
}

impl InputBackend for RemoteDesktopInputBackend {
    type Device = RemoteDesktopVirtualDevice;

    type KeyboardKeyEvent = EisEventAdapter<reis::request::KeyboardKey, PressedCount>;

    type PointerAxisEvent = UnusedEvent;
    type PointerButtonEvent = UnusedEvent;
    type PointerMotionEvent = UnusedEvent;
    type PointerMotionAbsoluteEvent = UnusedEvent;

    type GestureSwipeBeginEvent = UnusedEvent;
    type GestureSwipeUpdateEvent = UnusedEvent;
    type GestureSwipeEndEvent = UnusedEvent;
    type GesturePinchBeginEvent = UnusedEvent;
    type GesturePinchUpdateEvent = UnusedEvent;
    type GesturePinchEndEvent = UnusedEvent;
    type GestureHoldBeginEvent = UnusedEvent;
    type GestureHoldEndEvent = UnusedEvent;

    type TouchDownEvent = UnusedEvent;
    type TouchUpEvent = UnusedEvent;
    type TouchMotionEvent = UnusedEvent;
    type TouchCancelEvent = UnusedEvent;
    type TouchFrameEvent = UnusedEvent;
    type TabletToolAxisEvent = UnusedEvent;
    type TabletToolProximityEvent = UnusedEvent;
    type TabletToolTipEvent = UnusedEvent;
    type TabletToolButtonEvent = UnusedEvent;

    type SwitchToggleEvent = UnusedEvent;

    type SpecialEvent = UnusedEvent;
}

impl Device for RemoteDesktopVirtualDevice {
    fn id(&self) -> String {
        format!(
            "Remote desktop virtual device {}/{}",
            self.session_id, self.device_id
        )
    }

    fn name(&self) -> String {
        String::from("Remote desktop virtual device")
    }

    fn has_capability(&self, capability: DeviceCapability) -> bool {
        // TODO: only actual EIS selected capabilities?
        matches!(
            capability,
            DeviceCapability::Keyboard | DeviceCapability::Pointer | DeviceCapability::Touch
        )
    }

    fn usb_id(&self) -> Option<(u32, u32)> {
        None
    }

    fn syspath(&self) -> Option<std::path::PathBuf> {
        None
    }
}

impl NiriInputDevice for RemoteDesktopVirtualDevice {
    fn output(&self, _state: &State) -> Option<Output> {
        // TODO: Per-output
        None
        //self.output().cloned()
    }
}

/// Wrapper to implement [`Event`] automatically, hold extra data and avoid Rust's orphan rule
pub struct EisEventAdapter<Ev, Extra = ()> {
    pub device: RemoteDesktopVirtualDevice,
    pub inner: Ev,
    pub extra: Extra,
}

impl<Ev: reis::request::EventTime, Extra> Event<RemoteDesktopInputBackend>
    for EisEventAdapter<Ev, Extra>
{
    fn time(&self) -> u64 {
        self.inner.time()
    }

    fn device(&self) -> <RemoteDesktopInputBackend as InputBackend>::Device {
        self.device.clone()
    }
}

/// Extra passed to the keyboard key event containing the number of keys pressed on all devices in
/// the seat.
pub struct PressedCount(pub u32);

impl KeyboardKeyEvent<RemoteDesktopInputBackend>
    for EisEventAdapter<reis::request::KeyboardKey, PressedCount>
{
    fn key_code(&self) -> Keycode {
        Keycode::new(self.inner.key)
    }

    fn state(&self) -> KeyState {
        match self.inner.state {
            reis::ei::keyboard::KeyState::Released => KeyState::Released,
            reis::ei::keyboard::KeyState::Press => KeyState::Pressed,
        }
    }

    fn count(&self) -> u32 {
        self.extra.0
    }
}
