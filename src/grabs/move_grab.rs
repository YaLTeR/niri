use smithay::desktop::Window;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab,
    PointerInnerHandle, RelativeMotionEvent,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};
use smithay::wayland::seat::WaylandFocus;

use crate::Niri;

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<Niri>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
}

impl PointerGrab<Niri> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut Niri,
        handle: &mut PointerInnerHandle<'_, Niri>,
        _focus: Option<(WlSurface, Point<i32, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        // let delta = event.location - self.start_data.location;
        // let new_location = self.initial_window_location.to_f64() + delta;
        // let (window, space) = data
        //     .monitor_set
        //     .find_window_and_space(self.window.wl_surface().as_ref().unwrap())
        //     .unwrap();
        // space.map_element(window.clone(), new_location.to_i32_round(), true);
    }

    fn relative_motion(
        &mut self,
        data: &mut Niri,
        handle: &mut PointerInnerHandle<'_, Niri>,
        focus: Option<(WlSurface, Point<i32, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut Niri,
        handle: &mut PointerInnerHandle<'_, Niri>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        // The button is a button code as defined in the
        // Linux kernel's linux/input-event-codes.h header file, e.g. BTN_LEFT.
        const BTN_LEFT: u32 = 0x110;

        if !handle.current_pressed().contains(&BTN_LEFT) {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(data, event.serial, event.time);
        }
    }

    fn axis(
        &mut self,
        data: &mut Niri,
        handle: &mut PointerInnerHandle<'_, Niri>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn start_data(&self) -> &PointerGrabStartData<Niri> {
        &self.start_data
    }
}
