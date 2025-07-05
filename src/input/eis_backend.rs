use smithay::backend::input::{self as traits, Device, DeviceCapability, InputBackend, UnusedEvent};
use smithay::output::Output;

use crate::input::backend_ext::NiriInputDevice;
use crate::niri::State;

pub struct EisInputBackend;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct EisVirtualDevice {
    id: usize,
}

impl InputBackend for EisInputBackend {
    type Device = EisVirtualDevice;

    type KeyboardKeyEvent = EisKeyboardKeyEvent;

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

impl Device for EisVirtualDevice {
    fn id(&self) -> String {
        format!("EIS virtual device {}", self.id)
    }

    fn name(&self) -> String {
        String::from("EIS virtual device")
    }

    fn has_capability(&self, capability: DeviceCapability) -> bool {
        // TODO: only actual EIS selected capabilities
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

impl NiriInputDevice for EisVirtualDevice {
    fn output(&self, _state: &State) -> Option<Output> {
        // TODO: Per-output
        None
        //self.output().cloned()
    }
}

struct EisKeyboardKeyEvent {}

impl traits::Event<EisInputBackend> for EisKeyboardKeyEvent {
    fn time(&self) -> u64 {
        todo!()
    }

    fn device(&self) -> <EisInputBackend as InputBackend>::Device {
        todo!()
    }
}
impl traits::KeyboardKeyEvent<EisInputBackend> for EisKeyboardKeyEvent {
    fn key_code(&self) -> traits::Keycode {
        todo!()
    }

    fn state(&self) -> traits::KeyState {
        todo!()
    }

    fn count(&self) -> u32 {
        todo!()
    }
}
