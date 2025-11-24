// Core Column implementation - construction, configuration, animation, rendering, state queries, and tile management
//
// This file contains the fundamental Column methods for:
// - Construction and configuration
// - Animation advancement and state
// - Rendering element updates
// - Basic state queries
// - Tile management operations

use std::iter::zip;
use std::rc::Rc;

use niri_ipc::ColumnDisplay;
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::super::super::tab_indicator::{TabIndicator, TabInfo};
use super::super::super::tile::Tile;
use super::super::super::{LayoutElement, Options};
use super::super::types::{MoveAnimation, TileData, WindowHeight};
use super::{Column, ColumnWidth};
use crate::animation::Animation;
use crate::layout::SizingMode;

#[cfg(test)]
use crate::animation::Clock as TestClock;

impl<W: LayoutElement> Column<W> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::layout::scrolling) fn new_with_tile(
        tile: Tile<W>,
        view_size: Size<f64, Logical>,
        working_area: Rectangle<f64, Logical>,
        parent_area: Rectangle<f64, Logical>,
        scale: f64,
        width: ColumnWidth,
        is_full_width: bool,
    ) -> Self {
        let options = tile.options.clone();

        let display_mode = tile
            .window()
            .rules()
            .default_column_display
            .unwrap_or(options.layout.default_column_display);

        // Try to match width to a preset width. Consider the following case: a terminal (foot)
        // sizes itself to the terminal grid. We open it with default-column-width 0.5. It shrinks
        // by a few pixels to evenly match the terminal grid. Then we press
        // switch-preset-column-width intending to go to proportion 0.667, but the preset width
        // matching code picks the proportion 0.5 preset because it's the next smallest width after
        // the current foot's window width. Effectively, this makes the first
        // switch-preset-column-width press ignored.
        //
        // However, here, we do know that width = proportion 0.5 (regardless of what the window
        // opened with), and we can match it to a preset right away, if one exists.
        let preset_width_idx = options
            .layout
            .preset_column_widths
            .iter()
            .position(|preset| width == ColumnWidth::from(*preset));

        let mut rv = Self {
            tiles: vec![],
            data: vec![],
            active_tile_idx: 0,
            width,
            preset_width_idx,
            is_full_width,
            is_pending_maximized: false,
            is_pending_fullscreen: false,
            display_mode,
            tab_indicator: TabIndicator::new(options.layout.tab_indicator),
            move_animation: None,
            view_size,
            working_area,
            parent_area,
            scale,
            clock: tile.clock.clone(),
            options,
        };

        let pending_sizing_mode = tile.window().pending_sizing_mode();

        rv.add_tile_at(0, tile);

        match pending_sizing_mode {
            SizingMode::Normal => (),
            SizingMode::Maximized => rv.set_maximized(true),
            SizingMode::Fullscreen => rv.set_fullscreen(true),
        }

        // Animate the tab indicator for new columns.
        if display_mode == ColumnDisplay::Tabbed
            && !rv.options.layout.tab_indicator.hide_when_single_tab
            && rv.sizing_mode().is_normal()
        {
            // Usually new columns are created together with window movement actions. For new
            // windows, we handle that in start_open_animation().
            rv.tab_indicator
                .start_open_animation(rv.clock.clone(), rv.options.animations.window_movement.0);
        }

        rv
    }

    pub(in crate::layout::scrolling) fn update_config(
        &mut self,
        view_size: Size<f64, Logical>,
        working_area: Rectangle<f64, Logical>,
        parent_area: Rectangle<f64, Logical>,
        scale: f64,
        options: Rc<Options>,
    ) {
        let mut update_sizes = false;

        if self.view_size != view_size
            || self.working_area != working_area
            || self.parent_area != parent_area
        {
            update_sizes = true;
        }

        // If preset widths changed, clear our stored preset index.
        if self.options.layout.preset_column_widths != options.layout.preset_column_widths {
            self.preset_width_idx = None;
        }

        // If preset heights changed, make our heights non-preset.
        if self.options.layout.preset_window_heights != options.layout.preset_window_heights {
            self.convert_heights_to_auto();
            update_sizes = true;
        }

        if self.options.layout.gaps != options.layout.gaps {
            update_sizes = true;
        }

        if self.options.layout.border.off != options.layout.border.off
            || self.options.layout.border.width != options.layout.border.width
        {
            update_sizes = true;
        }

        if self.options.layout.tab_indicator != options.layout.tab_indicator {
            update_sizes = true;
        }

        for (tile, data) in zip(&mut self.tiles, &mut self.data) {
            tile.update_config(view_size, scale, options.clone());
            data.update(tile);
        }

        self.tab_indicator
            .update_config(options.layout.tab_indicator);
        self.view_size = view_size;
        self.working_area = working_area;
        self.parent_area = parent_area;
        self.scale = scale;
        self.options = options;

        if update_sizes {
            self.update_tile_sizes(false);
        }
    }

    pub fn advance_animations(&mut self) {
        if let Some(move_) = &mut self.move_animation {
            if move_.anim.is_done() {
                self.move_animation = None;
            }
        }

        for tile in &mut self.tiles {
            tile.advance_animations();
        }

        self.tab_indicator.advance_animations();
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.move_animation.is_some()
            || self.tab_indicator.are_animations_ongoing()
            || self.tiles.iter().any(Tile::are_animations_ongoing)
    }

    pub fn are_transitions_ongoing(&self) -> bool {
        self.move_animation.is_some()
            || self.tab_indicator.are_animations_ongoing()
            || self.tiles.iter().any(Tile::are_transitions_ongoing)
    }

    pub fn update_render_elements(&mut self, is_active: bool, view_rect: Rectangle<f64, Logical>) {
        let active_idx = self.active_tile_idx;
        for (tile_idx, (tile, tile_off)) in self.tiles_mut().enumerate() {
            let is_active = is_active && tile_idx == active_idx;

            let mut tile_view_rect = view_rect;
            tile_view_rect.loc -= tile_off + tile.render_offset();
            tile.update_render_elements(is_active, tile_view_rect);
        }

        let config = self.tab_indicator.config();
        let offsets = self.tile_offsets_iter(self.data.iter().copied());
        let tabs = zip(&self.tiles, offsets)
            .enumerate()
            .map(|(tile_idx, (tile, tile_off))| {
                let is_active = tile_idx == active_idx;
                let is_urgent = tile.window().is_urgent();
                let tile_pos = tile_off + tile.render_offset();
                TabInfo::from_tile(tile, tile_pos, is_active, is_urgent, &config)
            });

        // Hide the tab indicator in fullscreen. If you have it configured to overlap the window,
        // you don't want that to happen in fullscreen. Also, laying things out correctly when the
        // tab indicator is within the column and the column goes fullscreen, would require too
        // many changes to the code for too little benefit (it's mostly invisible anyway).
        let enabled = self.display_mode == ColumnDisplay::Tabbed && self.sizing_mode().is_normal();

        self.tab_indicator.update_render_elements(
            enabled,
            self.tab_indicator_area(),
            view_rect,
            self.tiles.len(),
            tabs,
            is_active,
            self.scale,
        );
    }

    pub fn render_offset(&self) -> Point<f64, Logical> {
        let mut offset = Point::from((0., 0.));

        if let Some(move_) = &self.move_animation {
            offset.x += move_.from * move_.anim.value();
        }

        offset
    }

    pub fn animate_move_from(&mut self, from_x_offset: f64) {
        self.animate_move_from_with_config(
            from_x_offset,
            self.options.animations.window_movement.0,
        );
    }

    pub fn animate_move_from_with_config(
        &mut self,
        from_x_offset: f64,
        config: niri_config::Animation,
    ) {
        let current_offset = self
            .move_animation
            .as_ref()
            .map_or(0., |move_| move_.from * move_.anim.value());

        let anim = Animation::new(self.clock.clone(), 1., 0., 0., config);
        self.move_animation = Some(MoveAnimation {
            anim,
            from: from_x_offset + current_offset,
        });
    }

    pub fn offset_move_anim_current(&mut self, offset: f64) {
        if let Some(move_) = self.move_animation.as_mut() {
            // If the anim is almost done, there's little point trying to offset it; we can let
            // things jump. If it turns out like a bad idea, we could restart the anim instead.
            let value = move_.anim.value();
            if value > 0.001 {
                move_.from += offset / value;
            }
        }
    }

    pub(in crate::layout::scrolling) fn sizing_mode(&self) -> SizingMode {
        // The logic that satisfies these behaviors is to check if *any* tile is fullscreen.
        let mut any_fullscreen = false;
        let mut any_maximized = false;
        for tile in &self.tiles {
            match tile.sizing_mode() {
                SizingMode::Normal => (),
                SizingMode::Maximized => any_maximized = true,
                SizingMode::Fullscreen => any_fullscreen = true,
            }
        }

        if any_fullscreen {
            SizingMode::Fullscreen
        } else if any_maximized {
            SizingMode::Maximized
        } else {
            SizingMode::Normal
        }
    }

    pub fn contains(&self, window: &W::Id) -> bool {
        self.tiles
            .iter()
            .map(Tile::window)
            .any(|win| win.id() == window)
    }

    pub fn position(&self, window: &W::Id) -> Option<usize> {
        self.tiles
            .iter()
            .map(Tile::window)
            .position(|win| win.id() == window)
    }

    pub fn is_pending_fullscreen(&self) -> bool {
        self.is_pending_fullscreen
    }

    pub fn is_pending_maximized(&self) -> bool {
        self.is_pending_maximized
    }

    pub fn pending_sizing_mode(&self) -> SizingMode {
        if self.is_pending_fullscreen {
            SizingMode::Fullscreen
        } else if self.is_pending_maximized {
            SizingMode::Maximized
        } else {
            SizingMode::Normal
        }
    }

    // Public getters for testing and verification
    #[cfg(test)]
    pub fn options(&self) -> &Rc<Options> {
        &self.options
    }

    #[cfg(test)]
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    #[cfg(test)]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub(in crate::layout::scrolling) fn activate_idx(&mut self, idx: usize) -> bool {
        if self.active_tile_idx == idx {
            return false;
        }

        self.active_tile_idx = idx;

        self.tiles[idx].ensure_alpha_animates_to_1();

        true
    }

    pub(in crate::layout::scrolling) fn activate_window(&mut self, window: &W::Id) {
        let idx = self.position(window).unwrap();
        self.activate_idx(idx);
    }

    pub(in crate::layout::scrolling) fn add_tile_at(&mut self, idx: usize, mut tile: Tile<W>) {
        tile.update_config(self.view_size, self.scale, self.options.clone());

        // Inserting a tile pushes down all tiles below it, but also in always-centering mode it
        // will affect the X position of all tiles in the column.
        let mut prev_offsets = Vec::with_capacity(self.tiles.len() + 1);
        prev_offsets.extend(self.tile_offsets().take(self.tiles.len()));

        if self.display_mode != ColumnDisplay::Tabbed {
            self.is_pending_fullscreen = false;
            self.is_pending_maximized = false;
        }

        self.data
            .insert(idx, TileData::new(&tile, WindowHeight::auto_1()));
        self.tiles.insert(idx, tile);
        self.update_tile_sizes(true);

        // Animate tiles according to the offset changes.
        prev_offsets.insert(idx, Point::default());
        for (i, ((tile, offset), prev)) in zip(self.tiles_mut(), prev_offsets).enumerate() {
            if i == idx {
                continue;
            }

            tile.animate_move_from(prev - offset);
        }
    }

    pub(in crate::layout::scrolling) fn update_window(&mut self, window: &W::Id) {
        let (tile_idx, tile) = self
            .tiles
            .iter_mut()
            .enumerate()
            .find(|(_, tile)| tile.window().id() == window)
            .unwrap();

        let prev_height = self.data[tile_idx].size.h;

        tile.update_window();
        self.data[tile_idx].update(tile);

        let offset = prev_height - self.data[tile_idx].size.h;

        let is_tabbed = self.display_mode == ColumnDisplay::Tabbed;

        // Move windows below in tandem with resizing.
        //
        // FIXME: in always-centering mode, window resizing will affect the offsets of all other
        // windows in the column, so they should all be animated. How should this interact with
        // animated vs. non-animated resizes? For example, an animated +20 resize followed by two
        // non-animated -10 resizes.
        if !is_tabbed && offset != 0. {
            if tile.resize_animation().is_some() {
                // If there's a resize animation (that may have just started in
                // tile.update_window()), then the apparent size change is smooth with no sudden
                // jumps. This corresponds to adding an Y animation to tiles below.
                for tile in &mut self.tiles[tile_idx + 1..] {
                    tile.animate_move_y_from_with_config(
                        offset,
                        self.options.animations.window_resize.anim,
                    );
                }
            } else {
                // There's no resize animation, but the offset is nonzero. This could happen for
                // example:
                // - if the window resized on its own, which we don't animate
                // - if the window resized by less than 10 px (the resize threshold)
                //
                // The latter case could also cancel an ongoing resize animation.
                //
                // Now, stationary tiles below shouldn't react to this offset change in any way,
                // i.e. their apparent Y position should jump together with the resize. However,
                // tiles below that are already animating an Y movement should offset their
                // animations to avoid the jump.
                //
                // Notably, this is necessary to fix the animation jump when resizing height back
                // and forth in quick succession (in a way that cancels the resize animation).
                for tile in &mut self.tiles[tile_idx + 1..] {
                    tile.offset_move_y_anim_current(offset);
                }
            }
        }
    }
}
