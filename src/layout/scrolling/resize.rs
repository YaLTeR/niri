
use crate::layout::ColumnDisplay;
use smithay::utils::{Logical, Point};

use super::space::ScrollingSpace;
use super::super::workspace::InteractiveResize;
use super::super::{InteractiveResizeData};
use super::super::LayoutElement;
use crate::utils::ResizeEdge;

use super::types::ColumnWidth;
use crate::layout::SizeChange;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn interactive_resize_begin(&mut self, window: W::Id, edges: ResizeEdge) -> bool {
        if self.interactive_resize.is_some() {
            return false;
        }

        let col = self
            .columns
            .iter_mut()
            .find(|col| col.contains(&window))
            .unwrap();

        if !col.pending_sizing_mode().is_normal() {
            return false;
        }

        let tile = col
            .tiles
            .iter_mut()
            .find(|tile| tile.window().id() == &window)
            .unwrap();

        let original_window_size = tile.window_size();

        let resize = InteractiveResize {
            window,
            original_window_size,
            data: InteractiveResizeData { edges },
        };
        self.interactive_resize = Some(resize);

        self.view_offset.stop_anim_and_gesture();

        true
    }

    pub fn interactive_resize_update(
        &mut self,
        window: &W::Id,
        delta: Point<f64, Logical>,
    ) -> bool {
        let Some(resize) = &self.interactive_resize else {
            return false;
        };

        if window != &resize.window {
            return false;
        }

        let is_centering = self.is_centering_focused_column();

        let col = self
            .columns
            .iter_mut()
            .find(|col| col.contains(window))
            .unwrap();

        let tile_idx = col
            .tiles
            .iter()
            .position(|tile| tile.window().id() == window)
            .unwrap();

        if resize.data.edges.intersects(ResizeEdge::LEFT_RIGHT) {
            let mut dx = delta.x;
            if resize.data.edges.contains(ResizeEdge::LEFT) {
                dx = -dx;
            };

            if is_centering {
                dx *= 2.;
            }

            let window_width = (resize.original_window_size.w + dx).round() as i32;
            col.set_column_width(SizeChange::SetFixed(window_width), Some(tile_idx), false);
        }

        if resize.data.edges.intersects(ResizeEdge::TOP_BOTTOM) {
            // Prevent the simplest case of weird resizing (top edge when this is the topmost
            // window).
            if !(resize.data.edges.contains(ResizeEdge::TOP) && tile_idx == 0) {
                let mut dy = delta.y;
                if resize.data.edges.contains(ResizeEdge::TOP) {
                    dy = -dy;
                };

                // FIXME: some smarter height distribution would be nice here so that vertical
                // resizes work as expected in more cases.

                let window_height = (resize.original_window_size.h + dy).round() as i32;
                col.set_window_height(SizeChange::SetFixed(window_height), Some(tile_idx), false);
            }
        }

        true
    }

    pub fn interactive_resize_end(&mut self, window: Option<&W::Id>) {
        let Some(resize) = &self.interactive_resize else {
            return;
        };

        if let Some(window) = window {
            if window != &resize.window {
                return;
            }

            // Animate the active window into view right away.
            if self.columns[self.active_column_idx].contains(window) {
                self.animate_view_offset_to_column(None, self.active_column_idx, None);
            }
        }

        self.interactive_resize = None;
    }

    pub fn toggle_width(&mut self, forwards: bool) {
        if self.columns.is_empty() {
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        col.toggle_width(None, forwards);

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn toggle_full_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        col.toggle_full_width();

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn set_window_width(&mut self, window: Option<&W::Id>, change: SizeChange) {
        if self.columns.is_empty() {
            return;
        }

        let (col_idx, tile_idx) = if let Some(window) = window {
            self.columns
                .iter()
                .enumerate()
                .find_map(|(col_idx, col)| {
                    col.tiles
                        .iter()
                        .position(|tile| tile.window().id() == window)
                        .map(|tile_idx| (col_idx, Some(tile_idx)))
                })
                .unwrap()
        } else {
            (self.active_column_idx, None)
        };

        let col = &mut self.columns[col_idx];
        col.set_column_width(change, tile_idx, true);
        self.data[col_idx].update(col);

        let col = &mut self.columns[col_idx];
        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn set_window_height(&mut self, window: Option<&W::Id>, change: SizeChange) {
        if self.columns.is_empty() {
            return;
        }

        let (col, tile_idx) = if let Some(window) = window {
            self.columns
                .iter_mut()
                .find_map(|col| {
                    col.tiles
                        .iter()
                        .position(|tile| tile.window().id() == window)
                        .map(|tile_idx| (col, Some(tile_idx)))
                })
                .unwrap()
        } else {
            (&mut self.columns[self.active_column_idx], None)
        };

        col.set_window_height(change, tile_idx, true);

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn reset_window_height(&mut self, window: Option<&W::Id>) {
        if self.columns.is_empty() {
            return;
        }

        let (col, tile_idx) = if let Some(window) = window {
            self.columns
                .iter_mut()
                .find_map(|col| {
                    col.tiles
                        .iter()
                        .position(|tile| tile.window().id() == window)
                        .map(|tile_idx| (col, Some(tile_idx)))
                })
                .unwrap()
        } else {
            (&mut self.columns[self.active_column_idx], None)
        };

        col.reset_window_height(tile_idx);

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn toggle_window_width(&mut self, window: Option<&W::Id>, forwards: bool) {
        if self.columns.is_empty() {
            return;
        }

        let (col, tile_idx) = if let Some(window) = window {
            self.columns
                .iter_mut()
                .find_map(|col| {
                    col.tiles
                        .iter()
                        .position(|tile| tile.window().id() == window)
                        .map(|tile_idx| (col, Some(tile_idx)))
                })
                .unwrap()
        } else {
            (&mut self.columns[self.active_column_idx], None)
        };

        col.toggle_width(tile_idx, forwards);

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn toggle_window_height(&mut self, window: Option<&W::Id>, forwards: bool) {
        if self.columns.is_empty() {
            return;
        }

        let (col, tile_idx) = if let Some(window) = window {
            self.columns
                .iter_mut()
                .find_map(|col| {
                    col.tiles
                        .iter()
                        .position(|tile| tile.window().id() == window)
                        .map(|tile_idx| (col, Some(tile_idx)))
                })
                .unwrap()
        } else {
            (&mut self.columns[self.active_column_idx], None)
        };

        col.toggle_window_height(tile_idx, forwards);

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn expand_column_to_available_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        if !col.pending_sizing_mode().is_normal() || col.is_full_width {
            return;
        }

        if self.is_centering_focused_column() {
            // Always-centered mode is different since the active window position cannot be
            // controlled (it's always at the center). I guess you could come up with different
            // logic here that computes the width in such a way so as to leave nearby columns fully
            // on screen while taking into account that the active column will remain centered
            // after resizing. But I'm not sure it's that useful? So let's do the simple thing.
            let col = &mut self.columns[self.active_column_idx];
            col.toggle_full_width();
            super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);
            return;
        }

        // NOTE: This logic won't work entirely correctly with small fixed-size maximized windows
        // (they have a different area and padding).

        // Consider the end of an ongoing animation because that's what compute to fit does too.
        let view_x = self.target_view_pos();
        let working_x = self.working_area.loc.x;
        let working_w = self.working_area.size.w;

        // Count all columns that are fully visible inside the working area.
        let mut width_taken = 0.;
        let mut leftmost_col_x = None;
        let mut active_col_x = None;
        let mut counted_non_active_column = false;

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
            } else {
                counted_non_active_column = true;
            }

            width_taken += width + gap;
        }

        if active_col_x.is_none() {
            // The active column wasn't fully on screen, so we can't meaningfully do anything.
            return;
        }

        let col = &mut self.columns[self.active_column_idx];

        let available_width = working_w - gap - width_taken - col.extra_size().w;
        if available_width <= 0. {
            // Nowhere to expand.
            return;
        }

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);

        if !counted_non_active_column {
            // Only the active column was fully on-screen (maybe it's the only column), so we're
            // about to set its width to 100% of the working area. Let's do it via
            // toggle_full_width() as it lets you back out of it more intuitively.
            col.toggle_full_width();
            return;
        }

        let active_width = self.data[self.active_column_idx].width;
        col.width = ColumnWidth::Fixed(active_width + available_width);
        col.preset_width_idx = None;
        col.is_full_width = false;
        col.update_tile_sizes(true);

        // Put the leftmost window into the view.
        let new_view_x = leftmost_col_x.unwrap() - gap - working_x;
        self.animate_view_offset(self.active_column_idx, new_view_x - active_col_x.unwrap());
        // Just in case.
        self.animate_view_offset_to_column(None, self.active_column_idx, None);
    }

    pub fn set_fullscreen(&mut self, window: &W::Id, is_fullscreen: bool) -> bool {
        let mut col_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();

        if is_fullscreen == self.columns[col_idx].is_pending_fullscreen {
            return false;
        }

        let mut col = &mut self.columns[col_idx];
        let is_tabbed = col.display_mode == ColumnDisplay::Tabbed;

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);

        if is_fullscreen && (col.tiles.len() > 1 && !is_tabbed) {
            // This wasn't the only window in its column; extract it into a separate column.
            self.consume_or_expel_window_right(Some(window));
            col_idx += 1;
            col = &mut self.columns[col_idx];
        }

        col.set_fullscreen(is_fullscreen);

        // With place_within_column, the tab indicator changes the column size immediately.
        self.data[col_idx].update(col);

        true
    }

    pub fn set_maximized(&mut self, window: &W::Id, maximize: bool) -> bool {
        let mut col_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();

        if maximize == self.columns[col_idx].is_pending_maximized {
            return false;
        }

        let mut col = &mut self.columns[col_idx];
        let is_tabbed = col.display_mode == ColumnDisplay::Tabbed;

        super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, col);

        if maximize && (col.tiles.len() > 1 && !is_tabbed) {
            // This wasn't the only window in its column; extract it into a separate column.
            self.consume_or_expel_window_right(Some(window));
            col_idx += 1;
            col = &mut self.columns[col_idx];
        }

        col.set_maximized(maximize);

        // With place_within_column, the tab indicator changes the column size immediately.
        self.data[col_idx].update(col);

        true
    }
}
