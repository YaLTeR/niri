use std::rc::Rc;

use super::super::ScrollingSpace;
use crate::layout::LayoutElement;
use super::super::super::Options;
use super::super::column::Column;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn focus_left(&mut self) -> bool {
        if self.active_column_idx == 0 {
            return false;
        }
        self.activate_column(self.active_column_idx - 1);
        true
    }

    pub fn focus_right(&mut self) -> bool {
        if self.active_column_idx + 1 >= self.columns.len() {
            return false;
        }

        self.activate_column(self.active_column_idx + 1);
        true
    }

    pub fn focus_column_first(&mut self) {
        self.activate_column(0);
    }

    pub fn focus_column_last(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.activate_column(self.columns.len() - 1);
    }

    pub fn focus_column(&mut self, index: usize) {
        if self.columns.is_empty() {
            return;
        }

        self.activate_column(index.saturating_sub(1).min(self.columns.len() - 1));
    }

    pub fn focus_window_in_column(&mut self, index: u8) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].focus_index(index);
    }

    pub fn focus_down(&mut self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        self.columns[self.active_column_idx].focus_down()
    }

    pub fn focus_up(&mut self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        self.columns[self.active_column_idx].focus_up()
    }

    pub fn focus_down_or_left(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let column = &mut self.columns[self.active_column_idx];
        if !column.focus_down() {
            self.focus_left();
        }
    }

    pub fn focus_down_or_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let column = &mut self.columns[self.active_column_idx];
        if !column.focus_down() {
            self.focus_right();
        }
    }

    pub fn focus_up_or_left(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let column = &mut self.columns[self.active_column_idx];
        if !column.focus_up() {
            self.focus_left();
        }
    }

    pub fn focus_up_or_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let column = &mut self.columns[self.active_column_idx];
        if !column.focus_up() {
            self.focus_right();
        }
    }

    pub fn focus_top(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].focus_top()
    }

    pub fn focus_bottom(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].focus_bottom()
    }

    pub fn move_column_to_index(&mut self, index: usize) {
        if self.columns.is_empty() {
            return;
        }

        self.move_column_to(index.saturating_sub(1).min(self.columns.len() - 1));
    }

    pub(in crate::layout::scrolling) fn move_column_to(&mut self, new_idx: usize) {
        if self.active_column_idx == new_idx {
            return;
        }

        let current_col_x = self.column_x(self.active_column_idx);
        let next_col_x = self.column_x(self.active_column_idx + 1);

        let mut column = self.columns.remove(self.active_column_idx);
        let data = self.data.remove(self.active_column_idx);
        super::super::space::view_offset::cancel_resize_for_column(&mut self.interactive_resize, &mut column);
        self.columns.insert(new_idx, column);
        self.data.insert(new_idx, data);

        // Preserve the camera position when moving to the left.
        let view_offset_delta = -self.column_x(self.active_column_idx) + current_col_x;
        self.view_offset.offset(view_offset_delta);

        // The column we just moved is offset by the difference between its new and old position.
        let new_col_x = self.column_x(new_idx);
        self.columns[new_idx].animate_move_from(current_col_x - new_col_x);

        // All columns in between moved by the width of the column that we just moved.
        let others_x_offset = next_col_x - current_col_x;
        if self.active_column_idx < new_idx {
            for col in &mut self.columns[self.active_column_idx..new_idx] {
                col.animate_move_from(others_x_offset);
            }
        } else {
            for col in &mut self.columns[new_idx + 1..=self.active_column_idx] {
                col.animate_move_from(-others_x_offset);
            }
        }

        self.activate_column_with_anim_config(new_idx, self.options.animations.window_movement.0);
    }

    pub fn move_left(&mut self) -> bool {
        if self.active_column_idx == 0 {
            return false;
        }

        self.move_column_to(self.active_column_idx - 1);
        true
    }

    pub fn move_right(&mut self) -> bool {
        let new_idx = self.active_column_idx + 1;
        if new_idx >= self.columns.len() {
            return false;
        }

        self.move_column_to(new_idx);
        true
    }

    pub fn move_column_to_first(&mut self) {
        self.move_column_to(0);
    }

    pub fn move_column_to_last(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let new_idx = self.columns.len() - 1;
        self.move_column_to(new_idx);
    }

    pub fn move_down(&mut self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        self.columns[self.active_column_idx].move_down()
    }

    pub fn move_up(&mut self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        self.columns[self.active_column_idx].move_up()
    }
}
