use std::collections::HashSet;

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Device, DeviceCapability, Event,
    GestureBeginEvent, GestureEndEvent, GesturePinchUpdateEvent as _, GestureSwipeUpdateEvent as _,
    InputBackend, InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent, ProximityState, TabletToolButtonEvent, TabletToolEvent,
    TabletToolProximityEvent, TabletToolTipEvent, TabletToolTipState,
};
use smithay::backend::libinput::LibinputInputBackend;
use smithay::input::keyboard::{keysyms, FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorImageStatus, GestureHoldBeginEvent, GestureHoldEndEvent,
    GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
    GestureSwipeEndEvent, GestureSwipeUpdateEvent, MotionEvent, RelativeMotionEvent,
};
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::tablet_manager::{TabletDescriptor, TabletSeatTrait};

use crate::config::{Action, Binds, LayoutAction, Modifiers};
use crate::niri::State;
use crate::screenshot_ui::ScreenshotUi;
use crate::utils::{center, get_monotonic_time, spawn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorMod {
    Super,
    Alt,
}

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        let _span = tracy_client::span!("process_input_event");

        // A bit of a hack, but animation end runs some logic (i.e. workspace clean-up) and it
        // doesn't always trigger due to damage, etc. So run it here right before it might prove
        // important. Besides, animations affect the input, so it's best to have up-to-date values
        // here.
        self.niri.layout.advance_animations(get_monotonic_time());

        // Power on monitors if they were off.
        if should_activate_monitors(&event) {
            self.niri.activate_monitors(&self.backend);
        }

        let comp_mod = self.backend.mod_key();

        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let pressed = event.state() == KeyState::Pressed;

                let Some(Some(action)) = self.niri.seat.get_keyboard().unwrap().input(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |this, mods, keysym| {
                        let bindings = &this.niri.config.borrow().binds;
                        let key_code = event.key_code();
                        let modified = keysym.modified_sym();
                        let raw = keysym.raw_latin_sym_or_raw_current_sym();
                        should_intercept_key(
                            &mut this.niri.suppressed_keys,
                            bindings,
                            comp_mod,
                            key_code,
                            modified,
                            raw,
                            pressed,
                            *mods,
                            &this.niri.screenshot_ui,
                        )
                    },
                ) else {
                    return;
                };

                // Filter actions when the key is released or the session is locked.
                if !pressed {
                    return;
                }

                if self.niri.is_locked()
                    && !matches!(
                        action,
                        Action::Quit
                            | Action::ChangeVt(_)
                            | Action::Suspend
                            | Action::PowerOffMonitors
                            | Action::SwitchLayout(_)
                    )
                {
                    return;
                }

                match action {
                    Action::Quit => {
                        info!("quitting because quit bind was pressed");
                        self.niri.stop_signal.stop()
                    }
                    Action::ChangeVt(vt) => {
                        self.backend.change_vt(vt);
                        // Changing `VT` may not deliver the key releases, so clear the state.
                        self.niri.suppressed_keys.clear();
                    }
                    Action::Suspend => {
                        self.backend.suspend();
                        // Suspend may not deliver the key releases, so clear the state.
                        self.niri.suppressed_keys.clear();
                    }
                    Action::PowerOffMonitors => {
                        self.niri.deactivate_monitors(&self.backend);
                    }
                    Action::ToggleDebugTint => {
                        self.backend.toggle_debug_tint();
                    }
                    Action::Spawn(command) => {
                        if let Some((command, args)) = command.split_first() {
                            spawn(command, args);
                        }
                    }
                    Action::ScreenshotScreen => {
                        let active = self.niri.layout.active_output().cloned();
                        if let Some(active) = active {
                            if let Some(renderer) = self.backend.renderer() {
                                if let Err(err) = self.niri.screenshot(renderer, &active) {
                                    warn!("error taking screenshot: {err:?}");
                                }
                            }
                        }
                    }
                    Action::ConfirmScreenshot => {
                        if let Some(renderer) = self.backend.renderer() {
                            match self.niri.screenshot_ui.capture(renderer) {
                                Ok((size, pixels)) => {
                                    if let Err(err) = self.niri.save_screenshot(size, pixels) {
                                        warn!("error saving screenshot: {err:?}");
                                    }
                                }
                                Err(err) => {
                                    warn!("error capturing screenshot: {err:?}");
                                }
                            }
                        }

                        self.niri.screenshot_ui.close();
                        self.niri
                            .cursor_manager
                            .set_cursor_image(CursorImageStatus::default_named());
                        self.niri.queue_redraw_all();
                    }
                    Action::CancelScreenshot => {
                        self.niri.screenshot_ui.close();
                        self.niri
                            .cursor_manager
                            .set_cursor_image(CursorImageStatus::default_named());
                        self.niri.queue_redraw_all();
                    }
                    Action::Screenshot => {
                        if let Some(renderer) = self.backend.renderer() {
                            self.niri.open_screenshot_ui(renderer);
                        }
                    }
                    Action::ScreenshotWindow => {
                        let active = self.niri.layout.active_window();
                        if let Some((window, output)) = active {
                            if let Some(renderer) = self.backend.renderer() {
                                if let Err(err) =
                                    self.niri.screenshot_window(renderer, &output, &window)
                                {
                                    warn!("error taking screenshot: {err:?}");
                                }
                            }
                        }
                    }
                    Action::CloseWindow => {
                        if let Some(window) = self.niri.layout.focus() {
                            window.toplevel().send_close();
                        }
                    }
                    Action::FullscreenWindow => {
                        let focus = self.niri.layout.focus().cloned();
                        if let Some(window) = focus {
                            self.niri.layout.toggle_fullscreen(&window);
                        }
                    }
                    Action::SwitchLayout(action) => {
                        self.niri
                            .seat
                            .get_keyboard()
                            .unwrap()
                            .with_kkb_state(self, |mut state| match action {
                                LayoutAction::Next => state.cycle_next_layout(),
                                LayoutAction::Prev => state.cycle_prev_layout(),
                            });
                    }
                    Action::MoveColumnLeft => {
                        self.niri.layout.move_left();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MoveColumnRight => {
                        self.niri.layout.move_right();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MoveWindowDown => {
                        self.niri.layout.move_down();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MoveWindowUp => {
                        self.niri.layout.move_up();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::FocusColumnLeft => {
                        self.niri.layout.focus_left();
                    }
                    Action::FocusColumnRight => {
                        self.niri.layout.focus_right();
                    }
                    Action::FocusWindowDown => {
                        self.niri.layout.focus_down();
                    }
                    Action::FocusWindowUp => {
                        self.niri.layout.focus_up();
                    }
                    Action::MoveWindowToWorkspaceDown => {
                        self.niri.layout.move_to_workspace_down();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MoveWindowToWorkspaceUp => {
                        self.niri.layout.move_to_workspace_up();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MoveWindowToWorkspace(idx) => {
                        self.niri.layout.move_to_workspace(idx);
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::FocusWorkspaceDown => {
                        self.niri.layout.switch_workspace_down();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::FocusWorkspaceUp => {
                        self.niri.layout.switch_workspace_up();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::FocusWorkspace(idx) => {
                        self.niri.layout.switch_workspace(idx);
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MoveWorkspaceDown => {
                        self.niri.layout.move_workspace_down();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MoveWorkspaceUp => {
                        self.niri.layout.move_workspace_up();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::ConsumeWindowIntoColumn => {
                        self.niri.layout.consume_into_column();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::ExpelWindowFromColumn => {
                        self.niri.layout.expel_from_column();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::SwitchPresetColumnWidth => {
                        self.niri.layout.toggle_width();
                    }
                    Action::CenterColumn => {
                        self.niri.layout.center_column();
                        // FIXME: granular
                        self.niri.queue_redraw_all();
                    }
                    Action::MaximizeColumn => {
                        self.niri.layout.toggle_full_width();
                    }
                    Action::FocusMonitorLeft => {
                        if let Some(output) = self.niri.output_left() {
                            self.niri.layout.focus_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::FocusMonitorRight => {
                        if let Some(output) = self.niri.output_right() {
                            self.niri.layout.focus_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::FocusMonitorDown => {
                        if let Some(output) = self.niri.output_down() {
                            self.niri.layout.focus_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::FocusMonitorUp => {
                        if let Some(output) = self.niri.output_up() {
                            self.niri.layout.focus_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::MoveWindowToMonitorLeft => {
                        if let Some(output) = self.niri.output_left() {
                            self.niri.layout.move_to_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::MoveWindowToMonitorRight => {
                        if let Some(output) = self.niri.output_right() {
                            self.niri.layout.move_to_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::MoveWindowToMonitorDown => {
                        if let Some(output) = self.niri.output_down() {
                            self.niri.layout.move_to_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::MoveWindowToMonitorUp => {
                        if let Some(output) = self.niri.output_up() {
                            self.niri.layout.move_to_output(&output);
                            self.move_cursor_to_output(&output);
                        }
                    }
                    Action::SetColumnWidth(change) => {
                        self.niri.layout.set_column_width(change);
                    }
                    Action::SetWindowHeight(change) => {
                        self.niri.layout.set_window_height(change);
                    }
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                // We need an output to be able to move the pointer.
                if self.niri.global_space.outputs().next().is_none() {
                    return;
                }

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.niri.seat.get_pointer().unwrap();

                let pos = pointer.current_location();

                // We have an output, so we can compute the new location and focus.
                let mut new_pos = pos + event.delta();

                if self
                    .niri
                    .global_space
                    .output_under(new_pos)
                    .next()
                    .is_none()
                {
                    // We ended up outside the outputs and need to clip the movement.
                    if let Some(output) = self.niri.global_space.output_under(pos).next() {
                        // The pointer was previously on some output. Clip the movement against its
                        // boundaries.
                        let geom = self.niri.global_space.output_geometry(output).unwrap();
                        new_pos.x = new_pos
                            .x
                            .clamp(geom.loc.x as f64, (geom.loc.x + geom.size.w - 1) as f64);
                        new_pos.y = new_pos
                            .y
                            .clamp(geom.loc.y as f64, (geom.loc.y + geom.size.h - 1) as f64);
                    } else {
                        // The pointer was not on any output in the first place. Find one for it.
                        // Let's do the simple thing and just put it on the first output.
                        let output = self.niri.global_space.outputs().next().unwrap();
                        let geom = self.niri.global_space.output_geometry(output).unwrap();
                        new_pos = center(geom).to_f64();
                    }
                }

                if let Some(output) = self.niri.screenshot_ui.selection_output() {
                    let geom = self.niri.global_space.output_geometry(output).unwrap();
                    let mut point = new_pos;
                    point.x = point
                        .x
                        .clamp(geom.loc.x as f64, (geom.loc.x + geom.size.w - 1) as f64);
                    point.y = point
                        .y
                        .clamp(geom.loc.y as f64, (geom.loc.y + geom.size.h - 1) as f64);
                    let point = (point - geom.loc.to_f64())
                        .to_physical(output.current_scale().fractional_scale())
                        .to_i32_round();
                    self.niri.screenshot_ui.pointer_motion(point);
                }

                let under = self.niri.surface_under_and_global_space(new_pos);
                self.niri.pointer_focus = under.clone();
                let under = under.map(|u| u.surface);

                pointer.motion(
                    self,
                    under.clone(),
                    &MotionEvent {
                        location: new_pos,
                        serial,
                        time: event.time_msec(),
                    },
                );

                pointer.relative_motion(
                    self,
                    under,
                    &RelativeMotionEvent {
                        delta: event.delta(),
                        delta_unaccel: event.delta_unaccel(),
                        utime: event.time(),
                    },
                );

                pointer.frame(self);

                // Redraw to update the cursor position.
                // FIXME: redraw only outputs overlapping the cursor.
                self.niri.queue_redraw_all();
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(output) = self.niri.global_space.outputs().next() else {
                    return;
                };

                let output_geo = self.niri.global_space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.niri.seat.get_pointer().unwrap();

                if let Some(output) = self.niri.screenshot_ui.selection_output() {
                    let geom = self.niri.global_space.output_geometry(output).unwrap();
                    let mut point = pos;
                    point.x = point
                        .x
                        .clamp(geom.loc.x as f64, (geom.loc.x + geom.size.w - 1) as f64);
                    point.y = point
                        .y
                        .clamp(geom.loc.y as f64, (geom.loc.y + geom.size.h - 1) as f64);
                    let point = (point - geom.loc.to_f64())
                        .to_physical(output.current_scale().fractional_scale())
                        .to_i32_round();
                    self.niri.screenshot_ui.pointer_motion(point);
                }

                let under = self.niri.surface_under_and_global_space(pos);
                self.niri.pointer_focus = under.clone();
                let under = under.map(|u| u.surface);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );

                pointer.frame(self);

                // Redraw to update the cursor position.
                // FIXME: redraw only outputs overlapping the cursor.
                self.niri.queue_redraw_all();
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.niri.seat.get_pointer().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some(window) = self.niri.window_under_cursor() {
                        let window = window.clone();
                        self.niri.layout.activate_window(&window);
                    } else if let Some(output) = self.niri.output_under_cursor() {
                        self.niri.layout.activate_output(&output);
                    }
                };

                self.update_pointer_focus();

                if let Some(button) = event.button() {
                    let pos = pointer.current_location();
                    if let Some((output, _)) = self.niri.output_under(pos) {
                        let output = output.clone();
                        let geom = self.niri.global_space.output_geometry(&output).unwrap();
                        let mut point = pos;
                        // Re-clamp as pointer can be within 0.5 from the limit which will round up
                        // to a wrong value.
                        point.x = point
                            .x
                            .clamp(geom.loc.x as f64, (geom.loc.x + geom.size.w - 1) as f64);
                        point.y = point
                            .y
                            .clamp(geom.loc.y as f64, (geom.loc.y + geom.size.h - 1) as f64);
                        let point = (point - geom.loc.to_f64())
                            .to_physical(output.current_scale().fractional_scale())
                            .to_i32_round();
                        if self.niri.screenshot_ui.pointer_button(
                            output,
                            point,
                            button,
                            button_state,
                        ) {
                            self.niri.queue_redraw_all();
                        }
                    }
                }

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_discrete(Axis::Horizontal).unwrap_or(0.0) * 3.0
                });
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_discrete(Axis::Vertical).unwrap_or(0.0) * 3.0);
                let horizontal_amount_discrete = event.amount_discrete(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_discrete(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.discrete(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.discrete(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                self.update_pointer_focus();

                let pointer = &self.niri.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            InputEvent::TabletToolAxis { event, .. } => {
                let Some(output) = self.niri.output_for_tablet() else {
                    return;
                };

                let output_geo = self.niri.global_space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.niri.seat.get_pointer().unwrap();

                let under = self.niri.surface_under_and_global_space(pos);
                self.niri.pointer_focus = under.clone();
                let under = under.map(|u| u.surface);

                pointer.motion(
                    self,
                    under.clone(),
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);

                let tablet_seat = self.niri.seat.tablet_seat();
                let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&event.device()));
                let tool = tablet_seat.get_tool(&event.tool());
                if let (Some(tablet), Some(tool)) = (tablet, tool) {
                    if event.pressure_has_changed() {
                        tool.pressure(event.pressure());
                    }
                    if event.distance_has_changed() {
                        tool.distance(event.distance());
                    }
                    if event.tilt_has_changed() {
                        tool.tilt(event.tilt());
                    }
                    if event.slider_has_changed() {
                        tool.slider_position(event.slider_position());
                    }
                    if event.rotation_has_changed() {
                        tool.rotation(event.rotation());
                    }
                    if event.wheel_has_changed() {
                        tool.wheel(event.wheel_delta(), event.wheel_delta_discrete());
                    }

                    tool.motion(
                        pos,
                        under,
                        &tablet,
                        SERIAL_COUNTER.next_serial(),
                        event.time_msec(),
                    );
                }

                // Redraw to update the cursor position.
                // FIXME: redraw only outputs overlapping the cursor.
                self.niri.queue_redraw_all();
            }
            InputEvent::TabletToolTip { event, .. } => {
                let tool = self.niri.seat.tablet_seat().get_tool(&event.tool());

                if let Some(tool) = tool {
                    match event.tip_state() {
                        TabletToolTipState::Down => {
                            let serial = SERIAL_COUNTER.next_serial();
                            tool.tip_down(serial, event.time_msec());

                            let pointer = self.niri.seat.get_pointer().unwrap();
                            if !pointer.is_grabbed() {
                                if let Some(window) = self.niri.window_under_cursor() {
                                    let window = window.clone();
                                    self.niri.layout.activate_window(&window);
                                } else if let Some(output) = self.niri.output_under_cursor() {
                                    self.niri.layout.activate_output(&output);
                                }
                            };
                        }
                        TabletToolTipState::Up => {
                            tool.tip_up(event.time_msec());
                        }
                    }
                }
            }
            InputEvent::TabletToolProximity { event, .. } => {
                let Some(output) = self.niri.output_for_tablet() else {
                    return;
                };

                let output_geo = self.niri.global_space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.niri.seat.get_pointer().unwrap();

                let under = self.niri.surface_under_and_global_space(pos);
                self.niri.pointer_focus = under.clone();
                let under = under.map(|u| u.surface);

                pointer.motion(
                    self,
                    under.clone(),
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);

                let tablet_seat = self.niri.seat.tablet_seat();
                let tool = tablet_seat.add_tool::<Self>(&self.niri.display_handle, &event.tool());
                let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&event.device()));
                if let (Some(under), Some(tablet)) = (under, tablet) {
                    match event.state() {
                        ProximityState::In => tool.proximity_in(
                            pos,
                            under,
                            &tablet,
                            SERIAL_COUNTER.next_serial(),
                            event.time_msec(),
                        ),
                        ProximityState::Out => tool.proximity_out(event.time_msec()),
                    }
                }
            }
            InputEvent::TabletToolButton { event, .. } => {
                let tool = self.niri.seat.tablet_seat().get_tool(&event.tool());

                if let Some(tool) = tool {
                    tool.button(
                        event.button(),
                        event.button_state(),
                        SERIAL_COUNTER.next_serial(),
                        event.time_msec(),
                    );
                }
            }
            InputEvent::DeviceAdded { device } => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    self.niri.seat.tablet_seat().add_tablet::<Self>(
                        &self.niri.display_handle,
                        &TabletDescriptor::from(&device),
                    );
                }
            }
            InputEvent::DeviceRemoved { device } => {
                if device.has_capability(DeviceCapability::TabletTool) {
                    let tablet_seat = self.niri.seat.tablet_seat();

                    tablet_seat.remove_tablet(&TabletDescriptor::from(&device));

                    // If there are no tablets in seat we can remove all tools
                    if tablet_seat.count_tablets() == 0 {
                        tablet_seat.clear_tools();
                    }
                }
            }
            InputEvent::GestureSwipeBegin { event } => {
                if event.fingers() == 3 {
                    if let Some(output) = self.niri.output_under_cursor() {
                        self.niri.layout.workspace_switch_gesture_begin(&output);

                        // FIXME: granular. This one is awkward because this can cancel a gesture on
                        // multiple other outputs in theory.
                        self.niri.queue_redraw_all();
                    }

                    // We handled this event.
                    return;
                }

                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_swipe_begin(
                    self,
                    &GestureSwipeBeginEvent {
                        serial,
                        time: event.time_msec(),
                        fingers: event.fingers(),
                    },
                );
            }
            InputEvent::GestureSwipeUpdate { event } => {
                let res = self
                    .niri
                    .layout
                    .workspace_switch_gesture_update(event.delta_y());
                if let Some(output) = res {
                    if let Some(output) = output {
                        self.niri.queue_redraw(output);
                    }

                    // We handled this event.
                    return;
                }

                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_swipe_update(
                    self,
                    &GestureSwipeUpdateEvent {
                        time: event.time_msec(),
                        delta: event.delta(),
                    },
                );
            }
            InputEvent::GestureSwipeEnd { event } => {
                let res = self
                    .niri
                    .layout
                    .workspace_switch_gesture_end(event.cancelled());
                if let Some(output) = res {
                    self.niri.queue_redraw(output);

                    // We handled this event.
                    return;
                }

                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_swipe_end(
                    self,
                    &GestureSwipeEndEvent {
                        serial,
                        time: event.time_msec(),
                        cancelled: event.cancelled(),
                    },
                );
            }
            InputEvent::GesturePinchBegin { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_pinch_begin(
                    self,
                    &GesturePinchBeginEvent {
                        serial,
                        time: event.time_msec(),
                        fingers: event.fingers(),
                    },
                );
            }
            InputEvent::GesturePinchUpdate { event } => {
                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_pinch_update(
                    self,
                    &GesturePinchUpdateEvent {
                        time: event.time_msec(),
                        delta: event.delta(),
                        scale: event.scale(),
                        rotation: event.rotation(),
                    },
                );
            }
            InputEvent::GesturePinchEnd { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_pinch_end(
                    self,
                    &GesturePinchEndEvent {
                        serial,
                        time: event.time_msec(),
                        cancelled: event.cancelled(),
                    },
                );
            }
            InputEvent::GestureHoldBegin { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_hold_begin(
                    self,
                    &GestureHoldBeginEvent {
                        serial,
                        time: event.time_msec(),
                        fingers: event.fingers(),
                    },
                );
            }
            InputEvent::GestureHoldEnd { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.niri.seat.get_pointer().unwrap();

                if self.update_pointer_focus() {
                    pointer.frame(self);
                }

                pointer.gesture_hold_end(
                    self,
                    &GestureHoldEndEvent {
                        serial,
                        time: event.time_msec(),
                        cancelled: event.cancelled(),
                    },
                );
            }
            InputEvent::TouchDown { .. } => (),
            InputEvent::TouchMotion { .. } => (),
            InputEvent::TouchUp { .. } => (),
            InputEvent::TouchCancel { .. } => (),
            InputEvent::TouchFrame { .. } => (),
            InputEvent::Special(_) => (),
        }
    }

    pub fn process_libinput_event(&mut self, event: &mut InputEvent<LibinputInputBackend>) {
        if let InputEvent::DeviceAdded { device } = event {
            // According to Mutter code, this setting is specific to touchpads.
            let is_touchpad = device.config_tap_finger_count() > 0;
            if is_touchpad {
                let c = &self.niri.config.borrow().input.touchpad;
                let _ = device.config_tap_set_enabled(c.tap);
                let _ = device.config_scroll_set_natural_scroll_enabled(c.natural_scroll);
                let _ = device.config_accel_set_speed(c.accel_speed);
            }
        }
    }
}

/// Check whether the key should be intercepted and mark intercepted
/// pressed keys as `suppressed`, thus preventing `releases` corresponding
/// to them from being delivered.
#[allow(clippy::too_many_arguments)]
fn should_intercept_key(
    suppressed_keys: &mut HashSet<u32>,
    bindings: &Binds,
    comp_mod: CompositorMod,
    key_code: u32,
    modified: Keysym,
    raw: Option<Keysym>,
    pressed: bool,
    mods: ModifiersState,
    screenshot_ui: &ScreenshotUi,
) -> FilterResult<Option<Action>> {
    // Actions are only triggered on presses, release of the key
    // shouldn't try to intercept anything unless we have marked
    // the key to suppress.
    if !pressed && !suppressed_keys.contains(&key_code) {
        return FilterResult::Forward;
    }

    let mut final_action = action(bindings, comp_mod, modified, raw, mods);
    if screenshot_ui.is_open()
        // Allow only a subset of compositor actions while the screenshot UI is open,
        // since the user cannot see the screen.
        && !matches!(
            final_action,
            Some(Action::Quit | Action::ChangeVt(_) | Action::Suspend | Action::PowerOffMonitors)
        )
    {
        // Otherwise, use the screenshot UI action.
        final_action = screenshot_ui.action(raw, mods);
    }

    match (final_action, pressed) {
        (Some(action), true) => {
            suppressed_keys.insert(key_code);
            FilterResult::Intercept(Some(action))
        }
        (_, false) => {
            suppressed_keys.remove(&key_code);
            FilterResult::Intercept(None)
        }
        (None, true) => FilterResult::Forward,
    }
}

fn action(
    bindings: &Binds,
    comp_mod: CompositorMod,
    modified: Keysym,
    raw: Option<Keysym>,
    mods: ModifiersState,
) -> Option<Action> {
    use keysyms::*;

    // Handle hardcoded binds.
    #[allow(non_upper_case_globals)] // wat
    match modified.raw() {
        modified @ KEY_XF86Switch_VT_1..=KEY_XF86Switch_VT_12 => {
            let vt = (modified - KEY_XF86Switch_VT_1 + 1) as i32;
            return Some(Action::ChangeVt(vt));
        }
        KEY_XF86PowerOff => return Some(Action::Suspend),
        _ => (),
    }

    // Handle configured binds.
    let mut modifiers = Modifiers::empty();
    if mods.ctrl {
        modifiers |= Modifiers::CTRL;
    }
    if mods.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if mods.alt {
        modifiers |= Modifiers::ALT;
    }
    if mods.logo {
        modifiers |= Modifiers::SUPER;
    }

    let (mod_down, mut comp_mod) = match comp_mod {
        CompositorMod::Super => (mods.logo, Modifiers::SUPER),
        CompositorMod::Alt => (mods.alt, Modifiers::ALT),
    };
    if mod_down {
        modifiers |= Modifiers::COMPOSITOR;
    } else {
        comp_mod = Modifiers::empty();
    }

    let Some(raw) = raw else {
        return None;
    };

    for bind in &bindings.0 {
        if bind.key.keysym != raw {
            continue;
        }

        if bind.key.modifiers | comp_mod == modifiers {
            return bind.actions.first().cloned();
        }
    }

    None
}

fn should_activate_monitors<I: InputBackend>(event: &InputEvent<I>) -> bool {
    match event {
        InputEvent::Keyboard { event } if event.state() == KeyState::Pressed => true,
        InputEvent::PointerMotion { .. }
        | InputEvent::PointerMotionAbsolute { .. }
        | InputEvent::PointerButton { .. }
        | InputEvent::PointerAxis { .. }
        | InputEvent::GestureSwipeBegin { .. }
        | InputEvent::GesturePinchBegin { .. }
        | InputEvent::GestureHoldBegin { .. }
        | InputEvent::TouchDown { .. }
        | InputEvent::TouchMotion { .. }
        | InputEvent::TabletToolAxis { .. }
        | InputEvent::TabletToolProximity { .. }
        | InputEvent::TabletToolTip { .. }
        | InputEvent::TabletToolButton { .. } => true,
        // Ignore events like device additions and removals, key releases, gesture ends.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Action, Bind, Binds, Key, Modifiers};

    #[test]
    fn bindings_suppress_keys() {
        let close_keysym = Keysym::q;
        let bindings = Binds(vec![Bind {
            key: Key {
                keysym: close_keysym,
                modifiers: Modifiers::COMPOSITOR | Modifiers::CTRL,
            },
            actions: vec![Action::CloseWindow],
        }]);

        let comp_mod = CompositorMod::Super;
        let mut suppressed_keys = HashSet::new();

        let screenshot_ui = ScreenshotUi::new();

        // The key_code we pick is arbitrary, the only thing
        // that matters is that they are different between cases.

        let close_key_code = close_keysym.into();
        let close_key_event = |suppr: &mut HashSet<u32>, mods: ModifiersState, pressed| {
            should_intercept_key(
                suppr,
                &bindings,
                comp_mod,
                close_key_code,
                close_keysym,
                Some(close_keysym),
                pressed,
                mods,
                &screenshot_ui,
            )
        };

        // Key event with the code which can't trigger any action.
        let none_key_event = |suppr: &mut HashSet<u32>, mods: ModifiersState, pressed| {
            should_intercept_key(
                suppr,
                &bindings,
                comp_mod,
                Keysym::l.into(),
                Keysym::l,
                Some(Keysym::l),
                pressed,
                mods,
                &screenshot_ui,
            )
        };

        let mut mods = ModifiersState {
            logo: true,
            ctrl: true,
            ..Default::default()
        };

        // Action press/release.

        let filter = close_key_event(&mut suppressed_keys, mods, true);
        assert!(matches!(
            filter,
            FilterResult::Intercept(Some(Action::CloseWindow))
        ));
        assert!(suppressed_keys.contains(&close_key_code));

        let filter = close_key_event(&mut suppressed_keys, mods, false);
        assert!(matches!(filter, FilterResult::Intercept(None)));
        assert!(suppressed_keys.is_empty());

        // Remove mod to make it for a binding.

        mods.shift = true;
        let filter = close_key_event(&mut suppressed_keys, mods, true);
        assert!(matches!(filter, FilterResult::Forward));

        mods.shift = false;
        let filter = close_key_event(&mut suppressed_keys, mods, false);
        assert!(matches!(filter, FilterResult::Forward));

        // Just none press/release.

        let filter = none_key_event(&mut suppressed_keys, mods, true);
        assert!(matches!(filter, FilterResult::Forward));

        let filter = none_key_event(&mut suppressed_keys, mods, false);
        assert!(matches!(filter, FilterResult::Forward));

        // Press action, press arbitrary, release action, release arbitrary.

        let filter = close_key_event(&mut suppressed_keys, mods, true);
        assert!(matches!(
            filter,
            FilterResult::Intercept(Some(Action::CloseWindow))
        ));

        let filter = none_key_event(&mut suppressed_keys, mods, true);
        assert!(matches!(filter, FilterResult::Forward));

        let filter = close_key_event(&mut suppressed_keys, mods, false);
        assert!(matches!(filter, FilterResult::Intercept(None)));

        let filter = none_key_event(&mut suppressed_keys, mods, false);
        assert!(matches!(filter, FilterResult::Forward));

        // Trigger and remove all mods.

        let filter = close_key_event(&mut suppressed_keys, mods, true);
        assert!(matches!(
            filter,
            FilterResult::Intercept(Some(Action::CloseWindow))
        ));

        mods = Default::default();
        let filter = close_key_event(&mut suppressed_keys, mods, false);
        assert!(matches!(filter, FilterResult::Intercept(None)));

        // Ensure that no keys are being suppressed.
        assert!(suppressed_keys.is_empty());
    }
}
