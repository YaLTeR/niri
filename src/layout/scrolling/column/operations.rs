// Column operations implementation - focus management, width/height operations, and display mode changes
//
// This file contains the Column methods for:
// - Focus management (up, down, top, bottom, index)
// - Width operations (toggle, set, full width)
// - Height operations (set, reset, toggle, convert to auto)
// - Display mode changes (fullscreen, maximize, column display)

use std::cmp::min;
use std::iter::zip;

use niri_config::PresetSize;
use niri_ipc::{ColumnDisplay, SizeChange};
use smithay::utils::Point;

use super::super::super::{LayoutElement, Options};
use super::super::types::WindowHeight;
use super::super::super::workspace::ResolvedSize;
use super::{Column, ColumnWidth};
use crate::layout::SizingMode;

impl<W: LayoutElement> Column<W> {
    pub(in crate::layout::scrolling) fn focus_index(&mut self, index: u8) {
        let idx = min(usize::from(index.saturating_sub(1)), self.tiles.len() - 1);
        self.activate_idx(idx);
    }

    pub(in crate::layout::scrolling) fn focus_up(&mut self) -> bool {
        self.activate_idx(self.active_tile_idx.saturating_sub(1))
    }

    pub(in crate::layout::scrolling) fn focus_down(&mut self) -> bool {
        self.activate_idx(min(self.active_tile_idx + 1, self.tiles.len() - 1))
    }

    pub(in crate::layout::scrolling) fn focus_top(&mut self) {
        self.activate_idx(0);
    }

    pub(in crate::layout::scrolling) fn focus_bottom(&mut self) {
        self.activate_idx(self.tiles.len() - 1);
    }

    pub(in crate::layout::scrolling) fn move_up(&mut self) -> bool {
        let new_idx = self.active_tile_idx.saturating_sub(1);
        if self.active_tile_idx == new_idx {
            return false;
        }

        let mut ys = self.tile_offsets().skip(self.active_tile_idx);
        let active_y = ys.next().unwrap().y;
        let next_y = ys.next().unwrap().y;
        drop(ys);

        self.tiles.swap(self.active_tile_idx, new_idx);
        self.data.swap(self.active_tile_idx, new_idx);
        self.active_tile_idx = new_idx;

        // Animate the movement.
        let new_active_y = self.tile_offset(new_idx).y;
        self.tiles[new_idx].animate_move_y_from(active_y - new_active_y);
        self.tiles[new_idx + 1].animate_move_y_from(active_y - next_y);

        true
    }

    pub(in crate::layout::scrolling) fn move_down(&mut self) -> bool {
        let new_idx = min(self.active_tile_idx + 1, self.tiles.len() - 1);
        if self.active_tile_idx == new_idx {
            return false;
        }

        let mut ys = self.tile_offsets().skip(self.active_tile_idx);
        let active_y = ys.next().unwrap().y;
        let next_y = ys.next().unwrap().y;
        drop(ys);

        self.tiles.swap(self.active_tile_idx, new_idx);
        self.data.swap(self.active_tile_idx, new_idx);
        self.active_tile_idx = new_idx;

        // Animate the movement.
        let new_active_y = self.tile_offset(new_idx).y;
        self.tiles[new_idx].animate_move_y_from(active_y - new_active_y);
        self.tiles[new_idx - 1].animate_move_y_from(next_y - active_y);

        true
    }

