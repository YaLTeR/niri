use std::cell::Cell;
use std::process::Command;

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::input::keyboard::{keysyms, FilterResult};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::shell::xdg::XdgShellHandler;

use crate::niri::Niri;

enum InputAction {
    Quit,
    ChangeVt(i32),
    SpawnTerminal,
    CloseWindow,
    ToggleFullscreen,
}

pub enum CompositorMod {
    Super,
    Alt,
}

impl Niri {
    pub fn process_input_event<I: InputBackend>(
        &mut self,
        change_vt: &mut dyn FnMut(i32),
        compositor_mod: CompositorMod,
        event: InputEvent<I>,
    ) {
        let _span = tracy_client::span!("process_input_event");
        trace!("process_input_event");

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
                            let mod_down = match compositor_mod {
                                CompositorMod::Super => mods.logo,
                                CompositorMod::Alt => mods.alt,
                            };

                            // FIXME: these don't work in the Russian layout. I guess I'll need to
                            // find a US keymap, then map keys somehow.
                            match keysym.modified_sym() {
                                keysyms::KEY_E if mod_down => {
                                    FilterResult::Intercept(InputAction::Quit)
                                }
                                keysym @ keysyms::KEY_XF86Switch_VT_1
                                    ..=keysyms::KEY_XF86Switch_VT_12 => {
                                    let vt = (keysym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32;
                                    FilterResult::Intercept(InputAction::ChangeVt(vt))
                                }
                                keysyms::KEY_t if mod_down => {
                                    FilterResult::Intercept(InputAction::SpawnTerminal)
                                }
                                keysyms::KEY_q if mod_down => {
                                    FilterResult::Intercept(InputAction::CloseWindow)
                                }
                                keysyms::KEY_f if mod_down => {
                                    FilterResult::Intercept(InputAction::ToggleFullscreen)
                                }
                                _ => FilterResult::Forward,
                            }
                        } else {
                            FilterResult::Forward
                        }
                    },
                );

                if let Some(action) = action {
                    match action {
                        InputAction::Quit => {
                            info!("quitting because quit bind was pressed");
                            self.stop_signal.stop()
                        }
                        InputAction::ChangeVt(vt) => {
                            (*change_vt)(vt);
                        }
                        InputAction::SpawnTerminal => {
                            if let Err(err) = Command::new("alacritty").spawn() {
                                warn!("error spawning alacritty: {err}");
                            }
                        }
                        InputAction::CloseWindow => {
                            if let Some(focus) = self.seat.get_keyboard().unwrap().current_focus() {
                                // FIXME: is there a better way of doing this?
                                for window in self.space.elements() {
                                    let found = Cell::new(false);
                                    window.with_surfaces(|surface, _| {
                                        if surface == &focus {
                                            found.set(true);
                                        }
                                    });
                                    if found.get() {
                                        window.toplevel().send_close();
                                        break;
                                    }
                                }
                            }
                        }
                        InputAction::ToggleFullscreen => {
                            if let Some(focus) = self.seat.get_keyboard().unwrap().current_focus() {
                                // FIXME: is there a better way of doing this?
                                let window = self.space.elements().find(|window| {
                                    let found = Cell::new(false);
                                    window.with_surfaces(|surface, _| {
                                        if surface == &focus {
                                            found.set(true);
                                        }
                                    });
                                    found.get()
                                });
                                if let Some(window) = window {
                                    let toplevel = window.toplevel().clone();
                                    if toplevel
                                        .current_state()
                                        .states
                                        .contains(xdg_toplevel::State::Fullscreen)
                                    {
                                        self.unfullscreen_request(toplevel);
                                    } else {
                                        self.fullscreen_request(toplevel, None);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();
                let mut pointer_location = pointer.current_location();

                pointer_location += event.delta();

                let output = self.space.outputs().next().unwrap();
                let output_geo = self.space.output_geometry(output).unwrap();

                pointer_location.x = pointer_location.x.clamp(0., output_geo.size.w as f64);
                pointer_location.y = pointer_location.y.clamp(0., output_geo.size.h as f64);

                let under = self.surface_under(pointer_location);
                pointer.motion(
                    self,
                    under.clone(),
                    &MotionEvent {
                        location: pointer_location,
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
                self.queue_redraw();
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under(pos);

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
                self.queue_redraw();
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some((window, _loc)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.space.raise_element(&window, true);
                        keyboard.set_focus(
                            self,
                            Some(window.toplevel().wl_surface().clone()),
                            serial,
                        );
                        self.space.elements().for_each(|window| {
                            window.toplevel().send_pending_configure();
                        });
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            window.toplevel().send_pending_configure();
                        });
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
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
