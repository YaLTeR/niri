use niri_config::{CenterFocusedColumn, Animation};

use super::ScrollingSpace;
use crate::layout::{LayoutElement, SizingMode};
use super::super::ViewOffset;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn is_centering_focused_column(&self) -> bool {
        self.options.layout.center_focused_column == CenterFocusedColumn::Always
            || (self.options.layout.always_center_single_column && self.columns.len() <= 1)
    }

    pub(in crate::layout::scrolling) fn compute_new_view_offset_fit(
        &self,
        target_x: Option<f64>,
        col_x: f64,
        width: f64,
        mode: SizingMode,
    ) -> f64 {
        if mode.is_fullscreen() {
            return 0.;
        }

        let (area, padding) = if mode.is_maximized() {
            (self.parent_area, 0.)
        } else {
            (self.working_area, self.options.layout.gaps)
        };

        let target_x = target_x.unwrap_or_else(|| self.target_view_pos());

        let new_offset =
            super::super::utils::compute_new_view_offset(target_x + area.loc.x, area.size.w, col_x, width, padding);

        // Non-fullscreen windows are always offset at least by the working area position.
        new_offset - area.loc.x
    }

    pub(in crate::layout::scrolling) fn compute_new_view_offset_centered(
        &self,
        target_x: Option<f64>,
        col_x: f64,
        width: f64,
        mode: SizingMode,
    ) -> f64 {
        if mode.is_fullscreen() {
            return self.compute_new_view_offset_fit(target_x, col_x, width, mode);
        }

        let area = if mode.is_maximized() {
            self.parent_area
        } else {
            self.working_area
        };

        // Columns wider than the view are left-aligned (the fit code can deal with that).
        if area.size.w <= width {
            return self.compute_new_view_offset_fit(target_x, col_x, width, mode);
        }

        -(area.size.w - width) / 2. - area.loc.x
    }

    pub(in crate::layout::scrolling) fn compute_new_view_offset_for_column_fit(&self, target_x: Option<f64>, idx: usize) -> f64 {
        let col = &self.columns[idx];
        self.compute_new_view_offset_fit(
            target_x,
            self.column_x(idx),
            col.width(),
            col.sizing_mode(),
        )
    }

    pub(in crate::layout::scrolling) fn compute_new_view_offset_for_column_centered(
        &self,
        target_x: Option<f64>,
        idx: usize,
    ) -> f64 {
        let col = &self.columns[idx];
        self.compute_new_view_offset_centered(
            target_x,
            self.column_x(idx),
            col.width(),
            col.sizing_mode(),
        )
    }

    pub(in crate::layout::scrolling) fn compute_new_view_offset_for_column(
        &self,
        target_x: Option<f64>,
        idx: usize,
        prev_idx: Option<usize>,
    ) -> f64 {
        if self.is_centering_focused_column() {
            return self.compute_new_view_offset_for_column_centered(target_x, idx);
        }

        match self.options.layout.center_focused_column {
            CenterFocusedColumn::Always => {
                self.compute_new_view_offset_for_column_centered(target_x, idx)
            }
            CenterFocusedColumn::OnOverflow => {
                let Some(prev_idx) = prev_idx else {
                    return self.compute_new_view_offset_for_column_fit(target_x, idx);
                };

                // Activating the same column.
                if prev_idx == idx {
                    return self.compute_new_view_offset_for_column_fit(target_x, idx);
                }

                // Always take the left or right neighbor of the target as the source.
                let source_idx = if prev_idx > idx {
                    std::cmp::min(idx + 1, self.columns.len() - 1)
                } else {
                    idx.saturating_sub(1)
                };

                let source_col_x = self.column_x(source_idx);
                let source_col_width = self.columns[source_idx].width();

                let target_col_x = self.column_x(idx);
                let target_col_width = self.columns[idx].width();

                // NOTE: This logic won't work entirely correctly with small fixed-size maximized
                // windows (they have a different area and padding).
                let total_width = if source_col_x < target_col_x {
                    // Source is left from target.
                    target_col_x - source_col_x + target_col_width
                } else {
                    // Source is right from target.
                    source_col_x - target_col_x + source_col_width
                } + self.options.layout.gaps * 2.;

                // If it fits together, do a normal animation, otherwise center the new column.
                if total_width <= self.working_area.size.w {
                    self.compute_new_view_offset_for_column_fit(target_x, idx)
                } else {
                    self.compute_new_view_offset_for_column_centered(target_x, idx)
                }
            }
            CenterFocusedColumn::Never => {
                self.compute_new_view_offset_for_column_fit(target_x, idx)
            }
        }
    }

    pub(in crate::layout::scrolling) fn animate_view_offset(&mut self, idx: usize, new_view_offset: f64) {
        self.animate_view_offset_with_config(
            idx,
            new_view_offset,
            self.options.animations.horizontal_view_movement.0,
        );
    }

    pub(in crate::layout::scrolling) fn animate_view_offset_with_config(
        &mut self,
        idx: usize,
        new_view_offset: f64,
        config: Animation,
    ) {
        let new_col_x = self.column_x(idx);
        let old_col_x = self.column_x(self.active_column_idx);
        let offset_delta = old_col_x - new_col_x;
        self.view_offset.offset(offset_delta);

        let pixel = 1. / self.scale;

        // If our view offset is already this or animating towards this, we don't need to do
        // anything.
        let to_diff = new_view_offset - self.view_offset.target();
        if to_diff.abs() < pixel {
            // Correct for any inaccuracy.
            self.view_offset.offset(to_diff);
            return;
        }

        match &mut self.view_offset {
            ViewOffset::Gesture(gesture) if gesture.dnd_last_event_time.is_some() => {
                gesture.stationary_view_offset = new_view_offset;

                let current_pos = gesture.current_view_offset - gesture.delta_from_tracker;
                gesture.delta_from_tracker = new_view_offset - current_pos;
                let offset_delta = new_view_offset - gesture.current_view_offset;
                gesture.current_view_offset = new_view_offset;

                gesture.animate_from(-offset_delta, self.clock.clone(), config);
            }
            _ => {
                // FIXME: also compute and use current velocity.
                self.view_offset = ViewOffset::Animation(crate::animation::Animation::new(
                    self.clock.clone(),
                    self.view_offset.current(),
                    new_view_offset,
                    0.,
                    config,
                ));
            }
        }
    }

    fn animate_view_offset_to_column_centered(
        &mut self,
        target_x: Option<f64>,
        idx: usize,
        config: Animation,
    ) {
        let new_view_offset = self.compute_new_view_offset_for_column_centered(target_x, idx);
        self.animate_view_offset_with_config(idx, new_view_offset, config);
    }

    pub(in crate::layout::scrolling) fn animate_view_offset_to_column_with_config(
        &mut self,
        target_x: Option<f64>,
        idx: usize,
        prev_idx: Option<usize>,
        config: Animation,
    ) {
        let new_view_offset = self.compute_new_view_offset_for_column(target_x, idx, prev_idx);
        self.animate_view_offset_with_config(idx, new_view_offset, config);
    }

    pub(in crate::layout::scrolling) fn animate_view_offset_to_column(
        &mut self,
        target_x: Option<f64>,
        idx: usize,
        prev_idx: Option<usize>,
    ) {
        self.animate_view_offset_to_column_with_config(
            target_x,
            idx,
            prev_idx,
            self.options.animations.horizontal_view_movement.0,
        )
    }

    pub(in crate::layout::scrolling) fn activate_column(&mut self, idx: usize) {
        self.activate_column_with_anim_config(
            idx,
            self.options.animations.horizontal_view_movement.0,
        );
    }

    pub(in crate::layout::scrolling) fn activate_column_with_anim_config(&mut self, idx: usize, config: Animation) {
        if self.active_column_idx == idx
            // During a DnD scroll, animate even when activating the same window, for DnD hold.
            && (self.columns.is_empty() || !self.view_offset.is_dnd_scroll())
        {
            return;
        }

        self.animate_view_offset_to_column_with_config(
            None,
            idx,
            Some(self.active_column_idx),
            config,
        );

        if self.active_column_idx != idx {
            self.active_column_idx = idx;

            // A different column was activated; reset the flag.
            self.activate_prev_column_on_removal = None;
            self.view_offset_to_restore = None;
            self.interactive_resize = None;
        }
    }

    pub fn center_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.animate_view_offset_to_column_centered(
            None,
            self.active_column_idx,
            self.options.animations.horizontal_view_movement.0,
        );

        let col = &mut self.columns[self.active_column_idx];
        cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn center_window(&mut self, window: Option<&W::Id>) {
        if self.columns.is_empty() {
            return;
        }

        let col_idx = if let Some(window) = window {
            self.columns
                .iter()
                .position(|col| col.contains(window))
                .unwrap()
        } else {
            self.active_column_idx
        };

        // We can reasonably center only the active column.
        if col_idx != self.active_column_idx {
            return;
        }

        self.center_column();
    }

    pub fn center_visible_columns(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        if self.is_centering_focused_column() {
            return;
        }

        // Consider the end of an ongoing animation because that's what compute to fit does too.
        let view_x = self.target_view_pos();
        let working_x = self.working_area.loc.x;
        let working_w = self.working_area.size.w;

        // Count all columns that are fully visible inside the working area.
        let mut width_taken = 0.;
        let mut leftmost_col_x = None;
        let mut active_col_x = None;

        let gap = self.options.layout.gaps;
        let col_xs = self.column_xs(self.data.iter().copied());
        for (idx, col_x) in col_xs.take(self.columns.len()).enumerate() {
            if col_x < view_x + working_x + gap {
                // Column goes off-screen to the left.
                continue;
            }

            leftmost_col_x.get_or_insert(col_x);

            let width = self.data[idx].width;
            if view_x + working_x + working_w < col_x + width + gap {
                // Column goes off-screen to the right. We can stop here.
                break;
            }

            if idx == self.active_column_idx {
                active_col_x = Some(col_x);
            }

            width_taken += width + gap;
        }

        if active_col_x.is_none() {
            // The active column wasn't fully on screen, so we can't meaningfully do anything.
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        cancel_resize_for_column(&mut self.interactive_resize, col);

        let free_space = working_w - width_taken + gap;
        let new_view_x = leftmost_col_x.unwrap() - free_space / 2. - working_x;

        self.animate_view_offset(self.active_column_idx, new_view_x - active_col_x.unwrap());
        // Just in case.
        self.animate_view_offset_to_column(None, self.active_column_idx, None);
    }
}

pub fn cancel_resize_for_column<W: LayoutElement>(
    interactive_resize: &mut Option<crate::layout::workspace::InteractiveResize<W>>,
    column: &mut super::super::column::Column<W>,
) {
    if let Some(resize) = interactive_resize {
        if column.contains(&resize.window) {
            *interactive_resize = None;
        }
    }

    for tile in &mut column.tiles {
        tile.window_mut().cancel_interactive_resize();
    }
}
