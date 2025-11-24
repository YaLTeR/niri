use std::cmp::min;

use niri_config::Animation;
use niri_ipc::ColumnDisplay;
use smithay::utils::{Point, Serial};

use super::super::ScrollingSpace;
use crate::layout::LayoutElement;
use super::super::super::{RemovedTile, Options};
use super::super::super::tile::Tile;
use super::super::types::{ColumnData, ColumnWidth, ViewOffset};
use super::super::column::Column;
use crate::utils::transaction::{Transaction, TransactionBlocker};
use crate::utils::ResizeEdge;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn add_tile(
        &mut self,
        col_idx: Option<usize>,
        tile: Tile<W>,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
        anim_config: Option<Animation>,
    ) {
        let column = Column::new_with_tile(
            tile,
            self.view_size,
            self.working_area,
            self.parent_area,
            self.scale,
            width,
            is_full_width,
        );

        self.add_column(col_idx, column, activate, anim_config);
    }

    pub fn add_tile_to_column(
        &mut self,
        col_idx: usize,
        tile_idx: Option<usize>,
        tile: Tile<W>,
        activate: bool,
    ) {
        let prev_next_x = self.column_x(col_idx + 1);

        let target_column = &mut self.columns[col_idx];
        let tile_idx = tile_idx.unwrap_or(target_column.tiles.len());
        let mut prev_active_tile_idx = target_column.active_tile_idx;

        target_column.add_tile_at(tile_idx, tile);
        self.data[col_idx].update(target_column);

        if tile_idx <= prev_active_tile_idx {
            target_column.active_tile_idx += 1;
            prev_active_tile_idx += 1;
        }

        if activate {
            target_column.activate_idx(tile_idx);
            if self.active_column_idx != col_idx {
                self.activate_column(col_idx);
            }
        }

        let target_column = &mut self.columns[col_idx];
        if target_column.display_mode == ColumnDisplay::Tabbed {
            if target_column.active_tile_idx == tile_idx {
                // Fade out the previously active tile.
                let tile = &mut target_column.tiles[prev_active_tile_idx];
                tile.animate_alpha(1., 0., self.options.animations.window_movement.0);
            } else {
                // Fade out when adding into a tabbed column into the background.
                let tile = &mut target_column.tiles[tile_idx];
                tile.animate_alpha(1., 0., self.options.animations.window_movement.0);
            }
        }

        // Adding a wider window into a column increases its width now (even if the window will
        // shrink later). Move the columns to account for this.
        let offset = self.column_x(col_idx + 1) - prev_next_x;
        if self.active_column_idx <= col_idx {
            for col in &mut self.columns[col_idx + 1..] {
                col.animate_move_from(-offset);
            }
        } else {
            for col in &mut self.columns[..=col_idx] {
                col.animate_move_from(offset);
            }
        }
    }

    pub fn add_tile_right_of(
        &mut self,
        right_of: &W::Id,
        tile: Tile<W>,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        let right_of_idx = self
            .columns
            .iter()
            .position(|col| col.contains(right_of))
            .unwrap();
        let col_idx = right_of_idx + 1;

        self.add_tile(Some(col_idx), tile, activate, width, is_full_width, None);
    }

    pub fn add_column(
        &mut self,
        idx: Option<usize>,
        mut column: Column<W>,
        activate: bool,
        anim_config: Option<Animation>,
    ) {
        let was_empty = self.columns.is_empty();

        let idx = idx.unwrap_or_else(|| {
            if was_empty {
                0
            } else {
                self.active_column_idx + 1
            }
        });

        column.update_config(
            self.view_size,
            self.working_area,
            self.parent_area,
            self.scale,
            self.options.clone(),
        );
        self.data.insert(idx, ColumnData::new(&column));
        self.columns.insert(idx, column);

        if activate {
            // If this is the first window on an empty workspace, remove the effect of whatever
            // view_offset was left over and skip the animation.
            if was_empty {
                self.view_offset = ViewOffset::Static(0.);
                self.view_offset =
                    ViewOffset::Static(self.compute_new_view_offset_for_column(None, idx, None));
            }

            let prev_offset = (!was_empty && idx == self.active_column_idx + 1)
                .then(|| self.view_offset.stationary());

            let anim_config =
                anim_config.unwrap_or(self.options.animations.horizontal_view_movement.0);
            self.activate_column_with_anim_config(idx, anim_config);
            self.activate_prev_column_on_removal = prev_offset;
        } else if !was_empty && idx <= self.active_column_idx {
            self.active_column_idx += 1;
        }

        // Animate movement of other columns.
        let offset = self.column_x(idx + 1) - self.column_x(idx);
        let config = anim_config.unwrap_or(self.options.animations.window_movement.0);
        if self.active_column_idx <= idx {
            for col in &mut self.columns[idx + 1..] {
                col.animate_move_from_with_config(-offset, config);
            }
        } else {
            for col in &mut self.columns[..idx] {
                col.animate_move_from_with_config(offset, config);
            }
        }
    }

    pub fn remove_active_tile(&mut self, transaction: Transaction) -> Option<RemovedTile<W>> {
        if self.columns.is_empty() {
            return None;
        }

        let column = &self.columns[self.active_column_idx];
        Some(self.remove_tile_by_idx(
            self.active_column_idx,
            column.active_tile_idx,
            transaction,
            None,
        ))
    }

    pub fn remove_tile(&mut self, window: &W::Id, transaction: Transaction) -> RemovedTile<W> {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();
        let column = &self.columns[column_idx];

        let tile_idx = column.position(window).unwrap();
        self.remove_tile_by_idx(column_idx, tile_idx, transaction, None)
    }

    pub fn remove_tile_by_idx(
        &mut self,
        column_idx: usize,
        tile_idx: usize,
        transaction: Transaction,
        anim_config: Option<Animation>,
    ) -> RemovedTile<W> {
        // If this is the only tile in the column, remove the whole column.
        if self.columns[column_idx].tiles.len() == 1 {
            let mut column = self.remove_column_by_idx(column_idx, anim_config);
            return RemovedTile {
                tile: column.tiles.remove(tile_idx),
                width: column.width,
                is_full_width: column.is_full_width,
                is_floating: false,
            };
        }

        let column = &mut self.columns[column_idx];
        let prev_width = self.data[column_idx].width;

        let movement_config = anim_config.unwrap_or(self.options.animations.window_movement.0);

        // Animate movement of other tiles.
        // FIXME: tiles can move by X too, in a centered or resizing layout with one window smaller
        // than the others.
        let offset_y = column.tile_offset(tile_idx + 1).y - column.tile_offset(tile_idx).y;
        for tile in &mut column.tiles[tile_idx + 1..] {
            tile.animate_move_y_from(offset_y);
        }

        if column.display_mode == ColumnDisplay::Tabbed && tile_idx != column.active_tile_idx {
            // Fade in when removing background tab from a tabbed column.
            let tile = &mut column.tiles[tile_idx];
            tile.animate_alpha(0., 1., movement_config);
        }

        let was_normal = column.sizing_mode().is_normal();

        let tile = column.tiles.remove(tile_idx);
        column.data.remove(tile_idx);

        // If an active column became non-fullscreen after removing the tile, clear the stored
        // unfullscreen offset.
        if column_idx == self.active_column_idx && !was_normal && column.sizing_mode().is_normal() {
            self.view_offset_to_restore = None;
        }

        // If one window is left, reset its weight to 1.
        if column.data.len() == 1 {
            if let super::super::types::WindowHeight::Auto { weight } = &mut column.data[0].height {
                *weight = 1.;
            }
        }

        // Stop interactive resize.
        if let Some(resize) = &self.interactive_resize {
            if tile.window().id() == &resize.window {
                self.interactive_resize = None;
            }
        }

        let tile = RemovedTile {
            tile,
            width: column.width,
            is_full_width: column.is_full_width,
            is_floating: false,
        };

        #[allow(clippy::comparison_chain)] // What do you even want here?
        if tile_idx < column.active_tile_idx {
            // A tile above was removed; preserve the current position.
            column.active_tile_idx -= 1;
        } else if tile_idx == column.active_tile_idx {
            // The active tile was removed, so the active tile index shifted to the next tile.
            if tile_idx == column.tiles.len() {
                // The bottom tile was removed and it was active, update active idx to remain valid.
                column.activate_idx(tile_idx - 1);
            } else {
                // Ensure the newly active tile animates to opaque.
                column.tiles[tile_idx].ensure_alpha_animates_to_1();
            }
        }

        column.update_tile_sizes_with_transaction(true, transaction);
        self.data[column_idx].update(column);
        let offset = prev_width - column.width();

        // Animate movement of the other columns.
        if self.active_column_idx <= column_idx {
            for col in &mut self.columns[column_idx + 1..] {
                col.animate_move_from_with_config(offset, movement_config);
            }
        } else {
            for col in &mut self.columns[..=column_idx] {
                col.animate_move_from_with_config(-offset, movement_config);
            }
        }

        tile
    }

    pub fn remove_active_column(&mut self) -> Option<Column<W>> {
        if self.columns.is_empty() {
            return None;
        }

        Some(self.remove_column_by_idx(self.active_column_idx, None))
    }

    pub fn remove_column_by_idx(
        &mut self,
        column_idx: usize,
        anim_config: Option<Animation>,
    ) -> Column<W> {
        // Animate movement of the other columns.
        let movement_config = anim_config.unwrap_or(self.options.animations.window_movement.0);
        let offset = self.column_x(column_idx + 1) - self.column_x(column_idx);
        if self.active_column_idx <= column_idx {
            for col in &mut self.columns[column_idx + 1..] {
                col.animate_move_from_with_config(offset, movement_config);
            }
        } else {
            for col in &mut self.columns[..column_idx] {
                col.animate_move_from_with_config(-offset, movement_config);
            }
        }

        let column = self.columns.remove(column_idx);
        self.data.remove(column_idx);

        // Stop interactive resize.
        if let Some(resize) = &self.interactive_resize {
            if column
                .tiles
                .iter()
                .any(|tile| tile.window().id() == &resize.window)
            {
                self.interactive_resize = None;
            }
        }

        if column_idx + 1 == self.active_column_idx {
            // The previous column, that we were going to activate upon removal of the active
            // column, has just been itself removed.
            self.activate_prev_column_on_removal = None;
        }

        if column_idx == self.active_column_idx {
            self.view_offset_to_restore = None;
        }

        if self.columns.is_empty() {
            return column;
        }

        let view_config = anim_config.unwrap_or(self.options.animations.horizontal_view_movement.0);

        if column_idx < self.active_column_idx {
            // A column to the left was removed; preserve the current position.
            // FIXME: preserve activate_prev_column_on_removal.
            self.active_column_idx -= 1;
            self.activate_prev_column_on_removal = None;
        } else if column_idx == self.active_column_idx
            && self.activate_prev_column_on_removal.is_some()
        {
            // The active column was removed, and we needed to activate the previous column.
            if 0 < column_idx {
                let prev_offset = self.activate_prev_column_on_removal.unwrap();

                self.activate_column_with_anim_config(self.active_column_idx - 1, view_config);

                // Restore the view offset but make sure to scroll the view in case the
                // previous window had resized.
                self.animate_view_offset_with_config(
                    self.active_column_idx,
                    prev_offset,
                    view_config,
                );
                self.animate_view_offset_to_column_with_config(
                    None,
                    self.active_column_idx,
                    None,
                    view_config,
                );
            }
        } else {
            self.activate_column_with_anim_config(
                min(self.active_column_idx, self.columns.len() - 1),
                view_config,
            );
        }

        column
    }

    pub fn update_window(&mut self, window: &W::Id, serial: Option<Serial>) {
        let (col_idx, column) = self
            .columns
            .iter_mut()
            .enumerate()
            .find(|(_, col)| col.contains(window))
            .unwrap();
        let was_normal = column.sizing_mode().is_normal();
        let prev_origin = column.tiles_origin();

        let (tile_idx, tile) = column
            .tiles
            .iter_mut()
            .enumerate()
            .find(|(_, tile)| tile.window().id() == window)
            .unwrap();

        let resize = tile.window_mut().interactive_resize_data();

        // Do this before calling update_window() so it can get up-to-date info.
        if let Some(serial) = serial {
            tile.window_mut().on_commit(serial);
        }

        let prev_width = self.data[col_idx].width;

        column.update_window(window);
        self.data[col_idx].update(column);
        column.update_tile_sizes(false);

        let offset = prev_width - self.data[col_idx].width;

        // Move other columns in tandem with resizing.
        let ongoing_resize_anim = column.tiles[tile_idx].resize_animation().is_some();
        if offset != 0. {
            if self.active_column_idx <= col_idx {
                for col in &mut self.columns[col_idx + 1..] {
                    // If there's a resize animation on the tile (that may have just started in
                    // column.update_window()), then the apparent size change is smooth with no
                    // sudden jumps. This corresponds to adding an X animation to adjacent columns.
                    //
                    // There could also be no resize animation with nonzero offset. This could
                    // happen for example:
                    // - if the window resized on its own, which we don't animate
                    // - if the window resized by less than 10 px (the resize threshold)
                    //
                    // The latter case could also cancel an ongoing resize animation.
                    //
                    // Now, stationary columns shouldn't react to this offset change in any way,
                    // i.e. their apparent X position should jump together with the resize.
                    // However, adjacent columns that are already animating an X movement should
                    // offset their animations to avoid the jump.
                    //
                    // Notably, this is necessary to fix the animation jump when resizing width back
                    // and forth in quick succession (in a way that cancels the resize animation).
                    if ongoing_resize_anim {
                        col.animate_move_from_with_config(
                            offset,
                            self.options.animations.window_resize.anim,
                        );
                    } else {
                        col.offset_move_anim_current(offset);
                    }
                }
            } else {
                for col in &mut self.columns[..=col_idx] {
                    if ongoing_resize_anim {
                        col.animate_move_from_with_config(
                            -offset,
                            self.options.animations.window_resize.anim,
                        );
                    } else {
                        col.offset_move_anim_current(-offset);
                    }
                }
            }
        }

        // When a column goes between fullscreen and non-fullscreen, the tiles origin can change.
        // The change comes from things like ignoring struts and hiding the tab indicator in
        // fullscreen, so both in X and Y directions.
        let column = &mut self.columns[col_idx];
        let new_origin = column.tiles_origin();
        let origin_delta = prev_origin - new_origin;
        if origin_delta != Point::new(0., 0.) {
            for (tile, _pos) in column.tiles_mut() {
                tile.animate_move_from(origin_delta);
            }
        }

        if col_idx == self.active_column_idx {
            // If offset == 0, then don't mess with the view or the gesture. Some clients (Firefox,
            // Chromium, Electron) currently don't commit after the ack of a configure that drops
            // the Resizing state, which can trigger this code path for a while.
            let resize = if offset != 0. { resize } else { None };
            if let Some(resize) = resize {
                // Don't bother with the gesture.
                self.view_offset.cancel_gesture();

                // If this is an interactive resize commit of an active window, then we need to
                // either preserve the view offset or adjust it accordingly.
                let centered = self.is_centering_focused_column();

                let width = self.data[col_idx].width;
                let offset = if centered {
                    // FIXME: when view_offset becomes fractional, this can be made additive too.
                    let new_offset =
                        -(self.working_area.size.w - width) / 2. - self.working_area.loc.x;
                    new_offset - self.view_offset.target()
                } else if resize.edges.contains(ResizeEdge::LEFT) {
                    -offset
                } else {
                    0.
                };

                self.view_offset.offset(offset);
            }

            // When the active column goes fullscreen, store the view offset to restore later.
            let is_normal = self.columns[col_idx].sizing_mode().is_normal();
            if was_normal && !is_normal {
                self.view_offset_to_restore = Some(self.view_offset.stationary());
            }

            // Upon unfullscreening, restore the view offset.
            //
            // In tabbed display mode, there can be multiple tiles in a fullscreen column. They
            // will unfullscreen one by one, and the column width will shrink only when the
            // last tile unfullscreens. This is when we want to restore the view offset,
            // otherwise it will immediately reset back by the animate_view_offset below.
            let unfullscreen_offset = if !was_normal && is_normal {
                // Take the value unconditionally, even if the view is currently frozen by
                // a view gesture. It shouldn't linger around because it's only valid for this
                // particular unfullscreen.
                self.view_offset_to_restore.take()
            } else {
                None
            };

            // We might need to move the view to ensure the resized window is still visible. But
            // only do it when the view isn't frozen by an interactive resize or a view gesture.
            if self.interactive_resize.is_none() && !self.view_offset.is_gesture() {
                // Restore the view offset upon unfullscreening if needed.
                if let Some(prev_offset) = unfullscreen_offset {
                    self.animate_view_offset(col_idx, prev_offset);
                }

                // Synchronize the horizontal view movement with the resize so that it looks nice.
                // This is especially important for always-centered view.
                let config = if ongoing_resize_anim {
                    self.options.animations.window_resize.anim
                } else {
                    self.options.animations.horizontal_view_movement.0
                };

                // FIXME: we will want to skip the animation in some cases here to make continuously
                // resizing windows not look janky.
                self.animate_view_offset_to_column_with_config(None, col_idx, None, config);
            }
        }
    }

    pub fn scroll_amount_to_activate(&self, window: &W::Id) -> f64 {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();

        if self.active_column_idx == column_idx {
            return 0.;
        }

        // Consider the end of an ongoing animation because that's what compute to fit does too.
        let target_x = self.target_view_pos();
        let new_view_offset = self.compute_new_view_offset_for_column(
            Some(target_x),
            column_idx,
            Some(self.active_column_idx),
        );

        let new_col_x = self.column_x(column_idx);
        let from_view_offset = target_x - new_col_x;

        (from_view_offset - new_view_offset).abs() / self.working_area.size.w
    }

    pub fn activate_window(&mut self, window: &W::Id) -> bool {
        let column_idx = self.columns.iter().position(|col| col.contains(window));
        let Some(column_idx) = column_idx else {
            return false;
        };
        let column = &mut self.columns[column_idx];

        column.activate_window(window);
        self.activate_column(column_idx);

        true
    }
}
