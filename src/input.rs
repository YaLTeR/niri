use std::any::Any;
use std::collections::HashSet;
use std::time::Duration;

use input::event::gesture::GestureEventCoordinates as _;
use niri_config::{Action, Binds, Modifiers};
use niri_ipc::LayoutSwitchTarget;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Device, DeviceCapability, Event,
    GestureBeginEvent, GestureEndEvent, GesturePinchUpdateEvent as _, GestureSwipeUpdateEvent as _,
    InputBackend, InputEvent, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent, ProximityState, TabletToolButtonEvent, TabletToolEvent,
    TabletToolProximityEvent, TabletToolTipEvent, TabletToolTipState, TouchEvent,
};
use smithay::backend::libinput::LibinputInputBackend;
use smithay::input::keyboard::xkb::keysym_get_name;
use smithay::input::keyboard::{keysyms, FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorImageStatus, GestureHoldBeginEvent, GestureHoldEndEvent,
    GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
    GestureSwipeEndEvent, GestureSwipeUpdateEvent, MotionEvent, RelativeMotionEvent,
};
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotionEvent, UpEvent};
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::pointer_constraints::{with_pointer_constraint, PointerConstraint};
use smithay::wayland::tablet_manager::{TabletDescriptor, TabletSeatTrait};

use crate::niri::State;
use crate::ui::screenshot_ui::ScreenshotUi;
use crate::utils::spawning::spawn;
use crate::utils::{center, get_monotonic_time};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorMod {
    Super,
    Alt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabletData {
    pub aspect_ratio: f64,
}

impl State {
    pub fn process_input_event<I: InputBackend + 'static>(&mut self, event: InputEvent<I>)
    where
        I::Device: 'static, // Needed for downcasting.
    {
        let _span = tracy_client::span!("process_input_event");

        // A bit of a hack, but animation end runs some logic (i.e. workspace clean-up) and it
        // doesn't always trigger due to damage, etc. So run it here right before it might prove
        // important. Besides, animations affect the input, so it's best to have up-to-date values
        // here.
        self.niri.layout.advance_animations(get_monotonic_time());

        if self.niri.monitors_active {
            // Notify the idle-notifier of activity.
            if should_notify_activity(&event) {
                self.niri
                    .idle_notifier_state
                    .notify_activity(&self.niri.seat);
            }
        } else {
            // Power on monitors if they were off.
            if should_activate_monitors(&event) {
                self.niri.activate_monitors(&mut self.backend);

                // Notify the idle-notifier of activity only if we're also powering on the
                // monitors.
                self.niri
                    .idle_notifier_state
                    .notify_activity(&self.niri.seat);
            }
        }

        let hide_hotkey_overlay =
            self.niri.hotkey_overlay.is_open() && should_hide_hotkey_overlay(&event);

        let hide_exit_confirm_dialog = self
            .niri
            .exit_confirm_dialog
            .as_ref()
            .map_or(false, |d| d.is_open())
            && should_hide_exit_confirm_dialog(&event);

        use InputEvent::*;
        match event {
            DeviceAdded { device } => self.on_device_added(device),
            DeviceRemoved { device } => self.on_device_removed(device),
            Keyboard { event } => self.on_keyboard::<I>(event),
            PointerMotion { event } => self.on_pointer_motion::<I>(event),
            PointerMotionAbsolute { event } => self.on_pointer_motion_absolute::<I>(event),
            PointerButton { event } => self.on_pointer_button::<I>(event),
            PointerAxis { event } => self.on_pointer_axis::<I>(event),
            TabletToolAxis { event } => self.on_tablet_tool_axis::<I>(event),
            TabletToolTip { event } => self.on_tablet_tool_tip::<I>(event),
            TabletToolProximity { event } => self.on_tablet_tool_proximity::<I>(event),
            TabletToolButton { event } => self.on_tablet_tool_button::<I>(event),
            GestureSwipeBegin { event } => self.on_gesture_swipe_begin::<I>(event),
            GestureSwipeUpdate { event } => self.on_gesture_swipe_update::<I>(event),
            GestureSwipeEnd { event } => self.on_gesture_swipe_end::<I>(event),
            GesturePinchBegin { event } => self.on_gesture_pinch_begin::<I>(event),
            GesturePinchUpdate { event } => self.on_gesture_pinch_update::<I>(event),
            GesturePinchEnd { event } => self.on_gesture_pinch_end::<I>(event),
            GestureHoldBegin { event } => self.on_gesture_hold_begin::<I>(event),
            GestureHoldEnd { event } => self.on_gesture_hold_end::<I>(event),
            TouchDown { event } => self.on_touch_down::<I>(event),
            TouchMotion { event } => self.on_touch_motion::<I>(event),
            TouchUp { event } => self.on_touch_up::<I>(event),
            TouchCancel { event } => self.on_touch_cancel::<I>(event),
            TouchFrame { event } => self.on_touch_frame::<I>(event),
            SwitchToggle { .. } => (),
            Special(_) => (),
        }

        // Do this last so that screenshot still gets it.
        // FIXME: do this in a less cursed fashion somehow.
        if hide_hotkey_overlay && self.niri.hotkey_overlay.hide() {
            self.niri.queue_redraw_all();
        }

        if let Some(dialog) = &mut self.niri.exit_confirm_dialog {
            if hide_exit_confirm_dialog && dialog.hide() {
                self.niri.queue_redraw_all();
            }
        }
    }

    pub fn process_libinput_event(&mut self, event: &mut InputEvent<LibinputInputBackend>) {
        let _span = tracy_client::span!("process_libinput_event");

        match event {
            InputEvent::DeviceAdded { device } => {
                self.niri.devices.insert(device.clone());

                if device.has_capability(input::DeviceCapability::TabletTool) {
                    match device.size() {
                        Some((w, h)) => {
                            let aspect_ratio = w / h;
                            let data = TabletData { aspect_ratio };
                            self.niri.tablets.insert(device.clone(), data);
                        }
                        None => {
                            warn!("tablet tool device has no size");
                        }
                    }
                }

                if device.has_capability(input::DeviceCapability::Keyboard) {
                    if let Some(led_state) = self
                        .niri
                        .seat
                        .get_keyboard()
                        .map(|keyboard| keyboard.led_state())
                    {
                        device.led_update(led_state.into());
                    }
                }

                if device.has_capability(input::DeviceCapability::Touch) {
                    self.niri.touch.insert(device.clone());
                }

                apply_libinput_settings(&self.niri.config.borrow().input, device);
            }
            InputEvent::DeviceRemoved { device } => {
                self.niri.touch.remove(device);
                self.niri.tablets.remove(device);
                self.niri.devices.remove(device);
            }
            _ => (),
        }
    }

    fn on_device_added(&mut self, device: impl Device) {
        if device.has_capability(DeviceCapability::TabletTool) {
            let tablet_seat = self.niri.seat.tablet_seat();

            let desc = TabletDescriptor::from(&device);
            tablet_seat.add_tablet::<Self>(&self.niri.display_handle, &desc);
        }
        if device.has_capability(DeviceCapability::Touch) && self.niri.seat.get_touch().is_none() {
            self.niri.seat.add_touch();
        }
    }

    fn on_device_removed(&mut self, device: impl Device) {
        if device.has_capability(DeviceCapability::TabletTool) {
            let tablet_seat = self.niri.seat.tablet_seat();

            let desc = TabletDescriptor::from(&device);
            tablet_seat.remove_tablet(&desc);

            // If there are no tablets in seat we can remove all tools
            if tablet_seat.count_tablets() == 0 {
                tablet_seat.clear_tools();
            }
        }
        if device.has_capability(DeviceCapability::Touch) && self.niri.touch.is_empty() {
            self.niri.seat.remove_touch();
        }
    }

    /// Computes the cursor position for the tablet event.
    ///
    /// This function handles the tablet output mapping, as well as coordinate clamping and aspect
    /// ratio correction.
    fn compute_tablet_position<I: InputBackend>(
        &self,
        event: &(impl Event<I> + TabletToolEvent<I>),
    ) -> Option<Point<f64, Logical>>
    where
        I::Device: 'static,
    {
        let output = self.niri.output_for_tablet()?;
        let output_geo = self.niri.global_space.output_geometry(output).unwrap();

        let mut pos = event.position_transformed(output_geo.size);
        pos.x /= output_geo.size.w as f64;
        pos.y /= output_geo.size.h as f64;

        let device = event.device();
        if let Some(device) = (&device as &dyn Any).downcast_ref::<input::Device>() {
            if let Some(data) = self.niri.tablets.get(device) {
                // This code does the same thing as mutter with "keep aspect ratio" enabled.
                let output_aspect_ratio = output_geo.size.w as f64 / output_geo.size.h as f64;
                let ratio = data.aspect_ratio / output_aspect_ratio;

                if ratio > 1. {
                    pos.x *= ratio;
                } else {
                    pos.y /= ratio;
                }
            }
        };

        pos.x *= output_geo.size.w as f64;
        pos.y *= output_geo.size.h as f64;
        pos.x = pos.x.clamp(0.0, output_geo.size.w as f64 - 1.);
        pos.y = pos.y.clamp(0.0, output_geo.size.h as f64 - 1.);
        Some(pos + output_geo.loc.to_f64())
    }

    fn on_keyboard<I: InputBackend>(&mut self, event: I::KeyboardKeyEvent) {
        let comp_mod = self.backend.mod_key();

        let time = Event::time_msec(&event);
        let pressed = event.state() == KeyState::Pressed;
        let keyboard = self.niri.seat.get_keyboard().unwrap();

        // When running in the winit backend, we don't always get release events.
        // Specifically, you can reproduce this issue reliably like so:
        //
        // 1. Open niri nested in niri
        // 2. Focus nested niri
        // 3. Press any modifier key, e.g. "Super" (and do not release)
        //   -> nested niri gets a KeyState::Pressed event
        // 4. Click with the mouse cursor on another window in the top-level compositor
        // 5. Release the modifier key
        //   -> nested niri gets *no* event
        // 6. Focus nested niri again
        //
        // After you do this, xkbcommon will be in an annoying state,
        // where it reports that modifier as being permanently pressed.
        // (or at least, as far as smithay is concerned)
        // Even release events do not "unpress" this modifier.
        //
        // As such, we detect this *before* it happens, like so.
        if pressed && keyboard.pressed_keys().contains(&event.key_code().into()) {
            // Then, we insert a fake KeyState::Released event.
            keyboard
                .input(
                    self,
                    event.key_code(),
                    KeyState::Released,
                    SERIAL_COUNTER.next_serial(),
                    time,
                    |_, _, keysym| {
                        debug!(
                            "Duplicate keypress of `{key}`. A fake release event was generated.",
                            key = keysym_get_name(keysym.modified_sym())
                        );
                        // And lastly, we tell smithay to keep its mouth shut about this.
                        FilterResult::Intercept(())
                    },
                )
                .unwrap();
        }

        let Some(Some(action)) = keyboard.input(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            time,
            |this, mods, keysym| {
                let bindings = &this.niri.config.borrow().binds;
                let key_code = event.key_code();
                let modified = keysym.modified_sym();
                let raw = keysym.raw_latin_sym_or_raw_current_sym();

                if let Some(dialog) = &this.niri.exit_confirm_dialog {
                    if dialog.is_open() && pressed && raw == Some(Keysym::Return) {
                        info!("quitting after confirming exit dialog");
                        this.niri.stop_signal.stop();
                    }
                }

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
                    this.niri.config.borrow().input.disable_power_key_handling,
                )
            },
        ) else {
            return;
        };

        // Filter actions when the key is released or the session is locked.
        if !pressed {
            return;
        }

        self.do_action(action);
    }

    pub fn do_action(&mut self, action: Action) {
        if self.niri.is_locked() && !allowed_when_locked(&action) {
            return;
        }

        if let Some(touch) = self.niri.seat.get_touch() {
            touch.cancel(self);
        }

        match action {
            Action::Quit(skip_confirmation) => {
                if !skip_confirmation {
                    if let Some(dialog) = &mut self.niri.exit_confirm_dialog {
                        if dialog.show() {
                            self.niri.queue_redraw_all();
                        }
                        return;
                    }
                }

                info!("quitting as requested");
                self.niri.stop_signal.stop()
            }
            Action::ChangeVt(vt) => {
                self.backend.change_vt(vt);
                // Changing VT may not deliver the key releases, so clear the state.
                self.niri.suppressed_keys.clear();
            }
            Action::Suspend => {
                self.backend.suspend();
                // Suspend may not deliver the key releases, so clear the state.
                self.niri.suppressed_keys.clear();
            }
            Action::PowerOffMonitors => {
                self.niri.deactivate_monitors(&mut self.backend);
            }
            Action::ToggleDebugTint => {
                self.backend.toggle_debug_tint();
                self.niri.queue_redraw_all();
            }
            Action::Spawn(command) => {
                spawn(command);
            }
            Action::ScreenshotScreen => {
                let active = self.niri.layout.active_output().cloned();
                if let Some(active) = active {
                    self.backend.with_primary_renderer(|renderer| {
                        if let Err(err) = self.niri.screenshot(renderer, &active) {
                            warn!("error taking screenshot: {err:?}");
                        }
                    });
                }
            }
            Action::ConfirmScreenshot => {
                self.backend.with_primary_renderer(|renderer| {
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
                });

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
                self.backend.with_primary_renderer(|renderer| {
                    self.niri.open_screenshot_ui(renderer);
                });
            }
            Action::ScreenshotWindow => {
                let active = self.niri.layout.active_window();
                if let Some((window, output)) = active {
                    self.backend.with_primary_renderer(|renderer| {
                        if let Err(err) = self.niri.screenshot_window(renderer, output, window) {
                            warn!("error taking screenshot: {err:?}");
                        }
                    });
                }
            }
            Action::CloseWindow => {
                if let Some(window) = self.niri.layout.focus() {
                    window.toplevel().expect("no x11 support").send_close();
                }
            }
            Action::FullscreenWindow => {
                let focus = self.niri.layout.focus().cloned();
                if let Some(window) = focus {
                    self.niri.layout.toggle_fullscreen(&window);
                    // FIXME: granular
                    self.niri.queue_redraw_all();
                }
            }
            Action::SwitchLayout(action) => {
                self.niri.seat.get_keyboard().unwrap().with_xkb_state(
                    self,
                    |mut state| match action {
                        LayoutSwitchTarget::Next => state.cycle_next_layout(),
                        LayoutSwitchTarget::Prev => state.cycle_prev_layout(),
                    },
                );
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
            Action::MoveColumnToFirst => {
                self.niri.layout.move_column_to_first();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::MoveColumnToLast => {
                self.niri.layout.move_column_to_last();
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
            Action::MoveWindowDownOrToWorkspaceDown => {
                self.niri.layout.move_down_or_to_workspace_down();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::MoveWindowUpOrToWorkspaceUp => {
                self.niri.layout.move_up_or_to_workspace_up();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::ConsumeOrExpelWindowLeft => {
                self.niri.layout.consume_or_expel_window_left();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::ConsumeOrExpelWindowRight => {
                self.niri.layout.consume_or_expel_window_right();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusColumnLeft => {
                self.niri.layout.focus_left();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusColumnRight => {
                self.niri.layout.focus_right();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusColumnFirst => {
                self.niri.layout.focus_column_first();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusColumnLast => {
                self.niri.layout.focus_column_last();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusWindowDown => {
                self.niri.layout.focus_down();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusWindowUp => {
                self.niri.layout.focus_up();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusWindowOrWorkspaceDown => {
                self.niri.layout.focus_window_or_workspace_down();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::FocusWindowOrWorkspaceUp => {
                self.niri.layout.focus_window_or_workspace_up();
                // FIXME: granular
                self.niri.queue_redraw_all();
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
                let idx = idx.saturating_sub(1) as usize;
                self.niri.layout.move_to_workspace(idx);
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::MoveColumnToWorkspaceDown => {
                self.niri.layout.move_column_to_workspace_down();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::MoveColumnToWorkspaceUp => {
                self.niri.layout.move_column_to_workspace_up();
                // FIXME: granular
                self.niri.queue_redraw_all();
            }
            Action::MoveColumnToWorkspace(idx) => {
                let idx = idx.saturating_sub(1) as usize;
                self.niri.layout.move_column_to_workspace(idx);
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
                let idx = idx.saturating_sub(1) as usize;
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
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveWindowToMonitorRight => {
                if let Some(output) = self.niri.output_right() {
                    self.niri.layout.move_to_output(&output);
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveWindowToMonitorDown => {
                if let Some(output) = self.niri.output_down() {
                    self.niri.layout.move_to_output(&output);
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveWindowToMonitorUp => {
                if let Some(output) = self.niri.output_up() {
                    self.niri.layout.move_to_output(&output);
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveColumnToMonitorLeft => {
                if let Some(output) = self.niri.output_left() {
                    self.niri.layout.move_column_to_output(&output);
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveColumnToMonitorRight => {
                if let Some(output) = self.niri.output_right() {
                    self.niri.layout.move_column_to_output(&output);
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveColumnToMonitorDown => {
                if let Some(output) = self.niri.output_down() {
                    self.niri.layout.move_column_to_output(&output);
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveColumnToMonitorUp => {
                if let Some(output) = self.niri.output_up() {
                    self.niri.layout.move_column_to_output(&output);
                    self.niri.layout.focus_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::SetColumnWidth(change) => {
                self.niri.layout.set_column_width(change);
            }
            Action::SetWindowHeight(change) => {
                self.niri.layout.set_window_height(change);
            }
            Action::ShowHotkeyOverlay => {
                if self.niri.hotkey_overlay.show() {
                    self.niri.queue_redraw_all();
                }
            }
            Action::MoveWorkspaceToMonitorLeft => {
                if let Some(output) = self.niri.output_left() {
                    self.niri.layout.move_workspace_to_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveWorkspaceToMonitorRight => {
                if let Some(output) = self.niri.output_right() {
                    self.niri.layout.move_workspace_to_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveWorkspaceToMonitorDown => {
                if let Some(output) = self.niri.output_down() {
                    self.niri.layout.move_workspace_to_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
            Action::MoveWorkspaceToMonitorUp => {
                if let Some(output) = self.niri.output_up() {
                    self.niri.layout.move_workspace_to_output(&output);
                    self.move_cursor_to_output(&output);
                }
            }
        }
    }

    fn on_pointer_motion<I: InputBackend>(&mut self, event: I::PointerMotionEvent) {
        // We need an output to be able to move the pointer.
        if self.niri.global_space.outputs().next().is_none() {
            return;
        }

        let serial = SERIAL_COUNTER.next_serial();

        let pointer = self.niri.seat.get_pointer().unwrap();

        let pos = pointer.current_location();

        // We have an output, so we can compute the new location and focus.
        let mut new_pos = pos + event.delta();

        // We received an event for the regular pointer, so show it now.
        self.niri.tablet_cursor_location = None;

        // Check if we have an active pointer constraint.
        let mut pointer_confined = None;
        if let Some(focus) = self.niri.pointer_focus.as_ref() {
            let focus_surface_loc = focus.surface.1;
            let pos_within_surface = pos.to_i32_round() - focus_surface_loc;

            let mut pointer_locked = false;
            with_pointer_constraint(&focus.surface.0, &pointer, |constraint| {
                let Some(constraint) = constraint else { return };
                if !constraint.is_active() {
                    return;
                }

                // Constraint does not apply if not within region.
                if let Some(region) = constraint.region() {
                    if !region.contains(pos_within_surface) {
                        return;
                    }
                }

                match &*constraint {
                    PointerConstraint::Locked(_locked) => {
                        pointer_locked = true;
                    }
                    PointerConstraint::Confined(confine) => {
                        pointer_confined = Some((focus.surface.clone(), confine.region().cloned()));
                    }
                }
            });

            // If the pointer is locked, only send relative motion.
            if pointer_locked {
                pointer.relative_motion(
                    self,
                    Some(focus.surface.clone()),
                    &RelativeMotionEvent {
                        delta: event.delta(),
                        delta_unaccel: event.delta_unaccel(),
                        utime: event.time(),
                    },
                );

                pointer.frame(self);

                // I guess a redraw to hide the tablet cursor could be nice? Doesn't matter too
                // much here I think.
                return;
            }
        }

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

        // Handle confined pointer.
        if let Some((focus_surface, region)) = pointer_confined {
            let mut prevent = false;

            // Prevent the pointer from leaving the focused surface.
            if Some(&focus_surface.0) != under.as_ref().map(|x| &x.surface.0) {
                prevent = true;
            }

            // Prevent the pointer from leaving the confine region, if any.
            if let Some(region) = region {
                let new_pos_within_surface = new_pos.to_i32_round() - focus_surface.1;
                if !region.contains(new_pos_within_surface) {
                    prevent = true;
                }
            }

            if prevent {
                pointer.relative_motion(
                    self,
                    Some(focus_surface),
                    &RelativeMotionEvent {
                        delta: event.delta(),
                        delta_unaccel: event.delta_unaccel(),
                        utime: event.time(),
                    },
                );

                pointer.frame(self);

                return;
            }
        }

        // Activate a new confinement if necessary.
        self.niri.maybe_activate_pointer_constraint(new_pos, &under);

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

    fn on_pointer_motion_absolute<I: InputBackend>(
        &mut self,
        event: I::PointerMotionAbsoluteEvent,
    ) {
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
        self.niri.maybe_activate_pointer_constraint(pos, &under);
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

        // We moved the regular pointer, so show it now.
        self.niri.tablet_cursor_location = None;

        // Redraw to update the cursor position.
        // FIXME: redraw only outputs overlapping the cursor.
        self.niri.queue_redraw_all();
    }

    fn on_pointer_button<I: InputBackend>(&mut self, event: I::PointerButtonEvent) {
        let pointer = self.niri.seat.get_pointer().unwrap();

        let serial = SERIAL_COUNTER.next_serial();

        let button = event.button_code();

        let button_state = event.state();

        if ButtonState::Pressed == button_state {
            if let Some(window) = self.niri.window_under_cursor() {
                let window = window.clone();
                self.niri.layout.activate_window(&window);

                // FIXME: granular.
                self.niri.queue_redraw_all();
            } else if let Some(output) = self.niri.output_under_cursor() {
                self.niri.layout.activate_output(&output);

                // FIXME: granular.
                self.niri.queue_redraw_all();
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
                if self
                    .niri
                    .screenshot_ui
                    .pointer_button(output, point, button, button_state)
                {
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

    fn on_pointer_axis<I: InputBackend>(&mut self, event: I::PointerAxisEvent) {
        let source = event.source();

        let horizontal_amount = event
            .amount(Axis::Horizontal)
            .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 3.0 / 120.);
        let vertical_amount = event
            .amount(Axis::Vertical)
            .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 3.0 / 120.);
        let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
        let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

        let mut frame = AxisFrame::new(event.time_msec()).source(source);
        if horizontal_amount != 0.0 {
            frame = frame
                .relative_direction(Axis::Horizontal, event.relative_direction(Axis::Horizontal));
            frame = frame.value(Axis::Horizontal, horizontal_amount);
            if let Some(discrete) = horizontal_amount_discrete {
                frame = frame.v120(Axis::Horizontal, discrete as i32);
            }
        }
        if vertical_amount != 0.0 {
            frame =
                frame.relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical));
            frame = frame.value(Axis::Vertical, vertical_amount);
            if let Some(discrete) = vertical_amount_discrete {
                frame = frame.v120(Axis::Vertical, discrete as i32);
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

    fn on_tablet_tool_axis<I: InputBackend>(&mut self, event: I::TabletToolAxisEvent)
    where
        I::Device: 'static, // Needed for downcasting.
    {
        let Some(pos) = self.compute_tablet_position(&event) else {
            return;
        };

        let under = self.niri.surface_under_and_global_space(pos);
        let under = under.map(|u| u.surface);

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

            self.niri.tablet_cursor_location = Some(pos);
        }

        // Redraw to update the cursor position.
        // FIXME: redraw only outputs overlapping the cursor.
        self.niri.queue_redraw_all();
    }

    fn on_tablet_tool_tip<I: InputBackend>(&mut self, event: I::TabletToolTipEvent) {
        let tool = self.niri.seat.tablet_seat().get_tool(&event.tool());

        if let Some(tool) = tool {
            match event.tip_state() {
                TabletToolTipState::Down => {
                    let serial = SERIAL_COUNTER.next_serial();
                    tool.tip_down(serial, event.time_msec());

                    if let Some(pos) = self.niri.tablet_cursor_location {
                        if let Some(window) = self.niri.window_under(pos) {
                            let window = window.clone();
                            self.niri.layout.activate_window(&window);

                            // FIXME: granular.
                            self.niri.queue_redraw_all();
                        } else if let Some((output, _)) = self.niri.output_under(pos) {
                            let output = output.clone();
                            self.niri.layout.activate_output(&output);

                            // FIXME: granular.
                            self.niri.queue_redraw_all();
                        }
                    }
                }
                TabletToolTipState::Up => {
                    tool.tip_up(event.time_msec());
                }
            }
        }
    }

    fn on_tablet_tool_proximity<I: InputBackend>(&mut self, event: I::TabletToolProximityEvent)
    where
        I::Device: 'static, // Needed for downcasting.
    {
        let Some(pos) = self.compute_tablet_position(&event) else {
            return;
        };

        let under = self.niri.surface_under_and_global_space(pos);
        let under = under.map(|u| u.surface);

        let tablet_seat = self.niri.seat.tablet_seat();
        let tool = tablet_seat.add_tool::<Self>(&self.niri.display_handle, &event.tool());
        let tablet = tablet_seat.get_tablet(&TabletDescriptor::from(&event.device()));
        if let Some(tablet) = tablet {
            match event.state() {
                ProximityState::In => {
                    if let Some(under) = under {
                        tool.proximity_in(
                            pos,
                            under,
                            &tablet,
                            SERIAL_COUNTER.next_serial(),
                            event.time_msec(),
                        );
                    }
                    self.niri.tablet_cursor_location = Some(pos);
                }
                ProximityState::Out => {
                    tool.proximity_out(event.time_msec());

                    // Move the mouse pointer here to avoid discontinuity.
                    //
                    // Plus, Wayland SDL2 currently warps the pointer into some weird
                    // location on proximity out, so this shuold help it a little.
                    if let Some(pos) = self.niri.tablet_cursor_location {
                        self.move_cursor(pos);
                    }

                    self.niri.tablet_cursor_location = None;
                }
            }

            // FIXME: granular.
            self.niri.queue_redraw_all();
        }
    }

    fn on_tablet_tool_button<I: InputBackend>(&mut self, event: I::TabletToolButtonEvent) {
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

    fn on_gesture_swipe_begin<I: InputBackend>(&mut self, event: I::GestureSwipeBeginEvent) {
        if event.fingers() == 3 {
            self.niri.gesture_swipe_3f_cumulative = Some((0., 0.));

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

    fn on_gesture_swipe_update<I: InputBackend + 'static>(
        &mut self,
        event: I::GestureSwipeUpdateEvent,
    ) where
        I::Device: 'static,
    {
        let mut delta_x = event.delta_x();
        let mut delta_y = event.delta_y();

        if let Some(libinput_event) =
            (&event as &dyn Any).downcast_ref::<input::event::gesture::GestureSwipeUpdateEvent>()
        {
            delta_x = libinput_event.dx_unaccelerated();
            delta_y = libinput_event.dy_unaccelerated();
        }

        let device = event.device();
        if let Some(device) = (&device as &dyn Any).downcast_ref::<input::Device>() {
            if device.config_scroll_natural_scroll_enabled() {
                delta_x = -delta_x;
                delta_y = -delta_y;
            }
        }

        if let Some((cx, cy)) = &mut self.niri.gesture_swipe_3f_cumulative {
            *cx += delta_x;
            *cy += delta_y;

            // Check if the gesture moved far enough to decide. Threshold copied from GNOME Shell.
            let (cx, cy) = (*cx, *cy);
            if cx * cx + cy * cy >= 16. * 16. {
                self.niri.gesture_swipe_3f_cumulative = None;

                if let Some(output) = self.niri.output_under_cursor() {
                    if cx.abs() > cy.abs() {
                        self.niri.layout.view_offset_gesture_begin(&output);
                    } else {
                        self.niri.layout.workspace_switch_gesture_begin(&output);
                    }
                }
            }
        }

        let timestamp = Duration::from_micros(event.time());

        let mut handled = false;
        let res = self
            .niri
            .layout
            .workspace_switch_gesture_update(delta_y, timestamp);
        if let Some(output) = res {
            if let Some(output) = output {
                self.niri.queue_redraw(output);
            }
            handled = true;
        }

        let res = self
            .niri
            .layout
            .view_offset_gesture_update(delta_x, timestamp);
        if let Some(output) = res {
            if let Some(output) = output {
                self.niri.queue_redraw(output);
            }
            handled = true;
        }

        if handled {
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

    fn on_gesture_swipe_end<I: InputBackend>(&mut self, event: I::GestureSwipeEndEvent) {
        self.niri.gesture_swipe_3f_cumulative = None;

        let mut handled = false;
        let res = self
            .niri
            .layout
            .workspace_switch_gesture_end(event.cancelled());
        if let Some(output) = res {
            self.niri.queue_redraw(output);
            handled = true;
        }

        let res = self.niri.layout.view_offset_gesture_end(event.cancelled());
        if let Some(output) = res {
            self.niri.queue_redraw(output);
            handled = true;
        }

        if handled {
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

    fn on_gesture_pinch_begin<I: InputBackend>(&mut self, event: I::GesturePinchBeginEvent) {
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

    fn on_gesture_pinch_update<I: InputBackend>(&mut self, event: I::GesturePinchUpdateEvent) {
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

    fn on_gesture_pinch_end<I: InputBackend>(&mut self, event: I::GesturePinchEndEvent) {
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

    fn on_gesture_hold_begin<I: InputBackend>(&mut self, event: I::GestureHoldBeginEvent) {
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

    fn on_gesture_hold_end<I: InputBackend>(&mut self, event: I::GestureHoldEndEvent) {
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

    /// Computes the cursor position for the touch event.
    ///
    /// This function handles the touch output mapping, as well as coordinate transform
    fn compute_touch_location<I: InputBackend, E: AbsolutePositionEvent<I>>(
        &self,
        evt: &E,
    ) -> Option<Point<f64, Logical>> {
        let output = self.niri.output_for_touch()?;
        let output_geo = self.niri.global_space.output_geometry(output).unwrap();
        let transform = output.current_transform();
        let size = transform.invert().transform_size(output_geo.size);
        Some(
            transform.transform_point_in(evt.position_transformed(size), &size.to_f64())
                + output_geo.loc.to_f64(),
        )
    }

    fn on_touch_down<I: InputBackend>(&mut self, evt: I::TouchDownEvent) {
        let Some(handle) = self.niri.seat.get_touch() else {
            return;
        };
        let Some(touch_location) = self.compute_touch_location(&evt) else {
            return;
        };

        if !handle.is_grabbed() {
            let output_under_touch = self
                .niri
                .global_space
                .output_under(touch_location)
                .next()
                .cloned();
            if let Some(window) = self.niri.window_under(touch_location) {
                let window = window.clone();
                self.niri.layout.activate_window(&window);

                // FIXME: granular.
                self.niri.queue_redraw_all();
            } else if let Some(output) = output_under_touch {
                self.niri.layout.activate_output(&output);

                // FIXME: granular.
                self.niri.queue_redraw_all();
            };
        };

        let serial = SERIAL_COUNTER.next_serial();
        let under = self
            .niri
            .surface_under_and_global_space(touch_location)
            .map(|under| under.surface);
        handle.down(
            self,
            under,
            &DownEvent {
                slot: evt.slot(),
                location: touch_location,
                serial,
                time: evt.time_msec(),
            },
        );
    }
    fn on_touch_up<I: InputBackend>(&mut self, evt: I::TouchUpEvent) {
        let Some(handle) = self.niri.seat.get_touch() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        handle.up(
            self,
            &UpEvent {
                slot: evt.slot(),
                serial,
                time: evt.time_msec(),
            },
        )
    }
    fn on_touch_motion<I: InputBackend>(&mut self, evt: I::TouchMotionEvent) {
        let Some(handle) = self.niri.seat.get_touch() else {
            return;
        };
        let Some(touch_location) = self.compute_touch_location(&evt) else {
            return;
        };
        let under = self
            .niri
            .surface_under_and_global_space(touch_location)
            .map(|under| under.surface);
        handle.motion(
            self,
            under,
            &TouchMotionEvent {
                slot: evt.slot(),
                location: touch_location,
                time: evt.time_msec(),
            },
        );
    }
    fn on_touch_frame<I: InputBackend>(&mut self, _evt: I::TouchFrameEvent) {
        let Some(handle) = self.niri.seat.get_touch() else {
            return;
        };
        handle.frame(self);
    }
    fn on_touch_cancel<I: InputBackend>(&mut self, _evt: I::TouchCancelEvent) {
        let Some(handle) = self.niri.seat.get_touch() else {
            return;
        };
        handle.cancel(self);
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
    disable_power_key_handling: bool,
) -> FilterResult<Option<Action>> {
    // Actions are only triggered on presses, release of the key
    // shouldn't try to intercept anything unless we have marked
    // the key to suppress.
    if !pressed && !suppressed_keys.contains(&key_code) {
        return FilterResult::Forward;
    }

    let mut final_action = action(
        bindings,
        comp_mod,
        modified,
        raw,
        mods,
        disable_power_key_handling,
    );

    // Allow only a subset of compositor actions while the screenshot UI is open, since the user
    // cannot see the screen.
    if screenshot_ui.is_open() {
        let mut use_screenshot_ui_action = true;

        if let Some(action) = &final_action {
            if allowed_during_screenshot(action) {
                use_screenshot_ui_action = false;
            }
        }

        if use_screenshot_ui_action {
            final_action = screenshot_ui.action(raw, mods);
        }
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
    disable_power_key_handling: bool,
) -> Option<Action> {
    use keysyms::*;

    // Handle hardcoded binds.
    #[allow(non_upper_case_globals)] // wat
    match modified.raw() {
        modified @ KEY_XF86Switch_VT_1..=KEY_XF86Switch_VT_12 => {
            let vt = (modified - KEY_XF86Switch_VT_1 + 1) as i32;
            return Some(Action::ChangeVt(vt));
        }
        KEY_XF86PowerOff if !disable_power_key_handling => return Some(Action::Suspend),
        _ => (),
    }

    bound_action(bindings, comp_mod, raw, mods)
}

fn bound_action(
    bindings: &Binds,
    comp_mod: CompositorMod,
    raw: Option<Keysym>,
    mods: ModifiersState,
) -> Option<Action> {
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

    let (mod_down, comp_mod) = match comp_mod {
        CompositorMod::Super => (mods.logo, Modifiers::SUPER),
        CompositorMod::Alt => (mods.alt, Modifiers::ALT),
    };
    if mod_down {
        modifiers |= Modifiers::COMPOSITOR;
    }

    let raw = raw?;

    for bind in &bindings.0 {
        if bind.key.keysym != raw {
            continue;
        }

        let mut bind_modifiers = bind.key.modifiers;
        if bind_modifiers.contains(Modifiers::COMPOSITOR) {
            bind_modifiers |= comp_mod;
        } else if bind_modifiers.contains(comp_mod) {
            bind_modifiers |= Modifiers::COMPOSITOR;
        }

        if bind_modifiers == modifiers {
            return Some(bind.action.clone());
        }
    }

    None
}

fn should_activate_monitors<I: InputBackend>(event: &InputEvent<I>) -> bool {
    match event {
        InputEvent::Keyboard { event } if event.state() == KeyState::Pressed => true,
        InputEvent::PointerButton { event } if event.state() == ButtonState::Pressed => true,
        InputEvent::PointerMotion { .. }
        | InputEvent::PointerMotionAbsolute { .. }
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

fn should_hide_hotkey_overlay<I: InputBackend>(event: &InputEvent<I>) -> bool {
    match event {
        InputEvent::Keyboard { event } if event.state() == KeyState::Pressed => true,
        InputEvent::PointerButton { event } if event.state() == ButtonState::Pressed => true,
        InputEvent::PointerAxis { .. }
        | InputEvent::GestureSwipeBegin { .. }
        | InputEvent::GesturePinchBegin { .. }
        | InputEvent::TouchDown { .. }
        | InputEvent::TouchMotion { .. }
        | InputEvent::TabletToolTip { .. }
        | InputEvent::TabletToolButton { .. } => true,
        _ => false,
    }
}

fn should_hide_exit_confirm_dialog<I: InputBackend>(event: &InputEvent<I>) -> bool {
    match event {
        InputEvent::Keyboard { event } if event.state() == KeyState::Pressed => true,
        InputEvent::PointerButton { event } if event.state() == ButtonState::Pressed => true,
        InputEvent::PointerAxis { .. }
        | InputEvent::GestureSwipeBegin { .. }
        | InputEvent::GesturePinchBegin { .. }
        | InputEvent::TouchDown { .. }
        | InputEvent::TouchMotion { .. }
        | InputEvent::TabletToolTip { .. }
        | InputEvent::TabletToolButton { .. } => true,
        _ => false,
    }
}

fn should_notify_activity<I: InputBackend>(event: &InputEvent<I>) -> bool {
    !matches!(
        event,
        InputEvent::DeviceAdded { .. } | InputEvent::DeviceRemoved { .. }
    )
}

fn allowed_when_locked(action: &Action) -> bool {
    matches!(
        action,
        Action::Quit(_)
            | Action::ChangeVt(_)
            | Action::Suspend
            | Action::PowerOffMonitors
            | Action::SwitchLayout(_)
    )
}

fn allowed_during_screenshot(action: &Action) -> bool {
    matches!(
        action,
        Action::Quit(_) | Action::ChangeVt(_) | Action::Suspend | Action::PowerOffMonitors
    )
}

pub fn apply_libinput_settings(config: &niri_config::Input, device: &mut input::Device) {
    // According to Mutter code, this setting is specific to touchpads.
    let is_touchpad = device.config_tap_finger_count() > 0;
    if is_touchpad {
        let c = &config.touchpad;
        let _ = device.config_tap_set_enabled(c.tap);
        let _ = device.config_dwt_set_enabled(c.dwt);
        let _ = device.config_dwtp_set_enabled(c.dwtp);
        let _ = device.config_scroll_set_natural_scroll_enabled(c.natural_scroll);
        let _ = device.config_accel_set_speed(c.accel_speed);

        if let Some(accel_profile) = c.accel_profile {
            let _ = device.config_accel_set_profile(accel_profile.into());
        } else if let Some(default) = device.config_accel_default_profile() {
            let _ = device.config_accel_set_profile(default);
        }

        if let Some(tap_button_map) = c.tap_button_map {
            let _ = device.config_tap_set_button_map(tap_button_map.into());
        } else if let Some(default) = device.config_tap_default_button_map() {
            let _ = device.config_tap_set_button_map(default);
        }
    }

    // This is how Mutter tells apart mice.
    let mut is_trackball = false;
    let mut is_trackpoint = false;
    if let Some(udev_device) = unsafe { device.udev_device() } {
        if udev_device.property_value("ID_INPUT_TRACKBALL").is_some() {
            is_trackball = true;
        }
        if udev_device
            .property_value("ID_INPUT_POINTINGSTICK")
            .is_some()
        {
            is_trackpoint = true;
        }
    }

    let is_mouse = device.has_capability(input::DeviceCapability::Pointer)
        && !is_touchpad
        && !is_trackball
        && !is_trackpoint;
    if is_mouse {
        let c = &config.mouse;
        let _ = device.config_scroll_set_natural_scroll_enabled(c.natural_scroll);
        let _ = device.config_accel_set_speed(c.accel_speed);

        if let Some(accel_profile) = c.accel_profile {
            let _ = device.config_accel_set_profile(accel_profile.into());
        } else if let Some(default) = device.config_accel_default_profile() {
            let _ = device.config_accel_set_profile(default);
        }
    }

    if is_trackpoint {
        let c = &config.trackpoint;
        let _ = device.config_scroll_set_natural_scroll_enabled(c.natural_scroll);
        let _ = device.config_accel_set_speed(c.accel_speed);

        if let Some(accel_profile) = c.accel_profile {
            let _ = device.config_accel_set_profile(accel_profile.into());
        } else if let Some(default) = device.config_accel_default_profile() {
            let _ = device.config_accel_set_profile(default);
        }
    }
}

#[cfg(test)]
mod tests {
    use niri_config::{Bind, Key};

    use super::*;

    #[test]
    fn bindings_suppress_keys() {
        let close_keysym = Keysym::q;
        let bindings = Binds(vec![Bind {
            key: Key {
                keysym: close_keysym,
                modifiers: Modifiers::COMPOSITOR | Modifiers::CTRL,
            },
            action: Action::CloseWindow,
        }]);

        let comp_mod = CompositorMod::Super;
        let mut suppressed_keys = HashSet::new();

        let screenshot_ui = ScreenshotUi::new();
        let disable_power_key_handling = false;

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
                disable_power_key_handling,
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
                disable_power_key_handling,
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

    #[test]
    fn comp_mod_handling() {
        let bindings = Binds(vec![
            Bind {
                key: Key {
                    keysym: Keysym::q,
                    modifiers: Modifiers::COMPOSITOR,
                },
                action: Action::CloseWindow,
            },
            Bind {
                key: Key {
                    keysym: Keysym::h,
                    modifiers: Modifiers::SUPER,
                },
                action: Action::FocusColumnLeft,
            },
            Bind {
                key: Key {
                    keysym: Keysym::j,
                    modifiers: Modifiers::empty(),
                },
                action: Action::FocusWindowDown,
            },
            Bind {
                key: Key {
                    keysym: Keysym::k,
                    modifiers: Modifiers::COMPOSITOR | Modifiers::SUPER,
                },
                action: Action::FocusWindowUp,
            },
            Bind {
                key: Key {
                    keysym: Keysym::l,
                    modifiers: Modifiers::SUPER | Modifiers::ALT,
                },
                action: Action::FocusColumnRight,
            },
        ]);

        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::q),
                ModifiersState {
                    logo: true,
                    ..Default::default()
                }
            ),
            Some(Action::CloseWindow)
        );
        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::q),
                ModifiersState::default(),
            ),
            None,
        );

        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::h),
                ModifiersState {
                    logo: true,
                    ..Default::default()
                }
            ),
            Some(Action::FocusColumnLeft)
        );
        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::h),
                ModifiersState::default(),
            ),
            None,
        );

        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::j),
                ModifiersState {
                    logo: true,
                    ..Default::default()
                }
            ),
            None,
        );
        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::j),
                ModifiersState::default(),
            ),
            Some(Action::FocusWindowDown)
        );

        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::k),
                ModifiersState {
                    logo: true,
                    ..Default::default()
                }
            ),
            Some(Action::FocusWindowUp)
        );
        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::k),
                ModifiersState::default(),
            ),
            None,
        );

        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::l),
                ModifiersState {
                    logo: true,
                    alt: true,
                    ..Default::default()
                }
            ),
            Some(Action::FocusColumnRight)
        );
        assert_eq!(
            bound_action(
                &bindings,
                CompositorMod::Super,
                Some(Keysym::l),
                ModifiersState {
                    logo: true,
                    ..Default::default()
                },
            ),
            None,
        );
    }
}
