use std::borrow::Cow;

use smithay::backend::input::KeyState;
use smithay::desktop::PopupKind;
use smithay::input::keyboard::{KeyboardTarget, KeysymHandle, ModifiersState};
use smithay::input::pointer::{self, PointerTarget};
use smithay::input::Seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Serial};
use smithay::wayland::seat::WaylandFocus;

use crate::niri::State;
use crate::ui::screenshot_ui::ScreenshotUiFocusTarget;

#[derive(Debug, Clone, PartialEq)]
pub enum KeyboardFocusTarget {
    Surface(WlSurface),
    ScreenshotUi(ScreenshotUiFocusTarget),
    Overview,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointerFocusTarget {
    Surface(WlSurface),
    ScreenshotUi(ScreenshotUiFocusTarget),
    Overview,
}

impl KeyboardFocusTarget {
    fn inner(&self) -> &dyn KeyboardTarget<State> {
        match self {
            Self::Surface(surface) => surface,
            Self::ScreenshotUi(x) => x,
            Self::Overview => todo!(),
        }
    }
}

impl PointerFocusTarget {
    fn inner(&self) -> &dyn PointerTarget<State> {
        match self {
            Self::Surface(surface) => surface,
            Self::ScreenshotUi(x) => x,
            Self::Overview => todo!(),
        }
    }

    pub fn surface(&self) -> Option<&WlSurface> {
        match self {
            PointerFocusTarget::Surface(surface) => Some(surface),
            PointerFocusTarget::ScreenshotUi(_) => None,
            PointerFocusTarget::Overview => None,
        }
    }
}

impl From<KeyboardFocusTarget> for PointerFocusTarget {
    fn from(value: KeyboardFocusTarget) -> Self {
        match value {
            KeyboardFocusTarget::Surface(surface) => Self::Surface(surface),
            KeyboardFocusTarget::ScreenshotUi(x) => Self::ScreenshotUi(x),
            KeyboardFocusTarget::Overview => Self::Overview,
        }
    }
}

impl From<PopupKind> for KeyboardFocusTarget {
    fn from(value: PopupKind) -> Self {
        Self::Surface(value.wl_surface().clone())
    }
}

impl WaylandFocus for KeyboardFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            Self::Surface(surface) => Some(surface),
            Self::ScreenshotUi(_) => None,
            Self::Overview => None,
        }
        .map(Cow::Borrowed)
    }
}

impl WaylandFocus for PointerFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        self.surface().map(Cow::Borrowed)
    }
}

impl IsAlive for KeyboardFocusTarget {
    fn alive(&self) -> bool {
        match self {
            Self::Surface(surface) => surface.alive(),
            Self::ScreenshotUi(_) => true,
            Self::Overview => true,
        }
    }
}

impl IsAlive for PointerFocusTarget {
    fn alive(&self) -> bool {
        match self {
            Self::Surface(surface) => surface.alive(),
            Self::ScreenshotUi(_) => true,
            Self::Overview => true,
        }
    }
}

impl KeyboardTarget<State> for KeyboardFocusTarget {
    fn enter(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        self.inner().enter(seat, data, keys, serial);
    }

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial) {
        self.inner().leave(seat, data, serial);
    }

    fn key(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: u32,
    ) {
        self.inner().key(seat, data, key, state, serial, time);
    }

    fn modifiers(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        self.inner().modifiers(seat, data, modifiers, serial);
    }
}

impl PointerTarget<State> for PointerFocusTarget {
    fn enter(&self, seat: &Seat<State>, data: &mut State, event: &pointer::MotionEvent) {
        self.inner().enter(seat, data, event);
    }

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &pointer::MotionEvent) {
        self.inner().motion(seat, data, event);
    }

    fn relative_motion(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::RelativeMotionEvent,
    ) {
        self.inner().relative_motion(seat, data, event);
    }

    fn button(&self, seat: &Seat<State>, data: &mut State, event: &pointer::ButtonEvent) {
        self.inner().button(seat, data, event);
    }

    fn axis(&self, seat: &Seat<State>, data: &mut State, frame: pointer::AxisFrame) {
        self.inner().axis(seat, data, frame);
    }

    fn frame(&self, seat: &Seat<State>, data: &mut State) {
        self.inner().frame(seat, data);
    }

    fn gesture_swipe_begin(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GestureSwipeBeginEvent,
    ) {
        self.inner().gesture_swipe_begin(seat, data, event);
    }

    fn gesture_swipe_update(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GestureSwipeUpdateEvent,
    ) {
        self.inner().gesture_swipe_update(seat, data, event);
    }

    fn gesture_swipe_end(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GestureSwipeEndEvent,
    ) {
        self.inner().gesture_swipe_end(seat, data, event);
    }

    fn gesture_pinch_begin(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GesturePinchBeginEvent,
    ) {
        self.inner().gesture_pinch_begin(seat, data, event);
    }

    fn gesture_pinch_update(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GesturePinchUpdateEvent,
    ) {
        self.inner().gesture_pinch_update(seat, data, event);
    }

    fn gesture_pinch_end(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GesturePinchEndEvent,
    ) {
        self.inner().gesture_pinch_end(seat, data, event);
    }

    fn gesture_hold_begin(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GestureHoldBeginEvent,
    ) {
        self.inner().gesture_hold_begin(seat, data, event);
    }

    fn gesture_hold_end(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &pointer::GestureHoldEndEvent,
    ) {
        self.inner().gesture_hold_end(seat, data, event);
    }

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial, time: u32) {
        self.inner().leave(seat, data, serial, time);
    }
}
