use std::process::Command;

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::input::keyboard::{keysyms, FilterResult, KeysymHandle, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::utils::SERIAL_COUNTER;

use crate::niri::Niri;
use crate::utils::get_monotonic_time;

enum Action {
    None,
    Quit,
    ChangeVt(i32),
    Spawn(String),
    CloseWindow,
    ToggleFullscreen,
    FocusLeft,
    FocusRight,
    FocusDown,
    FocusUp,
    MoveLeft,
    MoveRight,
    MoveDown,
    MoveUp,
    ConsumeIntoColumn,
    ExpelFromColumn,
    SwitchWorkspaceDown,
    SwitchWorkspaceUp,
    MoveToWorkspaceDown,
    MoveToWorkspaceUp,
    ToggleWidth,
    ToggleFullWidth,
}

pub enum CompositorMod {
    Super,
    Alt,
}

impl From<Action> for FilterResult<Action> {
    fn from(value: Action) -> Self {
        match value {
            Action::None => FilterResult::Forward,
            action => FilterResult::Intercept(action),
        }
    }
}

fn action(comp_mod: CompositorMod, keysym: KeysymHandle, mods: ModifiersState) -> Action {
    use keysyms::*;

    let modified = keysym.modified_sym();
    if matches!(modified, KEY_XF86Switch_VT_1..=KEY_XF86Switch_VT_12) {
        let vt = (modified - KEY_XF86Switch_VT_1 + 1) as i32;
        return Action::ChangeVt(vt);
    }

    let mod_down = match comp_mod {
        CompositorMod::Super => mods.logo,
        CompositorMod::Alt => mods.alt,
    };

    if !mod_down {
        return Action::None;
    }

    // FIXME: these don't work in the Russian layout. I guess I'll need to
    // find a US keymap, then map keys somehow.
    #[allow(non_upper_case_globals)] // wat
    match modified {
        KEY_E => Action::Quit,
        KEY_t => Action::Spawn("alacritty".to_owned()),
        KEY_q => Action::CloseWindow,
        KEY_F => Action::ToggleFullscreen,
        KEY_h | KEY_Left if mods.ctrl => Action::MoveLeft,
        KEY_l | KEY_Right if mods.ctrl => Action::MoveRight,
        KEY_j | KEY_Down if mods.ctrl => Action::MoveDown,
        KEY_k | KEY_Up if mods.ctrl => Action::MoveUp,
        KEY_h | KEY_Left => Action::FocusLeft,
        KEY_l | KEY_Right => Action::FocusRight,
        KEY_j | KEY_Down => Action::FocusDown,
        KEY_k | KEY_Up => Action::FocusUp,
        KEY_u if mods.ctrl => Action::MoveToWorkspaceDown,
        KEY_i if mods.ctrl => Action::MoveToWorkspaceUp,
        KEY_u => Action::SwitchWorkspaceDown,
        KEY_i => Action::SwitchWorkspaceUp,
        KEY_comma => Action::ConsumeIntoColumn,
        KEY_period => Action::ExpelFromColumn,
        KEY_r => Action::ToggleWidth,
        KEY_f => Action::ToggleFullWidth,
        _ => Action::None,
    }
}

impl Niri {
    pub fn process_input_event<I: InputBackend>(
        &mut self,
        change_vt: &mut dyn FnMut(i32),
        comp_mod: CompositorMod,
        event: InputEvent<I>,
    ) {
        let _span = tracy_client::span!("process_input_event");
        trace!("process_input_event");

        // A bit of a hack, but animation end runs some logic (i.e. workspace clean-up) and it
        // doesn't always trigger due to damage, etc. So run it here right before it might prove
        // important. Besides, animations affect the input, so it's best to have up-to-date values
        // here.
        self.monitor_set.advance_animations(get_monotonic_time());

        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                let action = self.seat.get_keyboard().unwrap().input(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, mods, keysym| {
                        if event.state() == KeyState::Pressed {
                            action(comp_mod, keysym, *mods).into()
                        } else {
                            FilterResult::Forward
                        }
                    },
                );

                if let Some(action) = action {
                    match action {
                        Action::None => unreachable!(),
                        Action::Quit => {
                            info!("quitting because quit bind was pressed");
                            self.stop_signal.stop()
                        }
                        Action::ChangeVt(vt) => {
                            (*change_vt)(vt);
                        }
                        Action::Spawn(command) => {
                            if let Err(err) = Command::new(command).spawn() {
                                warn!("error spawning alacritty: {err}");
                            }
                        }
                        Action::CloseWindow => {
                            if let Some(window) = self.monitor_set.focus() {
                                window.toplevel().send_close();
                            }
                        }
                        Action::ToggleFullscreen => {
                            if let Some(window) = self.monitor_set.focus() {
                                // FIXME
                            }
                        }
                        Action::MoveLeft => {
                            self.monitor_set.move_left();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::MoveRight => {
                            self.monitor_set.move_right();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::MoveDown => {
                            self.monitor_set.move_down();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::MoveUp => {
                            self.monitor_set.move_up();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::FocusLeft => {
                            self.monitor_set.focus_left();
                        }
                        Action::FocusRight => {
                            self.monitor_set.focus_right();
                        }
                        Action::FocusDown => {
                            self.monitor_set.focus_down();
                        }
                        Action::FocusUp => {
                            self.monitor_set.focus_up();
                        }
                        Action::MoveToWorkspaceDown => {
                            self.monitor_set.move_to_workspace_down();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::MoveToWorkspaceUp => {
                            self.monitor_set.move_to_workspace_up();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::SwitchWorkspaceDown => {
                            self.monitor_set.switch_workspace_down();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::SwitchWorkspaceUp => {
                            self.monitor_set.switch_workspace_up();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::ConsumeIntoColumn => {
                            self.monitor_set.consume_into_column();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::ExpelFromColumn => {
                            self.monitor_set.expel_from_column();
                            // FIXME: granular
                            self.queue_redraw_all();
                        }
                        Action::ToggleWidth => {
                            self.monitor_set.toggle_width();
                        }
                        Action::ToggleFullWidth => {
                            self.monitor_set.toggle_full_width();
                        }
                    }
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();
                let mut pos = pointer.current_location();

                pos += event.delta();

                let mut min_x = i32::MAX;
                let mut min_y = i32::MAX;
                let mut max_x = 0;
                let mut max_y = 0;
                for output in self.global_space.outputs() {
                    // FIXME: smarter clamping.
                    let geom = self.global_space.output_geometry(output).unwrap();
                    min_x = min_x.min(geom.loc.x);
                    min_y = min_y.min(geom.loc.y);
                    max_x = max_x.max(geom.loc.x + geom.size.w);
                    max_y = max_y.max(geom.loc.y + geom.size.h);
                }

                pos.x = pos.x.clamp(min_x as f64, max_x as f64);
                pos.y = pos.y.clamp(min_y as f64, max_y as f64);

                let under = self.surface_under_and_global_space(pos);

                pointer.motion(
                    self,
                    under.clone(),
                    &MotionEvent {
                        location: pos,
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

                // Redraw to update the cursor position.
                // FIXME: redraw only outputs overlapping the cursor.
                self.queue_redraw_all();
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                // FIXME: allow mapping tablet to different outputs.
                let output = self.global_space.outputs().next().unwrap();

                let output_geo = self.global_space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under_and_global_space(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );

                // Redraw to update the cursor position.
                // FIXME: redraw only outputs overlapping the cursor.
                self.queue_redraw_all();
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some(window) = self.window_under_cursor() {
                        let window = window.clone();
                        self.monitor_set.activate_window(&window);
                    } else {
                        let output = self.output_under_cursor().unwrap();
                        self.monitor_set.activate_output(&output);
                    }
                };

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
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
                } else if source == AxisSource::Finger {
                    frame = frame.stop(Axis::Horizontal);
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.discrete(Axis::Vertical, discrete as i32);
                    }
                } else if source == AxisSource::Finger {
                    frame = frame.stop(Axis::Vertical);
                }

                self.seat.get_pointer().unwrap().axis(self, frame);
            }
            _ => {}
        }
    }
}
