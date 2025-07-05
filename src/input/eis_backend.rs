//! Input backend for emulated input events received via [EI](https://libinput.pages.freedesktop.org/libei/) and `org.gnome.Mutter.RemoteDesktop`.

use reis::Interface;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisRelativeDirection, AxisSource, ButtonState, Device,
    DeviceCapability, Event, InputBackend, KeyState, KeyboardKeyEvent, Keycode, PointerAxisEvent,
    PointerButtonEvent, PointerMotionAbsoluteEvent, PointerMotionEvent, UnusedEvent,
};
use smithay::output::Output;

use crate::input::backend_ext::NiriInputDevice;
use crate::niri::State;

pub struct EisInputBackend;

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct EisVirtualDevice {
    /// Remote desktop session ID
    pub session_id: usize,
    /// Device ID unique to the remote desktop session
    pub device_id: u64,
}

impl InputBackend for EisInputBackend {
    type Device = EisVirtualDevice;

    type KeyboardKeyEvent = EisEventAdapter<reis::request::KeyboardKey, PressedCount>;

    type PointerAxisEvent = CombinedEisEventBase<ScrollFrame>;
    type PointerButtonEvent = EisEventAdapter<reis::request::Button>;
    type PointerMotionEvent = EisEventAdapter<reis::request::PointerMotion>;
    type PointerMotionAbsoluteEvent = EisEventAdapter<reis::request::PointerMotionAbsolute>;

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
        format!(
            "Remote desktop (EIS) virtual device {}/{}",
            self.session_id, self.device_id
        )
    }

    fn name(&self) -> String {
        String::from("Remote desktop (EIS) virtual device")
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

impl NiriInputDevice for EisVirtualDevice {
    fn output(&self, _state: &State) -> Option<Output> {
        // This would map local output coordinates to global output
        // coordinates if devices were per-output.
        None
    }
}

/// Wrapper to implement [`Event`] automatically and to hold extra data
pub struct EisEventAdapter<Ev, Extra = ()> {
    /// Remote desktop session ID
    pub session_id: usize,
    pub inner: Ev,
    pub extra: Extra,
}

impl<Ev: reis::request::EventTime, Extra> Event<EisInputBackend> for EisEventAdapter<Ev, Extra> {
    fn time(&self) -> u64 {
        self.inner.time()
    }

    fn device(&self) -> <EisInputBackend as InputBackend>::Device {
        EisVirtualDevice {
            session_id: self.session_id,
            device_id: self.inner.device().device().as_object().id(),
        }
    }
}

/// Extra passed to the keyboard key event containing the number of keys pressed on all devices in
/// the seat.
pub struct PressedCount(pub u32);

impl KeyboardKeyEvent<EisInputBackend>
    for EisEventAdapter<reis::request::KeyboardKey, PressedCount>
{
    fn key_code(&self) -> Keycode {
        // Offset from evdev keycodes (where KEY_ESCAPE is 1) to X11 keycodes
        Keycode::new(self.inner.key + 8)
    }

    fn state(&self) -> KeyState {
        match self.inner.state {
            reis::ei::keyboard::KeyState::Released => KeyState::Released,
            reis::ei::keyboard::KeyState::Press => KeyState::Pressed,
        }
    }

    fn count(&self) -> u32 {
        // Smithay does this already...
        self.extra.0
    }
}

/// A base for input events which consist of multiple high-level EIS requests
pub struct CombinedEisEventBase<T> {
    /// Remote desktop session ID
    pub session_id: usize,
    pub time: u64,
    pub device: reis::request::Device,
    pub data: T,
}

impl<T> Event<EisInputBackend> for CombinedEisEventBase<T> {
    fn time(&self) -> u64 {
        self.time
    }

    fn device(&self) -> <EisInputBackend as InputBackend>::Device {
        EisVirtualDevice {
            session_id: self.session_id,
            device_id: self.device.device().as_object().id(),
        }
    }
}

#[derive(Default)]
pub struct ScrollFrame {
    /// Continuous scrolling, like on a touchpad.
    pub delta: Option<(f32, f32)>,
    /// 120-notch scrolling, like on a traditional mouse wheel.
    ///
    /// According to the EI protocol, 120 in discrete is the same as 1.0 in delta.
    pub discrete: Option<(i32, i32)>,
    /// Last bool denotes `is_cancel`
    pub stop: Option<((bool, bool), bool)>,
}

fn tuple_axis<T>(tuple: (T, T), axis: Axis) -> T {
    match axis {
        Axis::Horizontal => tuple.0,
        Axis::Vertical => tuple.1,
    }
}

// TODO: this doesn't even have to be combined, because these should be mutually exclusive
impl PointerAxisEvent<EisInputBackend> for CombinedEisEventBase<ScrollFrame> {
    // TODO: test on both continuous and wheel

    // TODO: contradiction:
    // - `fn amount` documentation says that this is in pixels
    // - EI protocol says that a single scroll notch is 1.0
    // - Niri converts v120 to delta with (x/120*15)
    fn amount(&self, axis: Axis) -> Option<f64> {
        let mut value = tuple_axis(self.data.delta?, axis) as f64;

        if let Some(stop) = self.data.stop {
            if tuple_axis(stop.0, axis) {
                // Niri detects axis stop based on this
                value = 0.0;
            }
        }

        Some(value)
    }

    fn amount_v120(&self, axis: Axis) -> Option<f64> {
        // TODO: divide by 120?
        Some(tuple_axis(self.data.discrete?, axis) as f64)
    }

    fn source(&self) -> AxisSource {
        // No source for this, can only guess
        if self.data.delta.is_some() || self.data.stop.is_some() {
            AxisSource::Continuous
        } else {
            AxisSource::Wheel
        }
    }

    fn relative_direction(&self, _axis: Axis) -> AxisRelativeDirection {
        AxisRelativeDirection::Identical
    }
}

impl PointerButtonEvent<EisInputBackend> for EisEventAdapter<reis::request::Button> {
    fn button_code(&self) -> u32 {
        self.inner.button
    }

    fn state(&self) -> ButtonState {
        match self.inner.state {
            reis::ei::button::ButtonState::Released => ButtonState::Released,
            reis::ei::button::ButtonState::Press => ButtonState::Pressed,
        }
    }
}

impl PointerMotionEvent<EisInputBackend> for EisEventAdapter<reis::request::PointerMotion> {
    fn delta_x(&self) -> f64 {
        self.inner.dx as f64
    }

    fn delta_y(&self) -> f64 {
        self.inner.dy as f64
    }

    // Virtual pointer impl does this
    fn delta_x_unaccel(&self) -> f64 {
        self.inner.dx as f64
    }

    fn delta_y_unaccel(&self) -> f64 {
        self.inner.dy as f64
    }
}

impl AbsolutePositionEvent<EisInputBackend>
    for EisEventAdapter<reis::request::PointerMotionAbsolute>
{
    // It's in logical pixels..??!! So divide by region width first?
    // FIXME: this is actually super incorrect and should not work
    // TODO: send regions to EI clients
    fn x(&self) -> f64 {
        self.inner.dx_absolute as f64
    }

    fn y(&self) -> f64 {
        self.inner.dy_absolute as f64
    }

    fn x_transformed(&self, width: i32) -> f64 {
        self.inner.dx_absolute as f64 * width as f64
    }

    fn y_transformed(&self, height: i32) -> f64 {
        self.inner.dy_absolute as f64 * height as f64
    }
}

impl PointerMotionAbsoluteEvent<EisInputBackend>
    for EisEventAdapter<reis::request::PointerMotionAbsolute>
{
}
