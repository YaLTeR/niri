// ScrollingSpace core implementation - construction, configuration, animation, basic queries, and column access
//
// This file contains the ScrollingSpace methods for:
// - Construction and configuration
// - Animation advancement and state
// - Basic queries and accessors
// - Column positioning and access
// - Render element updates

use std::iter::{self, zip};
use std::rc::Rc;

use niri_ipc::ColumnDisplay;
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::super::super::tile::Tile;
use super::super::super::{LayoutElement, Options};
use super::super::types::ColumnData;
use super::super::utils::compute_working_area;
use crate::utils::transaction::TransactionBlocker;
use super::super::ViewOffset;
use super::{ScrollingSpace};

#[cfg(test)]
use tracing::warn;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn new(
        view_size: Size<f64, Logical>,
        parent_area: Rectangle<f64, Logical>,
        scale: f64,
        clock: crate::animation::Clock,
        options: Rc<Options>,
    ) -> Self {
        let working_area = compute_working_area(parent_area, scale, options.layout.struts);

        Self {
            columns: Vec::new(),
            data: Vec::new(),
            active_column_idx: 0,
            interactive_resize: None,
            view_offset: ViewOffset::Static(0.),
            activate_prev_column_on_removal: None,
            view_offset_to_restore: None,
            closing_windows: Vec::new(),
            view_size,
            working_area,
            parent_area,
            scale,
            clock,
            options,
        }
    }

    /// Returns a snapshot of the logical layout state in a parsable format.
    /// This captures direction-agnostic properties: structure, widths, sizes.
    /// Does NOT include render positions (x coordinates), as those are direction-dependent.
    /// 
    /// Format is designed to be easily parsable for generating both LTR and RTL golden files.
    /// Each line follows a consistent key=value or key:value pattern.
    pub fn snapshot(&self) -> String {
        #[cfg(test)]
        {
            tracing::debug!(
                "📸 SNAPSHOT: columns={}, active={}, view_offset={:?}, view_pos={:.1}, rtl={}",
                self.columns.len(),
                self.active_column_idx,
                self.view_offset,
                self.view_pos(),
                self.options.layout.right_to_left
            );
        }
        
        let mut s = String::new();
        
        // View/output dimensions (needed for RTL calculations)
        s.push_str(&format!("view_width={:.0}\n", self.view_size.w));
        s.push_str(&format!("view_height={:.0}\n", self.view_size.h));
        
        // Scale factor (affects pixel rounding)
        s.push_str(&format!("scale={}\n", self.scale));
        
        // Working area (after struts and layer-shell exclusive zones)
        s.push_str(&format!("working_area_x={:.0}\n", self.working_area.loc.x));
        s.push_str(&format!("working_area_y={:.0}\n", self.working_area.loc.y));
        s.push_str(&format!("working_area_width={:.0}\n", self.working_area.size.w));
        s.push_str(&format!("working_area_height={:.0}\n", self.working_area.size.h));
        
        // Parent area (excluding struts, for popup positioning)
        s.push_str(&format!("parent_area_x={:.0}\n", self.parent_area.loc.x));
        s.push_str(&format!("parent_area_y={:.0}\n", self.parent_area.loc.y));
        s.push_str(&format!("parent_area_width={:.0}\n", self.parent_area.size.w));
        s.push_str(&format!("parent_area_height={:.0}\n", self.parent_area.size.h));
        
        // Layout options that affect positioning
        s.push_str(&format!("gaps={}\n", self.options.layout.gaps));
        
        // View offset (logical scroll position)
        s.push_str(&format!("view_offset={:?}\n", self.view_offset));
        
        // View position (transformed coordinate for rendering)
        s.push_str(&format!("view_pos={:.1}\n", self.view_pos()));
        
        // Active column index
        s.push_str(&format!("active_column={}\n", self.active_column_idx));
        
        // Compute column X positions
        let col_xs: Vec<f64> = self.column_xs(self.data.iter().copied()).take(self.columns.len()).collect();
        
        // Active column position in content space
        if !self.columns.is_empty() {
            let active_col_x = col_xs[self.active_column_idx];
            s.push_str(&format!("active_column_x={:.1}\n", active_col_x));
        }
        
        // Active tile position in viewport space (where it appears on screen)
        if !self.columns.is_empty() {
            let active_col = &self.columns[self.active_column_idx];
            if !active_col.tiles.is_empty() {
                let active_col_x = col_xs[self.active_column_idx];
                let view_pos = self.view_pos();
                
                // In viewport: active column is at (active_col_x - view_pos, 0)
                let active_tile_viewport_x = active_col_x - view_pos;
                
                // For Y position, we need to compute tile Y offsets within the column
                let active_tile_idx = active_col.active_tile_idx;
                let gaps = self.options.layout.gaps;
                let mut y = 0.0;
                for i in 0..active_tile_idx {
                    y += active_col.data[i].size.h + gaps;
                }
                
                s.push_str(&format!("active_tile_viewport_x={:.1}\n", active_tile_viewport_x));
                s.push_str(&format!("active_tile_viewport_y={:.1}\n", y));
            }
        }
        
        // Column and tile structure
        for (i, col) in self.columns.iter().enumerate() {
            let is_active_col = i == self.active_column_idx;
            let col_x = col_xs[i];
            
            s.push_str(&format!(
                "column[{}]{}: x={:.1} width={:?} active_tile={}\n",
                i,
                if is_active_col { " [ACTIVE]" } else { "" },
                col_x,
                col.width,
                col.active_tile_idx
            ));
            
            let mut tile_y = 0.0;
            for (j, tile) in col.tiles.iter().enumerate() {
                let data = &col.data[j];
                let is_active_tile = is_active_col && j == col.active_tile_idx;
                
                s.push_str(&format!(
                    "  tile[{}]{}: x={:.1} y={:.1} w={:.0} h={:.0} window_id={:?}\n",
                    j,
                    if is_active_tile { " [ACTIVE]" } else { "" },
                    col_x,
                    tile_y,
                    data.size.w,
                    data.size.h,
                    tile.window().id()
                ));
                
                tile_y += data.size.h + self.options.layout.gaps;
            }
        }
        
        s
    }

    pub fn update_config(
        &mut self,
        view_size: Size<f64, Logical>,
        parent_area: Rectangle<f64, Logical>,
        scale: f64,
        options: Rc<Options>,
    ) {
        let working_area = compute_working_area(parent_area, scale, options.layout.struts);

        for (column, data) in zip(&mut self.columns, &mut self.data) {
            column.update_config(view_size, working_area, parent_area, scale, options.clone());
            data.update(column);
        }

        self.view_size = view_size;
        self.working_area = working_area;
        self.parent_area = parent_area;
        self.scale = scale;
        self.options = options;

        // Apply always-center and such right away.
        if !self.columns.is_empty() && !self.view_offset.is_gesture() {
            self.animate_view_offset_to_column(None, self.active_column_idx, None);
        }
    }

    pub fn update_shaders(&mut self) {
        for tile in self.tiles_mut() {
            tile.update_shaders();
        }
    }

    pub fn advance_animations(&mut self) {
        if let ViewOffset::Animation(anim) = &self.view_offset {
            if anim.is_done() {
                self.view_offset = ViewOffset::Static(anim.to());
            }
        }

        if let ViewOffset::Gesture(gesture) = &mut self.view_offset {
            // Make sure the last event time doesn't go too much out of date (for
            // workspaces not under cursor), causing sudden jumps.
            //
            // This happens after any dnd_scroll_gesture_scroll() calls (in
            // Layout::advance_animations()), so it doesn't mess up the time delta there.
            if let Some(last_time) = &mut gesture.dnd_last_event_time {
                let now = self.clock.now_unadjusted();
                if *last_time != now {
                    *last_time = now;

                    // If last_time was already == now, then dnd_scroll_gesture_scroll() must've
                    // updated the gesture already. Therefore, when this code runs, the pointer
                    // must be outside the DnD scrolling zone.
                    gesture.dnd_nonzero_start_time = None;
                }
            }

            if let Some(anim) = &mut gesture.animation {
                if anim.is_done() {
                    gesture.animation = None;
                }
            }
        }

        for col in &mut self.columns {
            col.advance_animations();
        }

        self.closing_windows.retain_mut(|closing| {
            closing.advance_animations();
            closing.are_animations_ongoing()
        });
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.view_offset.is_animation_ongoing()
            || self.columns.iter().any(|col| col.are_animations_ongoing())
            || !self.closing_windows.is_empty()
    }

    pub fn are_transitions_ongoing(&self) -> bool {
        !self.view_offset.is_static()
            || self.columns.iter().any(|col| col.are_transitions_ongoing())
            || !self.closing_windows.is_empty()
    }

    pub fn update_render_elements(&mut self, is_active: bool) {
        let view_pos = Point::from((self.view_pos(), 0.));
        let view_size = self.view_size;
        let active_idx = self.active_column_idx;
        for (col_idx, (col, col_x)) in self.columns_mut().enumerate() {
            let is_active = is_active && col_idx == active_idx;
            let col_off = Point::from((col_x, 0.));
            let col_pos = view_pos - col_off - col.render_offset();
            let view_rect = Rectangle::new(col_pos, view_size);
            col.update_render_elements(is_active, view_rect);
        }
    }

    pub fn tiles(&self) -> impl Iterator<Item = &Tile<W>> + '_ {
        self.columns.iter().flat_map(|col| col.tiles.iter())
    }

    pub fn tiles_mut(&mut self) -> impl Iterator<Item = &mut Tile<W>> + '_ {
        self.columns.iter_mut().flat_map(|col| col.tiles.iter_mut())
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn active_window(&self) -> Option<&W> {
        if self.columns.is_empty() {
            return None;
        }

        let col = &self.columns[self.active_column_idx];
        Some(col.tiles[col.active_tile_idx].window())
    }

    pub fn active_window_mut(&mut self) -> Option<&mut W> {
        if self.columns.is_empty() {
            return None;
        }

        let col = &mut self.columns[self.active_column_idx];
        Some(col.tiles[col.active_tile_idx].window_mut())
    }

    pub fn active_tile_mut(&mut self) -> Option<&mut Tile<W>> {
        if self.columns.is_empty() {
            return None;
        }

        let col = &mut self.columns[self.active_column_idx];
        Some(&mut col.tiles[col.active_tile_idx])
    }

    pub fn is_active_pending_fullscreen(&self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        let col = &self.columns[self.active_column_idx];
        col.pending_sizing_mode().is_fullscreen()
    }

    pub fn view_pos(&self) -> f64 {
        if self.options.layout.right_to_left {
            // In RTL, columns are already positioned from the right edge
            // view_pos is just the view_offset, not column_x + view_offset
            self.view_offset.current()
        } else {
            // In LTR, view_pos is column_x + view_offset
            self.column_x(self.active_column_idx) + self.view_offset.current()
        }
    }

    pub fn target_view_pos(&self) -> f64 {
        if self.options.layout.right_to_left {
            // In RTL, columns are already positioned from the right edge
            self.view_offset.target()
        } else {
            // In LTR, view_pos is column_x + view_offset
            self.column_x(self.active_column_idx) + self.view_offset.target()
        }
    }

    // HACK: pass a self.data iterator in manually as a workaround for the lack of method partial
    // borrowing. Note that this method's return value does not borrow the entire &Self!
    pub(in crate::layout::scrolling) fn column_xs<'a>(
        &'a self,
        data: impl Iterator<Item = ColumnData> + 'a,
    ) -> Box<dyn Iterator<Item = f64> + 'a> {
        let gaps = self.options.layout.gaps;
        let is_rtl = self.options.layout.right_to_left;
        
        // Chain with a dummy value to be able to get one past all columns' X.
        let dummy = ColumnData { width: 0. };
        let data = data.chain(iter::once(dummy));
        
        if is_rtl {
            // RTL: columns start from the right edge and grow leftward
            let working_width = self.working_area.size.w;
            let mut x = working_width;

            Box::new(data.map(move |data| {
                x -= data.width;
                let rv = x;
                x -= gaps;
                rv
            }))
        } else {
            // LTR: columns start from the left edge and grow rightward
            let mut x = 0.;

            Box::new(data.map(move |data| {
                let rv = x;
                x += data.width + gaps;
                rv
            }))
        }
    }

    pub(in crate::layout::scrolling) fn column_x(&self, column_idx: usize) -> f64 {
        self.column_xs(self.data.iter().copied())
            .nth(column_idx)
            .unwrap()
    }

    fn column_xs_in_render_order<'a>(
        &'a self,
        data: impl Iterator<Item = ColumnData> + 'a,
    ) -> Box<dyn Iterator<Item = f64> + 'a> {
        let active_idx = self.active_column_idx;
        let active_pos = self.column_x(active_idx);
        let offsets = self
            .column_xs(data)
            .enumerate()
            .filter_map(move |(idx, pos)| (idx != active_idx).then_some(pos));
        Box::new(iter::once(active_pos).chain(offsets))
    }

    pub fn columns(&self) -> impl Iterator<Item = &super::super::column::Column<W>> {
        self.columns.iter()
    }

    fn columns_mut(&mut self) -> impl Iterator<Item = (&mut super::super::column::Column<W>, f64)> + '_ {
        let offsets: Vec<_> = self.column_xs(self.data.iter().copied()).collect();
        zip(&mut self.columns, offsets)
    }

    pub fn columns_in_render_order(&self) -> impl Iterator<Item = (&super::super::column::Column<W>, f64)> + '_ {
        let offsets: Vec<_> = self.column_xs_in_render_order(self.data.iter().copied()).collect();

        let (first, active, rest) = if self.columns.is_empty() {
            (&[][..], &[][..], &[][..])
        } else {
            let (first, rest) = self.columns.split_at(self.active_column_idx);
            let (active, rest) = rest.split_at(1);
            (first, active, rest)
        };

        let columns = active.iter().chain(first).chain(rest);
        zip(columns, offsets)
    }

    pub fn columns_in_render_order_mut(&mut self) -> impl Iterator<Item = (&mut super::super::column::Column<W>, f64)> + '_ {
        let offsets: Vec<_> = self.column_xs_in_render_order(self.data.iter().copied()).collect();

        let (first, active, rest) = if self.columns.is_empty() {
            (&mut [][..], &mut [][..], &mut [][..])
        } else {
            let (first, rest) = self.columns.split_at_mut(self.active_column_idx);
            let (active, rest) = rest.split_at_mut(1);
            (first, active, rest)
        };

        let columns = active.iter_mut().chain(first).chain(rest);
        zip(columns, offsets)
    }

    
    #[cfg(test)]
    pub fn active_column_idx(&self) -> usize {
        self.active_column_idx
    }

    #[cfg(test)]
    pub fn view_offset(&self) -> &super::super::types::ViewOffset {
        &self.view_offset
    }

    #[cfg(test)]
    pub fn view_size(&self) -> Size<f64, Logical> {
        self.view_size
    }

    #[cfg(test)]
    pub fn parent_area(&self) -> Rectangle<f64, Logical> {
        self.parent_area
    }

    #[cfg(test)]
    pub fn clock(&self) -> &crate::animation::Clock {
        &self.clock
    }

    #[cfg(test)]
    pub fn options(&self) -> &Rc<Options> {
        &self.options
    }

    pub fn set_column_display(&mut self, display: ColumnDisplay) {
        if self.columns.is_empty() {
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        col.set_column_display(display);
    }

    pub fn start_close_animation_for_window(
        &mut self,
        renderer: &mut smithay::backend::renderer::gles::GlesRenderer,
        window: &W::Id,
        blocker: TransactionBlocker,
    ) {
        let (tile, mut tile_pos) = self
            .tiles_with_render_positions_mut(false)
            .find(|(tile, _)| tile.window().id() == window)
            .unwrap();

        let Some(snapshot) = tile.take_unmap_snapshot() else {
            return;
        };

        let tile_size = tile.tile_size();

        let (col_idx, tile_idx) = self
            .columns
            .iter()
            .enumerate()
            .find_map(|(col_idx, col)| {
                col.tiles()
                    .enumerate()
                    .find(|(_, (tile, _))| tile.window().id() == window)
                    .map(move |(tile_idx, _)| (col_idx, tile_idx))
            })
            .unwrap();

        let col = &self.columns[col_idx];
        let removing_last = col.tiles().count() == 1;

        // Skip closing animation for invisible tiles in a tabbed column.
        if col.display_mode == ColumnDisplay::Tabbed && tile_idx != col.active_tile_idx {
            return;
        }

        tile_pos.x += self.view_pos();

        if col_idx < self.active_column_idx {
            let offset = if removing_last {
                self.column_x(col_idx + 1) - self.column_x(col_idx)
            } else {
                self.data[col_idx].width
                    - col
                        .tiles()
                        .enumerate()
                        .filter_map(|(idx, (tile, _))| {
                            (idx != tile_idx).then_some(tile.tile_size().w)
                        })
                        .fold(0.0, f64::max)
            };
            tile_pos.x -= offset;
        }

        self.start_close_animation_for_tile(renderer, snapshot, tile_size, tile_pos, blocker);
    }

    fn start_close_animation_for_tile(
        &mut self,
        renderer: &mut smithay::backend::renderer::gles::GlesRenderer,
        snapshot: super::super::super::tile::TileRenderSnapshot,
        tile_size: Size<f64, Logical>,
        tile_pos: Point<f64, Logical>,
        blocker: TransactionBlocker,
    ) {
        use super::super::super::closing_window::ClosingWindow;
        use crate::animation::Animation;
        use smithay::utils::Scale;

        let anim = Animation::new(
            self.clock.clone(),
            0.,
            1.,
            0.,
            self.options.animations.window_close.anim,
        );

        let blocker = if self.options.disable_transactions {
            TransactionBlocker::completed()
        } else {
            blocker
        };

        let scale = Scale::from(self.scale);
        let res = ClosingWindow::new(
            renderer, snapshot, scale, tile_size, tile_pos, blocker, anim,
        );
        match res {
            Ok(closing) => {
                self.closing_windows.push(closing);
            }
            Err(err) => {
                warn!("error creating a closing window animation: {err:?}");
            }
        }
    }

    pub fn start_open_animation(&mut self, id: &W::Id) -> bool {
        self.columns
            .iter_mut()
            .any(|col| col.start_open_animation(id))
    }

    #[cfg(test)]
    pub fn verify_invariants(&self) {
        assert!(self.view_size.w > 0.);
        assert!(self.view_size.h > 0.);
        assert!(self.scale > 0.);
        assert!(self.scale.is_finite());
        assert_eq!(self.columns.len(), self.data.len());
        assert_eq!(
            self.working_area,
            super::super::utils::compute_working_area(self.parent_area, self.scale, self.options.layout.struts)
        );

        if !self.columns.is_empty() {
            assert!(self.active_column_idx < self.columns.len());

            for (column, data) in std::iter::zip(&self.columns, &self.data) {
                assert!(std::rc::Rc::ptr_eq(&self.options, column.options()));
                assert_eq!(&self.clock, column.clock());
                assert_eq!(self.scale, column.scale());
                column.verify_invariants();

                let mut data2 = *data;
                data2.width = column.width();
                assert_eq!(data2, *data);
            }
        }
    }
}
