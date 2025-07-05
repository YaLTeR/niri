use smithay::backend::input::ButtonState;
use smithay::desktop::Window;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, GestureHoldBeginEvent,
    GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
    GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
    GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
    RelativeMotionEvent,
};
use smithay::input::SeatHandler;
use smithay::utils::{IsAlive, Logical, Point, Serial, Size, SERIAL_COUNTER};

use crate::niri::State;

pub struct MoveGrab {
    start_data: PointerGrabStartData<State>,
    last_location: Point<f64, Logical>,
    swipe_location: Point<f64, Logical>,
    window: Window,
    gesture: GestureState,
    is_swipe_pinch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GestureState {
    Recognizing,
    Move,
}

impl MoveGrab {
    pub fn new(
        start_data: PointerGrabStartData<State>,
        window: Window,
        use_threshold: bool,
        is_swipe_pinch: bool,
    ) -> Self {
        let gesture = if use_threshold {
            GestureState::Recognizing
        } else {
            GestureState::Move
        };

        Self {
            last_location: start_data.location,
            swipe_location: start_data.location,
            start_data,
            window,
            gesture,
            is_swipe_pinch,
        }
    }

    fn on_ungrab(&mut self, state: &mut State) {
        state.niri.layout.interactive_move_end(&self.window);
        // FIXME: only redraw the window output.
        state.niri.queue_redraw_all();
        if !self.is_swipe_pinch {
            state
                .niri
                .cursor_manager
                .set_cursor_image(CursorImageStatus::default_named());
        }
    }
}

impl PointerGrab<State> for MoveGrab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // The pointer should not be moved by swipe and pinch gestures
        if !self.is_swipe_pinch {
            // While the grab is active, no client has pointer focus.
            handle.motion(data, None, event);
        } else if event.serial != Serial::from(0) {
            // Ignore normal pointer motion events if we're in a swipe/pinch
            // gesture
            return;
        }

        if self.window.alive() {
            if let Some((output, pos_within_output)) = data.niri.output_under(event.location) {
                let output = output.clone();
                let event_delta = event.location - self.last_location;
                self.last_location = event.location;

                if self.gesture == GestureState::Recognizing {
                    let c = event.location - self.start_data.location;

                    // Check if the gesture moved far enough to decide.
                    if c.x * c.x + c.y * c.y >= 8. * 8. {
                        self.gesture = GestureState::Move;

                        if !self.is_swipe_pinch {
                            data.niri
                                .cursor_manager
                                .set_cursor_image(CursorImageStatus::Named(CursorIcon::Move));
                        }
                    }
                }

                if self.gesture != GestureState::Move {
                    return;
                }

                let ongoing = data.niri.layout.interactive_move_update(
                    &self.window,
                    event_delta,
                    output,
                    pos_within_output,
                );
                if ongoing {
                    // FIXME: only redraw the previous and the new output.
                    data.niri.queue_redraw_all();
                    return;
                }
            } else {
                return;
            }
        }

        // We asserted `event.serial == Serial::from(0)` above
        // if `is_swipe_pinch` is true. This is not a valid serial.
        let serial = if self.is_swipe_pinch {
            SERIAL_COUNTER.next_serial()
        } else {
            event.serial
        };
        // The move is no longer ongoing.
        handle.unset_grab(self, data, serial, event.time, true);
    }

    fn relative_motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        // While the grab is active, no client has pointer focus.
        handle.relative_motion(data, None, event);
    }

    fn button(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        // When moving with the left button, right toggles floating, and vice versa.
        let toggle_floating_button = if self.start_data.button == 0x110 {
            0x111
        } else {
            0x110
        };
        if event.button == toggle_floating_button && event.state == ButtonState::Pressed {
            data.niri.layout.toggle_window_floating(Some(&self.window));
        }

        if !handle.current_pressed().contains(&self.start_data.button) {
            // The button that initiated the grab was released.
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        _data: &mut State,
        _handle: &mut PointerInnerHandle<'_, State>,
        _event: &GestureSwipeBeginEvent,
    ) {
        // handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeUpdateEvent,
    ) {
        self.swipe_location += event.delta;
        if let Some(mut global_rect) = data.global_bounding_rectangle() {
            // Shrink by 1 logical pixel, retaining center
            global_rect.loc = global_rect.loc + Point::new(1, 1);
            global_rect.size = global_rect.size - Size::new(2, 2);
            self.swipe_location = self.swipe_location.constrain(global_rect.to_f64());
        }
        self.motion(
            data,
            handle,
            None,
            &MotionEvent {
                location: self.swipe_location,
                serial: Serial::from(0),
                time: event.time,
            },
        );
        // handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeEndEvent,
    ) {
        // handle.gesture_swipe_end(data, event);
        handle.unset_grab(self, data, event.serial, event.time, true);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut State) {
        self.on_ungrab(data);
    }
}
