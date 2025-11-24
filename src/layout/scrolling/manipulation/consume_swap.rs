use std::rc::Rc;

use niri_ipc::ColumnDisplay;
use smithay::utils::{Logical, Point};

use super::super::ScrollingSpace;
use crate::layout::LayoutElement;
use super::super::super::{RemovedTile, ScrollDirection};
use super::super::types::ScrollDirection as Direction;
use crate::utils::transaction::Transaction;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn consume_or_expel_window_left(&mut self, window: Option<&W::Id>) {
        if self.columns.is_empty() {
            return;
        }

        let (source_col_idx, source_tile_idx) = if let Some(window) = window {
            self.columns
                .iter_mut()
                .enumerate()
                .find_map(|(col_idx, col)| {
                    col.tiles
                        .iter()
                        .position(|tile| tile.window().id() == window)
                        .map(|tile_idx| (col_idx, tile_idx))
                })
                .unwrap()
        } else {
            let source_col_idx = self.active_column_idx;
            let source_tile_idx = self.columns[self.active_column_idx].active_tile_idx;
            (source_col_idx, source_tile_idx)
        };

        let source_column = &self.columns[source_col_idx];
        let prev_off = source_column.tile_offset(source_tile_idx);

        let source_tile_was_active = self.active_column_idx == source_col_idx
            && source_column.active_tile_idx == source_tile_idx;

        if source_column.tiles.len() == 1 {
            if source_col_idx == 0 {
                return;
            }

            // Move into adjacent column.
            let target_column_idx = source_col_idx - 1;

            let offset = if self.active_column_idx <= source_col_idx {
                // Tiles to the right animate from the following column.
                self.column_x(source_col_idx) - self.column_x(target_column_idx)
            } else {
                // Tiles to the left animate to preserve their right edge position.
                f64::max(
                    0.,
                    self.data[target_column_idx].width - self.data[source_col_idx].width,
                )
            };
            let mut offset = Point::from((offset, 0.));

            if source_tile_was_active {
                // Make sure the previous (target) column is activated so the animation looks right.
                //
                // However, if it was already going to be activated, leave the offset as is. This
                // improves the workflow that has become common with tabbed columns: open a new
                // window, then immediately consume it left as a new tab.
                self.activate_prev_column_on_removal
                    .get_or_insert(self.view_offset.stationary() + offset.x);
            }

            offset.x += self.columns[source_col_idx].render_offset().x;
            let RemovedTile { tile, .. } = self.remove_tile_by_idx(
                source_col_idx,
                0,
                Transaction::new(),
                Some(self.options.animations.window_movement.0),
            );
            self.add_tile_to_column(target_column_idx, None, tile, source_tile_was_active);

            let target_column = &mut self.columns[target_column_idx];
            offset.x -= target_column.render_offset().x;
            offset += prev_off - target_column.tile_offset(target_column.tiles.len() - 1);

            let new_tile = target_column.tiles.last_mut().unwrap();
            new_tile.animate_move_from(offset);
        } else {
            // Move out of column.
            let mut offset = Point::from((source_column.render_offset().x, 0.));

            let removed =
                self.remove_tile_by_idx(source_col_idx, source_tile_idx, Transaction::new(), None);

            // We're inserting into the source column position.
            let target_column_idx = source_col_idx;

            self.add_tile(
                Some(target_column_idx),
                removed.tile,
                source_tile_was_active,
                removed.width,
                removed.is_full_width,
                Some(self.options.animations.window_movement.0),
            );

            if source_tile_was_active {
                // We added to the left, don't activate even further left on removal.
                self.activate_prev_column_on_removal = None;
            }

            if target_column_idx < self.active_column_idx {
                // Tiles to the left animate from the following column.
                offset.x += self.column_x(target_column_idx + 1) - self.column_x(target_column_idx);
            }

            let new_col = &mut self.columns[target_column_idx];
            offset += prev_off - new_col.tile_offset(0);
            new_col.tiles[0].animate_move_from(offset);
        }
    }

    pub fn consume_or_expel_window_right(&mut self, window: Option<&W::Id>) {
        if self.columns.is_empty() {
            return;
        }

        let (source_col_idx, source_tile_idx) = if let Some(window) = window {
            self.columns
                .iter_mut()
                .enumerate()
                .find_map(|(col_idx, col)| {
                    col.tiles
                        .iter()
                        .position(|tile| tile.window().id() == window)
                        .map(|tile_idx| (col_idx, tile_idx))
                })
                .unwrap()
        } else {
            let source_col_idx = self.active_column_idx;
            let source_tile_idx = self.columns[self.active_column_idx].active_tile_idx;
            (source_col_idx, source_tile_idx)
        };

        let cur_x = self.column_x(source_col_idx);

        let source_column = &self.columns[source_col_idx];
        let mut offset = Point::from((source_column.render_offset().x, 0.));
        let prev_off = source_column.tile_offset(source_tile_idx);

        let source_tile_was_active = self.active_column_idx == source_col_idx
            && source_column.active_tile_idx == source_tile_idx;

        if source_column.tiles.len() == 1 {
            if source_col_idx + 1 == self.columns.len() {
                return;
            }

            // Move into adjacent column.
            let target_column_idx = source_col_idx;

            offset.x += cur_x - self.column_x(source_col_idx + 1);
            offset.x -= self.columns[source_col_idx + 1].render_offset().x;

            if source_tile_was_active {
                // Make sure the target column gets activated.
                self.activate_prev_column_on_removal = None;
            }

            let RemovedTile { tile, .. } = self.remove_tile_by_idx(
                source_col_idx,
                0,
                Transaction::new(),
                Some(self.options.animations.window_movement.0),
            );
            self.add_tile_to_column(target_column_idx, None, tile, source_tile_was_active);

            let target_column = &mut self.columns[target_column_idx];
            offset += prev_off - target_column.tile_offset(target_column.tiles.len() - 1);

            let new_tile = target_column.tiles.last_mut().unwrap();
            new_tile.animate_move_from(offset);
        } else {
            // Move out of column.
            let prev_width = self.data[source_col_idx].width;

            let removed =
                self.remove_tile_by_idx(source_col_idx, source_tile_idx, Transaction::new(), None);

            let target_column_idx = source_col_idx + 1;

            self.add_tile(
                Some(target_column_idx),
                removed.tile,
                source_tile_was_active,
                removed.width,
                removed.is_full_width,
                Some(self.options.animations.window_movement.0),
            );

            offset.x += if self.active_column_idx <= target_column_idx {
                // Tiles to the right animate to the following column.
                cur_x - self.column_x(target_column_idx)
            } else {
                // Tiles to the left animate for a change in width.
                -f64::max(0., prev_width - self.data[target_column_idx].width)
            };

            let new_col = &mut self.columns[target_column_idx];
            offset += prev_off - new_col.tile_offset(0);
            new_col.tiles[0].animate_move_from(offset);
        }
    }

    pub fn consume_into_column(&mut self) {
        if self.columns.len() < 2 {
            return;
        }

        if self.active_column_idx == self.columns.len() - 1 {
            return;
        }

        let target_column_idx = self.active_column_idx;
        let source_column_idx = self.active_column_idx + 1;

        let offset = self.column_x(source_column_idx)
            + self.columns[source_column_idx].render_offset().x
            - self.column_x(target_column_idx);
        let mut offset = Point::from((offset, 0.));
        let prev_off = self.columns[source_column_idx].tile_offset(0);

        let removed = self.remove_tile_by_idx(source_column_idx, 0, Transaction::new(), None);
        self.add_tile_to_column(target_column_idx, None, removed.tile, false);

        let target_column = &mut self.columns[target_column_idx];
        offset += prev_off - target_column.tile_offset(target_column.tiles.len() - 1);
        offset.x -= target_column.render_offset().x;

        let new_tile = target_column.tiles.last_mut().unwrap();
        new_tile.animate_move_from(offset);
    }

    pub fn expel_from_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let source_col_idx = self.active_column_idx;
        let target_col_idx = self.active_column_idx + 1;
        let cur_x = self.column_x(source_col_idx);

        let source_column = &self.columns[self.active_column_idx];
        if source_column.tiles.len() == 1 {
            return;
        }

        let source_tile_idx = source_column.tiles.len() - 1;

        let mut offset = Point::from((source_column.render_offset().x, 0.));
        let prev_off = source_column.tile_offset(source_tile_idx);

        let removed =
            self.remove_tile_by_idx(source_col_idx, source_tile_idx, Transaction::new(), None);

        self.add_tile(
            Some(target_col_idx),
            removed.tile,
            false,
            removed.width,
            removed.is_full_width,
            Some(self.options.animations.window_movement.0),
        );

        offset.x += cur_x - self.column_x(target_col_idx);

        let new_col = &mut self.columns[target_col_idx];
        offset += prev_off - new_col.tile_offset(0);
        new_col.tiles[0].animate_move_from(offset);
    }

    pub fn swap_window_in_direction(&mut self, direction: Direction) {
        if self.columns.is_empty() {
            return;
        }

        // if this is the first (resp. last column), then this operation is equivalent
        // to an `consume_or_expel_window_left` (resp. `consume_or_expel_window_right`)
        match direction {
            Direction::Left => {
                if self.active_column_idx == 0 {
                    return;
                }
            }
            Direction::Right => {
                if self.active_column_idx == self.columns.len() - 1 {
                    return;
                }
            }
        }

        let source_column_idx = self.active_column_idx;
        let target_column_idx = self.active_column_idx.wrapping_add_signed(match direction {
            Direction::Left => -1,
            Direction::Right => 1,
        });

        // if both source and target columns contain a single tile, then the operation is equivalent
        // to a simple column move
        if self.columns[source_column_idx].tiles.len() == 1
            && self.columns[target_column_idx].tiles.len() == 1
        {
            return self.move_column_to(target_column_idx);
        }

        let source_tile_idx = self.columns[source_column_idx].active_tile_idx;
        let target_tile_idx = self.columns[target_column_idx].active_tile_idx;
        let source_column_drained = self.columns[source_column_idx].tiles.len() == 1;

        // capture the original positions of the tiles
        let (mut source_pt, mut target_pt) = (
            self.columns[source_column_idx].render_offset()
                + self.columns[source_column_idx].tile_offset(source_tile_idx),
            self.columns[target_column_idx].render_offset()
                + self.columns[target_column_idx].tile_offset(target_tile_idx),
        );
        source_pt.x += self.column_x(source_column_idx);
        target_pt.x += self.column_x(target_column_idx);

        let transaction = Transaction::new();

        // If the source column contains a single tile, this will also remove the column.
        // When this happens `source_column_drained` will be set and the column will need to be
        // recreated with `add_tile`
        let source_removed = self.remove_tile_by_idx(
            source_column_idx,
            source_tile_idx,
            transaction.clone(),
            None,
        );

        {
            // special case when the source column disappears after removing its last tile
            let adjusted_target_column_idx =
                if direction == Direction::Right && source_column_drained {
                    target_column_idx - 1
                } else {
                    target_column_idx
                };

            self.add_tile_to_column(
                adjusted_target_column_idx,
                Some(target_tile_idx),
                source_removed.tile,
                false,
            );

            let RemovedTile {
                tile: target_tile, ..
            } = self.remove_tile_by_idx(
                adjusted_target_column_idx,
                target_tile_idx + 1,
                transaction.clone(),
                None,
            );

            if source_column_drained {
                // recreate the drained column with only the target tile
                self.add_tile(
                    Some(source_column_idx),
                    target_tile,
                    true,
                    source_removed.width,
                    source_removed.is_full_width,
                    None,
                )
            } else {
                // simply add the removed target tile to the source column
                self.add_tile_to_column(
                    source_column_idx,
                    Some(source_tile_idx),
                    target_tile,
                    false,
                );
            }
        }

        // update the active tile in the modified columns
        self.columns[source_column_idx].active_tile_idx = source_tile_idx;
        self.columns[target_column_idx].active_tile_idx = target_tile_idx;

        // Animations
        self.columns[target_column_idx].tiles[target_tile_idx]
            .animate_move_from(source_pt - target_pt);
        self.columns[target_column_idx].tiles[target_tile_idx].ensure_alpha_animates_to_1();

        // FIXME: this stop_move_animations() causes the target tile animation to "reset" when
        // swapping. It's here as a workaround to stop the unwanted animation of moving the source
        // tile down when adding the target tile above it. This code needs to be written in some
        // other way not to trigger that animation, or to cancel it properly, so that swap doesn't
        // cancel all ongoing target tile animations.
        self.columns[source_column_idx].tiles[source_tile_idx].stop_move_animations();
        self.columns[source_column_idx].tiles[source_tile_idx]
            .animate_move_from(target_pt - source_pt);
        self.columns[source_column_idx].tiles[source_tile_idx].ensure_alpha_animates_to_1();

        self.activate_column(target_column_idx);
    }

    pub fn toggle_column_tabbed_display(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        let display = match col.display_mode {
            ColumnDisplay::Normal => ColumnDisplay::Tabbed,
            ColumnDisplay::Tabbed => ColumnDisplay::Normal,
        };

        col.set_column_display(display);
    }
}