    pub(in crate::layout::scrolling) fn toggle_width(&mut self, tile_idx: Option<usize>, forwards: bool) {
        let tile_idx = tile_idx.unwrap_or(self.active_tile_idx);

        let preset_idx = if self.is_full_width || self.is_pending_maximized {
            None
        } else {
            self.preset_width_idx
        };

        let len = self.options.layout.preset_column_widths.len();
        let preset_idx = if let Some(idx) = preset_idx {
            (idx + if forwards { 1 } else { len - 1 }) % len
        } else {
            let tile = &self.tiles[tile_idx];
            let current_window = tile.window_expected_or_current_size().w;
            let current_tile = tile.tile_expected_or_current_size().w;

            let mut it = self
                .options
                .layout
                .preset_column_widths
                .iter()
                .map(|preset| self.resolve_preset_width(*preset));

            if forwards {
                it.position(|resolved| {
                    match resolved {
                        // Some allowance for fractional scaling purposes.
                        ResolvedSize::Tile(resolved) => current_tile + 1. < resolved,
                        ResolvedSize::Window(resolved) => current_window + 1. < resolved,
                    }
                })
                .unwrap_or(0)
            } else {
                it.rposition(|resolved| {
                    match resolved {
                        // Some allowance for fractional scaling purposes.
                        ResolvedSize::Tile(resolved) => resolved + 1. < current_tile,
                        ResolvedSize::Window(resolved) => resolved + 1. < current_window,
                    }
                })
                .unwrap_or(len - 1)
            }
        };

        let preset = self.options.layout.preset_column_widths[preset_idx];
        self.set_column_width(SizeChange::from(preset), Some(tile_idx), true);

        self.preset_width_idx = Some(preset_idx);
    }

    pub(in crate::layout::scrolling) fn toggle_full_width(&mut self) {
        if self.is_pending_maximized {
            // Treat it as unmaximize.
            self.is_pending_maximized = false;
            self.is_full_width = false;
        } else {
            self.is_full_width = !self.is_full_width;
        }

        self.update_tile_sizes(true);
    }

