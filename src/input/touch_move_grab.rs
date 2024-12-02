use smithay::desktop::Window;
use smithay::input::touch::{
    DownEvent, GrabStartData as TouchGrabStartData, MotionEvent, OrientationEvent, ShapeEvent,
    TouchGrab, TouchInnerHandle, UpEvent,
};
use smithay::input::SeatHandler;
use smithay::utils::{IsAlive, Logical, Point, Serial};

use crate::niri::State;

pub struct TouchMoveGrab {
    start_data: TouchGrabStartData<State>,
    last_location: Point<f64, Logical>,
    window: Window,
}

impl TouchMoveGrab {
    pub fn new(start_data: TouchGrabStartData<State>, window: Window) -> Self {
        Self {
            last_location: start_data.location,
            start_data,
            window,
        }
    }

    fn on_ungrab(&mut self, state: &mut State) {
        state.niri.layout.interactive_move_end(&self.window);
        // FIXME: only redraw the window output.
        state.niri.queue_redraw_all();
    }
}

impl TouchGrab<State> for TouchMoveGrab {
    fn down(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::TouchFocus, Point<f64, Logical>)>,
        event: &DownEvent,
        seq: Serial,
    ) {
        handle.down(data, None, event, seq);
    }

    fn up(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        event: &UpEvent,
        seq: Serial,
    ) {
        handle.up(data, event, seq);

        if event.slot != self.start_data.slot {
            return;
        }

        handle.unset_grab(self, data);
    }

    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::TouchFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
        seq: Serial,
    ) {
        handle.motion(data, None, event, seq);

        if event.slot != self.start_data.slot {
            return;
        }

        if self.window.alive() {
            if let Some((output, pos_within_output)) = data.niri.output_under(event.location) {
                let output = output.clone();
                let event_delta = event.location - self.last_location;
                self.last_location = event.location;
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

        // The move is no longer ongoing.
        handle.unset_grab(self, data);
    }

    fn frame(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>, seq: Serial) {
        handle.frame(data, seq);
    }

    fn cancel(&mut self, data: &mut State, handle: &mut TouchInnerHandle<'_, State>, seq: Serial) {
        handle.cancel(data, seq);
        handle.unset_grab(self, data);
    }

    fn shape(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        event: &ShapeEvent,
        seq: Serial,
    ) {
        handle.shape(data, event, seq);
    }

    fn orientation(
        &mut self,
        data: &mut State,
        handle: &mut TouchInnerHandle<'_, State>,
        event: &OrientationEvent,
        seq: Serial,
    ) {
        handle.orientation(data, event, seq);
    }

    fn start_data(&self) -> &TouchGrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut State) {
        self.on_ungrab(data);
    }
}
