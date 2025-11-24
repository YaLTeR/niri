use std::rc::Rc;
use std::time::Duration;

use niri_config::{CenterFocusedColumn, Animation as ConfigAnimation};
use ordered_float::NotNan;
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::ScrollingSpace;
use crate::layout::LayoutElement;
use crate::layout::Options;
use super::types::{ViewOffset, ViewGesture, ScrollDirection, VIEW_GESTURE_WORKING_AREA_MOVEMENT};
use crate::input::swipe_tracker::SwipeTracker;
use super::column::Column;
use crate::animation::Animation;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn view_offset_gesture_begin(&mut self, is_touchpad: bool) {
        if self.columns.is_empty() {
            return;
        }

        if self.interactive_resize.is_some() {
            return;
        }

        let gesture = ViewGesture {
            current_view_offset: self.view_offset.current(),
            animation: None,
            tracker: SwipeTracker::new(),
            delta_from_tracker: self.view_offset.current(),
            stationary_view_offset: self.view_offset.stationary(),
            is_touchpad,
            dnd_last_event_time: None,
            dnd_nonzero_start_time: None,
        };
        self.view_offset = ViewOffset::Gesture(gesture);
    }

    pub fn dnd_scroll_gesture_begin(&mut self) {
        if let ViewOffset::Gesture(ViewGesture {
            dnd_last_event_time: Some(_),
            ..
        }) = &self.view_offset
        {
            // Already active.
            return;
        }

        let gesture = ViewGesture {
            current_view_offset: self.view_offset.current(),
            animation: None,
            tracker: SwipeTracker::new(),
            delta_from_tracker: self.view_offset.current(),
            stationary_view_offset: self.view_offset.stationary(),
            is_touchpad: false,
            dnd_last_event_time: Some(self.clock.now_unadjusted()),
            dnd_nonzero_start_time: None,
        };
        self.view_offset = ViewOffset::Gesture(gesture);

        self.interactive_resize = None;
    }

    pub fn view_offset_gesture_update(
        &mut self,
        delta_x: f64,
        timestamp: Duration,
        is_touchpad: bool,
    ) -> Option<bool> {
        let ViewOffset::Gesture(gesture) = &mut self.view_offset else {
            return None;
        };

        if gesture.is_touchpad != is_touchpad || gesture.dnd_last_event_time.is_some() {
            return None;
        }

        gesture.tracker.push(delta_x, timestamp);

        let norm_factor = if gesture.is_touchpad {
            self.working_area.size.w / VIEW_GESTURE_WORKING_AREA_MOVEMENT
        } else {
            1.
        };
        let pos = gesture.tracker.pos() * norm_factor;
        let view_offset = pos + gesture.delta_from_tracker;
        gesture.current_view_offset = view_offset;

        Some(true)
    }

    pub fn dnd_scroll_gesture_scroll(&mut self, delta: f64) -> bool {
        let ViewOffset::Gesture(gesture) = &mut self.view_offset else {
            return false;
        };

        let Some(last_time) = gesture.dnd_last_event_time else {
            // Not a DnD scroll.
            return false;
        };

        let config = &self.options.gestures.dnd_edge_view_scroll;

        let now = self.clock.now_unadjusted();
        gesture.dnd_last_event_time = Some(now);

        if delta == 0. {
            // We're outside the scrolling zone.
            gesture.dnd_nonzero_start_time = None;
            return false;
        }

        let nonzero_start = *gesture.dnd_nonzero_start_time.get_or_insert(now);

        // Delay starting the gesture a bit to avoid unwanted movement when dragging across
        // monitors.
        let delay = Duration::from_millis(u64::from(config.delay_ms));
        if now.saturating_sub(nonzero_start) < delay {
            return true;
        }

        let time_delta = now.saturating_sub(last_time).as_secs_f64();

        let delta = delta * time_delta * config.max_speed;

        gesture.tracker.push(delta, now);

        let view_offset = gesture.tracker.pos() + gesture.delta_from_tracker;

        // Clamp it so that it doesn't go too much out of bounds.
        let (leftmost, rightmost) = if self.columns.is_empty() {
            (0., 0.)
        } else {
            let gaps = self.options.layout.gaps;

            let mut leftmost = -self.working_area.size.w;

            let last_col_idx = self.columns.len() - 1;
            let last_col_x = self
                .columns
                .iter()
                .take(last_col_idx)
                .fold(0., |col_x, col| col_x + col.width() + gaps);
            let last_col_width = self.data[last_col_idx].width;
            let mut rightmost = last_col_x + last_col_width - self.working_area.loc.x;

            let active_col_x = self
                .columns
                .iter()
                .take(self.active_column_idx)
                .fold(0., |col_x, col| col_x + col.width() + gaps);
            leftmost -= active_col_x;
            rightmost -= active_col_x;

            (leftmost, rightmost)
        };
        let min_offset = f64::min(leftmost, rightmost);
        let max_offset = f64::max(leftmost, rightmost);
        let clamped_offset = view_offset.clamp(min_offset, max_offset);

        gesture.delta_from_tracker += clamped_offset - view_offset;
        gesture.current_view_offset = clamped_offset;
        true
    }

    pub fn view_offset_gesture_end(&mut self, is_touchpad: Option<bool>) -> bool {
        let ViewOffset::Gesture(gesture) = &mut self.view_offset else {
            return false;
        };

        if is_touchpad.is_some_and(|x| gesture.is_touchpad != x) {
            return false;
        }

        // We do not handle cancelling, just like GNOME Shell doesn't. For this gesture, proper
        // cancelling would require keeping track of the original active column, and then updating
        // it in all the right places (adding columns, removing columns, etc.) -- quite a bit of
        // effort and bug potential.

        // Take into account any idle time between the last event and now.
        let now = self.clock.now_unadjusted();
        gesture.tracker.push(0., now);

        let norm_factor = if gesture.is_touchpad {
            self.working_area.size.w / VIEW_GESTURE_WORKING_AREA_MOVEMENT
        } else {
            1.
        };
        let velocity = gesture.tracker.velocity() * norm_factor;
        let pos = gesture.tracker.pos() * norm_factor;
        let current_view_offset = pos + gesture.delta_from_tracker;

        if self.columns.is_empty() {
            self.view_offset = ViewOffset::Static(current_view_offset);
            return true;
        }

        // Figure out where the gesture would stop after deceleration.
        let end_pos = gesture.tracker.projected_end_pos() * norm_factor;
        let target_view_offset = end_pos + gesture.delta_from_tracker;

        // Compute the snapping points. These are where the view aligns with column boundaries on
        // either side.
        struct Snap {
            // View position relative to x = 0 (the first column).
            view_pos: f64,
            // Column to activate for this snapping point.
            col_idx: usize,
        }

        let mut snapping_points = Vec::new();

        if self.is_centering_focused_column() {
            let mut col_x = 0.;
            for (col_idx, col) in self.columns.iter().enumerate() {
                let col_w = col.width();
                let mode = col.sizing_mode();

                let area = if mode.is_maximized() {
                    self.parent_area
                } else {
                    self.working_area
                };

                let left_strut = area.loc.x;

                let view_pos = if mode.is_fullscreen() {
                    col_x
                } else if area.size.w <= col_w {
                    col_x - left_strut
                } else {
                    col_x - (area.size.w - col_w) / 2. - left_strut
                };
                snapping_points.push(Snap { view_pos, col_idx });

                col_x += col_w + self.options.layout.gaps;
            }
        } else {
            let center_on_overflow = matches!(
                self.options.layout.center_focused_column,
                CenterFocusedColumn::OnOverflow
            );

            let view_width = self.view_size.w;
            let gaps = self.options.layout.gaps;

            let snap_points =
                |col_x, col: &Column<W>, prev_col_w: Option<f64>, next_col_w: Option<f64>| {
                    let col_w = col.width();
                    let mode = col.sizing_mode();

                    let area = if mode.is_maximized() {
                        self.parent_area
                    } else {
                        self.working_area
                    };

                    let left_strut = area.loc.x;
                    let right_strut = self.view_size.w - area.size.w - area.loc.x;

                    // Normal columns align with the working area, but fullscreen columns align with
                    // the view size.
                    if mode.is_fullscreen() {
                        let left = col_x;
                        let right = left + col_w;
                        (left, right)
                    } else {
                        // Logic from compute_new_view_offset.
                        let padding = if mode.is_maximized() {
                            0.
                        } else {
                            ((area.size.w - col_w) / 2.).clamp(0., gaps)
                        };

                        let center = if area.size.w <= col_w {
                            col_x - left_strut
                        } else {
                            col_x - (area.size.w - col_w) / 2. - left_strut
                        };
                        let is_overflowing = |adj_col_w: Option<f64>| {
                            center_on_overflow
                                && adj_col_w
                                    .filter(|adj_col_w| {
                                        // NOTE: This logic won't work entirely correctly with small
                                        // fixed-size maximized windows (they have a different area
                                        // and padding).
                                        center_on_overflow
                                            && adj_col_w + 3.0 * gaps + col_w > area.size.w
                                    })
                                    .is_some()
                        };

                        let left = if is_overflowing(next_col_w) {
                            center
                        } else {
                            col_x - padding - left_strut
                        };
                        let right = if is_overflowing(prev_col_w) {
                            center + view_width
                        } else {
                            col_x + col_w + padding + right_strut
                        };
                        (left, right)
                    }
                };

            // Prevent the gesture from snapping further than the first/last column, as this is
            // generally undesired.
            //
            // It's ok if leftmost_snap is > rightmost_snap (this happens if the columns on a
            // workspace total up to less than the workspace width).

            // The first column's left snap isn't actually guaranteed to be the *leftmost* snap.
            // With weird enough left strut and perhaps a maximized small fixed-size window, you
            // can make the second window's left snap be further to the left than the first
            // window's. The same goes for the rightmost snap.
            //
            // This isn't actually a big problem because it's very much an obscure edge case. Just
            // need to make sure the code doesn't panic when that happens.
            let leftmost_snap = snap_points(
                0.,
                &self.columns[0],
                None,
                self.columns.get(1).map(|c| c.width()),
            )
            .0;
            let last_col_idx = self.columns.len() - 1;
            let last_col_x = self
                .columns
                .iter()
                .take(last_col_idx)
                .fold(0., |col_x, col| col_x + col.width() + gaps);
            let rightmost_snap = snap_points(
                last_col_x,
                &self.columns[last_col_idx],
                last_col_idx
                    .checked_sub(1)
                    .and_then(|idx| self.columns.get(idx).map(|c| c.width())),
                None,
            )
            .1 - view_width;

            snapping_points.push(Snap {
                view_pos: leftmost_snap,
                col_idx: 0,
            });
            snapping_points.push(Snap {
                view_pos: rightmost_snap,
                col_idx: last_col_idx,
            });

            let mut push = |col_idx, left, right| {
                if leftmost_snap < left && left < rightmost_snap {
                    snapping_points.push(Snap {
                        view_pos: left,
                        col_idx,
                    });
                }

                let right = right - view_width;
                if leftmost_snap < right && right < rightmost_snap {
                    snapping_points.push(Snap {
                        view_pos: right,
                        col_idx,
                    });
                }
            };

            let mut col_x = 0.;
            for (col_idx, col) in self.columns.iter().enumerate() {
                let (left, right) = snap_points(
                    col_x,
                    col,
                    col_idx
                        .checked_sub(1)
                        .and_then(|idx| self.columns.get(idx).map(|c| c.width())),
                    self.columns.get(col_idx + 1).map(|c| c.width()),
                );
                push(col_idx, left, right);

                col_x += col.width() + gaps;
            }
        }

        // Find the closest snapping point.
        snapping_points.sort_by_key(|snap| NotNan::new(snap.view_pos).unwrap());

        let active_col_x = self.column_x(self.active_column_idx);
        let target_view_pos = active_col_x + target_view_offset;
        let target_snap = snapping_points
            .iter()
            .min_by_key(|snap| NotNan::new((snap.view_pos - target_view_pos).abs()).unwrap())
            .unwrap();

        let mut new_col_idx = target_snap.col_idx;

        if !self.is_centering_focused_column() {
            // Focus the furthest window towards the direction of the gesture.
            if target_view_offset >= current_view_offset {
                for col_idx in (new_col_idx + 1)..self.columns.len() {
                    let col = &self.columns[col_idx];
                    let col_x = self.column_x(col_idx);
                    let col_w = col.width();
                    let mode = col.sizing_mode();

                    let area = if mode.is_maximized() {
                        self.parent_area
                    } else {
                        self.working_area
                    };

                    let left_strut = area.loc.x;

                    if mode.is_fullscreen() {
                        if target_snap.view_pos + self.view_size.w < col_x + col_w {
                            break;
                        }
                    } else {
                        let padding = if mode.is_maximized() {
                            0.
                        } else {
                            ((area.size.w - col_w) / 2.).clamp(0., self.options.layout.gaps)
                        };

                        if target_snap.view_pos + left_strut + area.size.w < col_x + col_w + padding
                        {
                            break;
                        }
                    }

                    new_col_idx = col_idx;
                }
            } else {
                for col_idx in (0..new_col_idx).rev() {
                    let col = &self.columns[col_idx];
                    let col_x = self.column_x(col_idx);
                    let col_w = col.width();
                    let mode = col.sizing_mode();

                    let area = if mode.is_maximized() {
                        self.parent_area
                    } else {
                        self.working_area
                    };

                    let left_strut = area.loc.x;

                    if mode.is_fullscreen() {
                        if col_x < target_snap.view_pos {
                            break;
                        }
                    } else {
                        let padding = if mode.is_maximized() {
                            0.
                        } else {
                            ((area.size.w - col_w) / 2.).clamp(0., self.options.layout.gaps)
                        };

                        if col_x - padding < target_snap.view_pos + left_strut {
                            break;
                        }
                    }

                    new_col_idx = col_idx;
                }
            }
        }

        let new_col_x = self.column_x(new_col_idx);
        let delta = active_col_x - new_col_x;

        if self.active_column_idx != new_col_idx {
            self.view_offset_to_restore = None;
        }

        self.active_column_idx = new_col_idx;

        let target_view_offset = target_snap.view_pos - new_col_x;

        self.view_offset = ViewOffset::Animation(Animation::new(
            self.clock.clone(),
            current_view_offset + delta,
            target_view_offset,
            velocity,
            self.options.animations.horizontal_view_movement.0,
        ));

        // HACK: deal with things like snapping to the right edge of a larger-than-view window.
        self.animate_view_offset_to_column(None, new_col_idx, None);

        true
    }

    pub fn dnd_scroll_gesture_end(&mut self) {
        let ViewOffset::Gesture(gesture) = &mut self.view_offset else {
            return;
        };

        if gesture.dnd_last_event_time.is_some() && gesture.tracker.pos() == 0. {
            // DnD didn't scroll anything, so preserve the current view position (rather than
            // snapping the window).
            self.view_offset = ViewOffset::Static(gesture.delta_from_tracker);

            if !self.columns.is_empty() {
                // Just in case, make sure the active window remains on screen.
                self.animate_view_offset_to_column(None, self.active_column_idx, None);
            }
            return;
        }

        self.view_offset_gesture_end(None);
    }
}
