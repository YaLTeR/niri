use smithay::backend::allocator::Fourcc;
use smithay::backend::input::ButtonState;
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorImageStatus, GestureHoldBeginEvent, GestureHoldEndEvent,
    GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
    GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData,
    MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
};
use smithay::input::SeatHandler;
use smithay::utils::{Logical, Physical, Point, Scale};

use crate::niri::State;
use crate::render_helpers::render_to_vec;

pub struct PickColorGrab {
    start_data: PointerGrabStartData<State>,
    temporary_location: Option<Point<f64, Logical>>,
}

impl PickColorGrab {
    pub fn new(start_data: PointerGrabStartData<State>) -> Self {
        Self {
            start_data,
            temporary_location: None,
        }
    }

    fn on_ungrab(&mut self, state: &mut State) {
        if let Some(tx) = state.niri.pick_color.take() {
            let _ = tx.send_blocking(None);
        }
        state
            .niri
            .cursor_manager
            .set_cursor_image(CursorImageStatus::default_named());
        state.niri.queue_redraw_all();
    }

    fn pick_color_at_point(location: Point<f64, Logical>, data: &mut State) -> Option<[u8; 4]> {
        let (output, pos_within_output) = data.niri.output_under(location)?;

        data.backend
            .with_primary_renderer(|renderer| {
                let scale = Scale::from(output.current_scale().fractional_scale());
                let physical_pos: Point<i32, Physical> =
                    pos_within_output.to_physical_precise_round(scale);
                let size = smithay::utils::Size::<i32, Physical>::from((1, 1));

                let elements = data.niri.render::<GlesRenderer>(
                    renderer,
                    output,
                    false,
                    crate::render_helpers::RenderTarget::ScreenCapture,
                );

                let pixels = match render_to_vec(
                    renderer,
                    size,
                    scale,
                    output.current_transform(),
                    Fourcc::Abgr8888,
                    elements.iter().rev().map(|elem| {
                        RelocateRenderElement::from_element(
                            elem,
                            Point::from((-physical_pos.x, -physical_pos.y)),
                            Relocate::Relative,
                        )
                    }),
                ) {
                    Ok(pixels) => pixels,
                    Err(_) => return None,
                };

                if pixels.len() == 4 {
                    Some([pixels[0], pixels[1], pixels[2], pixels[3]])
                } else {
                    None
                }
            })
            .flatten()
    }
}

impl PointerGrab<State> for PickColorGrab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
    }

    fn relative_motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, None, event);
    }

    fn button(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        if event.state != ButtonState::Pressed {
            return;
        }

        self.temporary_location = Some(handle.current_location());

        let color = match self.temporary_location {
            Some(location) => Self::pick_color_at_point(location, data),
            None => None,
        };

        if let Some(tx) = data.niri.pick_color.take() {
            let _ = tx.send_blocking(color);
        }

        handle.unset_grab(self, data, event.serial, event.time, true);
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
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
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