    pub(in crate::layout::scrolling) fn set_column_width(&mut self, change: SizeChange, tile_idx: Option<usize>, animate: bool) {
        let current = if self.is_full_width || self.is_pending_maximized {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let current_px = self.resolve_column_width(current);

        // FIXME: fix overflows then remove limits.
        const MAX_PX: f64 = 100000.;
        const MAX_F: f64 = 10000.;

        let width = match (current, change) {
            (_, SizeChange::SetFixed(fixed)) => {
                // As a special case, setting a fixed column width will compute it in such a way
                // that the specified (usually active) window gets that width. This is the
                // intention behind the ability to set a fixed size.
                let tile_idx = tile_idx.unwrap_or(self.active_tile_idx);
                let tile = &self.tiles[tile_idx];
                ColumnWidth::Fixed(
                    tile.tile_width_for_window_width(f64::from(fixed))
                        .clamp(1., MAX_PX),
                )
            }
            (_, SizeChange::SetProportion(proportion)) => {
                ColumnWidth::Proportion((proportion / 100.).clamp(0., MAX_F))
            }
            (_, SizeChange::AdjustFixed(delta)) => {
                let width = (current_px + f64::from(delta)).clamp(1., MAX_PX);
                ColumnWidth::Fixed(width)
            }
            (ColumnWidth::Proportion(current), SizeChange::AdjustProportion(delta)) => {
                let proportion = (current + delta / 100.).clamp(0., MAX_F);
                ColumnWidth::Proportion(proportion)
            }
            (ColumnWidth::Fixed(_), SizeChange::AdjustProportion(delta)) => {
                let full = self.working_area.size.w - self.options.layout.gaps;
                let current = if full == 0. {
                    1.
                } else {
                    (current_px + self.options.layout.gaps + self.extra_size().w) / full
                };
                let proportion = (current + delta / 100.).clamp(0., MAX_F);
                ColumnWidth::Proportion(proportion)
            }
        };

        self.width = width;
        self.preset_width_idx = None;
        self.is_full_width = false;
        self.is_pending_maximized = false;
        self.update_tile_sizes(animate);
    }

    pub(in crate::layout::scrolling) fn set_window_height(&mut self, change: SizeChange, tile_idx: Option<usize>, animate: bool) {
        let tile_idx = tile_idx.unwrap_or(self.active_tile_idx);

        // Start by converting all heights to automatic, since only one window in the column can be
        // non-auto-height. If the current tile is already non-auto, however, we can skip that
        // step. Which is not only for optimization, but also preserves automatic weights in case
        // one window is resized in such a way that other windows hit their min size, and then
        // back.
        if matches!(self.data[tile_idx].height, WindowHeight::Auto { .. }) {
            self.convert_heights_to_auto();
        }

        let current = self.data[tile_idx].height;
        let tile = &self.tiles[tile_idx];
        let current_window_px = match current {
            WindowHeight::Auto { .. } | WindowHeight::Preset(_) => tile.window_size().h,
            WindowHeight::Fixed(height) => height,
        };
        let current_tile_px = tile.tile_height_for_window_height(current_window_px);

        let working_size = self.working_area.size.h;
        let gaps = self.options.layout.gaps;
        let extra_size = self.extra_size().h;
        let full = working_size - gaps;
        let current_prop = if full == 0. {
            1.
        } else {
            (current_tile_px + gaps) / full
        };

        // FIXME: fix overflows then remove limits.
        const MAX_PX: f64 = 100000.;

        let mut window_height = match change {
            SizeChange::SetFixed(fixed) => f64::from(fixed),
            SizeChange::SetProportion(proportion) => {
                let tile_height = (working_size - gaps) * (proportion / 100.) - gaps - extra_size;
                tile.window_height_for_tile_height(tile_height)
            }
            SizeChange::AdjustFixed(delta) => current_window_px + f64::from(delta),
            SizeChange::AdjustProportion(delta) => {
                let proportion = current_prop + delta / 100.;
                let tile_height = (working_size - gaps) * proportion - gaps - extra_size;
                tile.window_height_for_tile_height(tile_height)
            }
        };

        // Clamp the height according to other windows' min sizes, or simply to working area height.
        let min_height_taken = if self.display_mode == ColumnDisplay::Tabbed {
            0.
        } else {
            self.tiles
                .iter()
                .enumerate()
                .filter(|(idx, _)| *idx != tile_idx)
                .map(|(_, tile)| f64::max(1., tile.min_size_nonfullscreen().h) + gaps)
                .sum::<f64>()
        };
        let height_left = working_size - extra_size - gaps - min_height_taken - gaps;
        let height_left = f64::max(1., tile.window_height_for_tile_height(height_left));
        window_height = f64::min(height_left, window_height);

        // Clamp it against the window height constraints.
        let win = &self.tiles[tile_idx].window();
        let min_h = win.min_size().h;
        let max_h = win.max_size().h;

        if max_h > 0 {
            window_height = f64::min(window_height, f64::from(max_h));
        }
        if min_h > 0 {
            window_height = f64::max(window_height, f64::from(min_h));
        }

        self.data[tile_idx].height = WindowHeight::Fixed(window_height.clamp(1., MAX_PX));
        self.is_pending_maximized = false;
        self.update_tile_sizes(animate);
    }

    pub(in crate::layout::scrolling) fn reset_window_height(&mut self, tile_idx: Option<usize>) {
        if self.display_mode == ColumnDisplay::Tabbed {
            // When tabbed, reset window height should work on any window, not just the fixed-size
            // one.
            for data in &mut self.data {
                data.height = WindowHeight::auto_1();
            }
        } else {
            let tile_idx = tile_idx.unwrap_or(self.active_tile_idx);
            self.data[tile_idx].height = WindowHeight::auto_1();
        }

        self.update_tile_sizes(true);
    }

    pub(in crate::layout::scrolling) fn toggle_window_height(&mut self, tile_idx: Option<usize>, forwards: bool) {
        let tile_idx = tile_idx.unwrap_or(self.active_tile_idx);

        // Start by converting all heights to automatic, since only one window in the column can be
        // non-auto-height. If the current tile is already non-auto, however, we can skip that
        // step. Which is not only for optimization, but also preserves automatic weights in case
        // one window is resized in such a way that other windows hit their min size, and then
        // back.
        if matches!(self.data[tile_idx].height, WindowHeight::Auto { .. }) {
            self.convert_heights_to_auto();
        }

        let len = self.options.layout.preset_window_heights.len();
        let preset_idx = match self.data[tile_idx].height {
            WindowHeight::Preset(idx) if !self.is_pending_maximized => {
                (idx + if forwards { 1 } else { len - 1 }) % len
            }
            _ => {
                let current = self.data[tile_idx].size.h;
                let tile = &self.tiles[tile_idx];

                let mut it = self
                    .options
                    .layout
                    .preset_window_heights
                    .iter()
                    .copied()
                    .map(|preset| {
                        let window_height = match self.resolve_preset_height(preset) {
                            ResolvedSize::Tile(h) => tile.window_height_for_tile_height(h),
                            ResolvedSize::Window(h) => h,
                        };
                        tile.tile_height_for_window_height(window_height.round().clamp(1., 100000.))
                    });

                if forwards {
                    it.position(|resolved| {
                        // Some allowance for fractional scaling purposes.
                        current + 1. < resolved
                    })
                    .unwrap_or(0)
                } else {
                    it.rposition(|resolved| {
                        // Some allowance for fractional scaling purposes.
                        resolved + 1. < current
                    })
                    .unwrap_or(len - 1)
                }
            }
        };
        self.data[tile_idx].height = WindowHeight::Preset(preset_idx);
        self.is_pending_maximized = false;
        self.update_tile_sizes(true);
    }

    /// Converts all heights in the column to automatic, preserving the apparent heights.
    ///
    /// All weights are recomputed to preserve the current tile heights while "centering" the
    /// weights at the median window height (it gets weight = 1).
    ///
    /// One case where apparent heights will not be preserved is when the column is taller than the
    /// working area.
    pub(in crate::layout::scrolling) fn convert_heights_to_auto(&mut self) {
        let heights: Vec<_> = self.tiles.iter().map(|tile| tile.tile_size().h).collect();

        // Weights are invariant to multiplication: a column with weights 2, 2, 1 is equivalent to
        // a column with weights 4, 4, 2. So we find the median window height and use that as 1.
        let mut sorted = heights.clone();
        sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];

        for (data, height) in zip(&mut self.data, heights) {
            let weight = height / median;
            data.height = WindowHeight::Auto { weight };
        }
    }

    pub(in crate::layout::scrolling) fn set_fullscreen(&mut self, is_fullscreen: bool) {
        if self.is_pending_fullscreen == is_fullscreen {
            return;
        }

        if is_fullscreen {
            assert!(self.tiles.len() == 1 || self.display_mode == ColumnDisplay::Tabbed);
        }

        self.is_pending_fullscreen = is_fullscreen;
        self.update_tile_sizes(true);
    }

    pub(in crate::layout::scrolling) fn set_maximized(&mut self, maximize: bool) {
        if self.is_pending_maximized == maximize {
            return;
        }

        if maximize {
            assert!(self.tiles.len() == 1 || self.display_mode == ColumnDisplay::Tabbed);
        }

        self.is_pending_maximized = maximize;
        self.update_tile_sizes(true);
    }

    pub(in crate::layout::scrolling) fn set_column_display(&mut self, display: ColumnDisplay) {
        if self.display_mode == display {
            return;
        }

        // Animate the movement.
        //
        // We're doing some shortcuts here because we know that currently normal vs. tabbed can
        // only cause a vertical shift + a shift to the origin.
        //
        // Doing it this way to avoid storing all tile positions in a vector. If more display modes
        // are added it might be simpler to just collect everything into a smallvec.
        let prev_origin = self.tiles_origin();
        self.display_mode = display;
        let new_origin = self.tiles_origin();
        let origin_delta = prev_origin - new_origin;

        // When need to walk the tiles in the normal display mode to get the right offsets.
        self.display_mode = ColumnDisplay::Normal;
        for (tile, pos) in self.tiles_mut() {
            let mut y_delta = pos.y - prev_origin.y;

            // Invert the Y motion when transitioning *to* normal display mode.
            if display == ColumnDisplay::Normal {
                y_delta *= -1.;
            }

            let mut delta = origin_delta;
            delta.y += y_delta;
            tile.animate_move_from(delta);
        }

        // Animate the opacity.
        for (idx, tile) in self.tiles.iter_mut().enumerate() {
            let is_active = idx == self.active_tile_idx;
            if !is_active {
                let (from, to) = if display == ColumnDisplay::Tabbed {
                    (1., 0.)
                } else {
                    (0., 1.)
                };
                tile.animate_alpha(from, to, self.options.animations.window_movement.0);
            }
        }

        // Animate the appearance of the tab indicator.
        if display == ColumnDisplay::Tabbed {
            self.tab_indicator.start_open_animation(
                self.clock.clone(),
                self.options.animations.window_movement.0,
            );
        }

        // Now switch the display mode for real.
        self.display_mode = display;
        self.update_tile_sizes(true);
    }
}
