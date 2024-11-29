use std::cmp::{max, min};
use std::iter::zip;
use std::rc::Rc;

use niri_ipc::SizeChange;
use smithay::utils::{Logical, Point, Rectangle, Scale, Serial, Size};

use super::closing_window::ClosingWindowRenderElement;
use super::scrolling::ColumnWidth;
use super::tile::{Tile, TileRenderElement};
use super::workspace::InteractiveResize;
use super::{ConfigureIntent, InteractiveResizeData, LayoutElement, Options, RemovedTile};
use crate::animation::Clock;
use crate::niri_render_elements;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::RenderTarget;
use crate::utils::ResizeEdge;

/// Space for floating windows.
#[derive(Debug)]
pub struct FloatingSpace<W: LayoutElement> {
    /// Tiles in top-to-bottom order.
    tiles: Vec<Tile<W>>,

    /// Extra per-tile data.
    data: Vec<Data>,

    /// Id of the active window.
    ///
    /// The active window is not necessarily the topmost window. Focus-follows-mouse should
    /// activate a window, but not bring it to the top, because that's very annoying.
    ///
    /// This is always set to `Some()` when `tiles` isn't empty.
    active_window_id: Option<W::Id>,

    /// Ongoing interactive resize.
    interactive_resize: Option<InteractiveResize<W>>,

    /// Working area for this space.
    working_area: Rectangle<f64, Logical>,

    /// Scale of the output the space is on (and rounds its sizes to).
    scale: f64,

    /// Clock for driving animations.
    clock: Clock,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

niri_render_elements! {
    FloatingSpaceRenderElement<R> => {
        Tile = TileRenderElement<R>,
        ClosingWindow = ClosingWindowRenderElement,
    }
}

/// Size-relative units.
struct SizeFrac;

/// Extra per-tile data.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Data {
    /// Position relative to the working area.
    pos: Point<f64, SizeFrac>,

    /// Cached position in logical coordinates.
    ///
    /// Not rounded to physical pixels.
    logical_pos: Point<f64, Logical>,

    /// Cached actual size of the tile.
    size: Size<f64, Logical>,

    /// Working area used for conversions.
    working_area: Rectangle<f64, Logical>,
}

impl Data {
    pub fn new<W: LayoutElement>(
        working_area: Rectangle<f64, Logical>,
        tile: &Tile<W>,
        logical_pos: Point<f64, Logical>,
    ) -> Self {
        let mut rv = Self {
            pos: Point::default(),
            logical_pos: Point::default(),
            size: Size::default(),
            working_area,
        };
        rv.update(tile);
        rv.set_logical_pos(logical_pos);
        rv
    }

    fn recompute_logical_pos(&mut self) {
        let mut logical_pos = Point::from((self.pos.x, self.pos.y));
        logical_pos.x *= self.working_area.size.w;
        logical_pos.y *= self.working_area.size.h;
        logical_pos += self.working_area.loc;
        self.logical_pos = logical_pos;
    }

    pub fn update_config(&mut self, working_area: Rectangle<f64, Logical>) {
        if self.working_area == working_area {
            return;
        }

        self.working_area = working_area;
        self.recompute_logical_pos();
    }

    pub fn update<W: LayoutElement>(&mut self, tile: &Tile<W>) {
        self.size = tile.tile_size();
    }

    pub fn set_logical_pos(&mut self, logical_pos: Point<f64, Logical>) {
        let pos = logical_pos - self.working_area.loc;
        let mut pos = Point::from((pos.x, pos.y));
        pos.x /= f64::max(self.working_area.size.w, 1.0);
        pos.y /= f64::max(self.working_area.size.h, 1.0);

        self.pos = pos;

        // This should get close to the same result as what we started with.
        self.recompute_logical_pos();
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        let mut temp = *self;
        temp.recompute_logical_pos();
        assert_eq!(
            self.logical_pos, temp.logical_pos,
            "cached logical pos must be up to date"
        );
    }
}

impl<W: LayoutElement> FloatingSpace<W> {
    pub fn new(
        working_area: Rectangle<f64, Logical>,
        scale: f64,
        clock: Clock,
        options: Rc<Options>,
    ) -> Self {
        Self {
            tiles: Vec::new(),
            data: Vec::new(),
            active_window_id: None,
            interactive_resize: None,
            working_area,
            scale,
            clock,
            options,
        }
    }

    pub fn update_config(
        &mut self,
        working_area: Rectangle<f64, Logical>,
        scale: f64,
        options: Rc<Options>,
    ) {
        for (tile, data) in zip(&mut self.tiles, &mut self.data) {
            tile.update_config(scale, options.clone());
            data.update(tile);
            data.update_config(working_area);
        }

        self.working_area = working_area;
        self.scale = scale;
        self.options = options;
    }

    pub fn update_shaders(&mut self) {
        for tile in &mut self.tiles {
            tile.update_shaders();
        }
    }

    pub fn advance_animations(&mut self) {
        for tile in &mut self.tiles {
            tile.advance_animations();
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.tiles.iter().any(Tile::are_animations_ongoing)
    }

    pub fn update_render_elements(&mut self, is_active: bool, view_rect: Rectangle<f64, Logical>) {
        let active = self.active_window_id.clone();
        for (tile, offset) in self.tiles_with_offsets_mut() {
            let id = tile.window().id();
            let is_active = is_active && Some(id) == active.as_ref();

            let mut tile_view_rect = view_rect;
            tile_view_rect.loc -= offset + tile.render_offset();
            tile.update(is_active, tile_view_rect);
        }
    }

    pub fn tiles(&self) -> impl Iterator<Item = &Tile<W>> + '_ {
        self.tiles.iter()
    }

    pub fn tiles_mut(&mut self) -> impl Iterator<Item = &mut Tile<W>> + '_ {
        self.tiles.iter_mut()
    }

    pub fn tiles_with_offsets(&self) -> impl Iterator<Item = (&Tile<W>, Point<f64, Logical>)> + '_ {
        let offsets = self.data.iter().map(|d| d.logical_pos);
        zip(&self.tiles, offsets)
    }

    pub fn tiles_with_offsets_mut(
        &mut self,
    ) -> impl Iterator<Item = (&mut Tile<W>, Point<f64, Logical>)> + '_ {
        let offsets = self.data.iter().map(|d| d.logical_pos);
        zip(&mut self.tiles, offsets)
    }

    pub fn tiles_with_render_positions(
        &self,
    ) -> impl Iterator<Item = (&Tile<W>, Point<f64, Logical>)> {
        let scale = self.scale;
        self.tiles_with_offsets().map(move |(tile, offset)| {
            let pos = offset + tile.render_offset();
            // Round to physical pixels.
            let pos = pos.to_physical_precise_round(scale).to_logical(scale);
            (tile, pos)
        })
    }

    pub fn tiles_with_render_positions_mut(
        &mut self,
        round: bool,
    ) -> impl Iterator<Item = (&mut Tile<W>, Point<f64, Logical>)> {
        let scale = self.scale;
        self.tiles_with_offsets_mut().map(move |(tile, offset)| {
            let mut pos = offset + tile.render_offset();
            // Round to physical pixels.
            if round {
                pos = pos.to_physical_precise_round(scale).to_logical(scale);
            }
            (tile, pos)
        })
    }

    /// Returns the geometry of the active tile relative to and clamped to the working area.
    ///
    /// During animations, assumes the final tile position.
    pub fn active_tile_visual_rectangle(&self) -> Option<Rectangle<f64, Logical>> {
        let (tile, offset) = self.tiles_with_offsets().next()?;

        let tile_size = tile.tile_size();
        let tile_rect = Rectangle::from_loc_and_size(offset, tile_size);

        self.working_area.intersection(tile_rect)
    }

    pub fn popup_target_rect(&self, id: &W::Id) -> Option<Rectangle<f64, Logical>> {
        for (tile, pos) in self.tiles_with_offsets() {
            if tile.window().id() == id {
                // TODO: intersect with working area width.
                let width = tile.window_size().w;
                let height = self.working_area.size.h;

                let mut target = Rectangle::from_loc_and_size((0., 0.), (width, height));
                target.loc.y -= pos.y;
                target.loc.y -= tile.window_loc().y;

                return Some(target);
            }
        }
        None
    }

    fn idx_of(&self, id: &W::Id) -> Option<usize> {
        self.tiles.iter().position(|tile| tile.window().id() == id)
    }

    fn contains(&self, id: &W::Id) -> bool {
        self.idx_of(id).is_some()
    }

    pub fn active_window(&self) -> Option<&W> {
        let id = self.active_window_id.as_ref()?;
        self.tiles
            .iter()
            .find(|tile| tile.window().id() == id)
            .map(Tile::window)
    }

    pub fn has_window(&self, id: &W::Id) -> bool {
        self.tiles.iter().any(|tile| tile.window().id() == id)
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn add_tile(&mut self, tile: Tile<W>, pos: Point<f64, Logical>, activate: bool) {
        self.add_tile_at(0, tile, pos, activate);
    }

    fn add_tile_at(
        &mut self,
        idx: usize,
        mut tile: Tile<W>,
        pos: Point<f64, Logical>,
        activate: bool,
    ) {
        tile.update_config(self.scale, self.options.clone());

        if tile.window().is_pending_fullscreen() {
            tile.window_mut()
                .request_size(Size::from((0, 0)), true, None);
        }

        if activate || self.tiles.is_empty() {
            self.active_window_id = Some(tile.window().id().clone());
        }

        let data = Data::new(self.working_area, &tile, pos);
        self.data.insert(idx, data);
        self.tiles.insert(idx, tile);
    }

    pub fn add_tile_above(&mut self, above: &W::Id, tile: Tile<W>) {
        // Activate the new window if above was active.
        let activate = Some(above) == self.active_window_id.as_ref();

        let idx = self.idx_of(above).unwrap();
        // TODO: pos
        self.add_tile_at(idx, tile, Point::default(), activate);
    }

    pub fn remove_active_tile(&mut self) -> Option<RemovedTile<W>> {
        let id = self.active_window_id.clone()?;
        Some(self.remove_tile(&id))
    }

    pub fn remove_tile(&mut self, id: &W::Id) -> RemovedTile<W> {
        let idx = self.idx_of(id).unwrap();
        self.remove_tile_by_idx(idx)
    }

    fn remove_tile_by_idx(&mut self, idx: usize) -> RemovedTile<W> {
        let tile = self.tiles.remove(idx);
        self.data.remove(idx);

        if self.tiles.is_empty() {
            self.active_window_id = None;
        } else if Some(tile.window().id()) == self.active_window_id.as_ref() {
            // The active tile was removed, make the topmost tile active.
            self.active_window_id = Some(self.tiles[0].window().id().clone());
        }

        // Stop interactive resize.
        if let Some(resize) = &self.interactive_resize {
            if tile.window().id() == &resize.window {
                self.interactive_resize = None;
            }
        }

        let width = ColumnWidth::Fixed(tile.window_size().w);
        RemovedTile {
            tile,
            // TODO
            width,
            is_full_width: false,
        }
    }

    pub fn activate_window_without_raising(&mut self, id: &W::Id) -> bool {
        if !self.contains(id) {
            return false;
        }

        self.active_window_id = Some(id.clone());
        true
    }

    pub fn activate_window(&mut self, id: &W::Id) -> bool {
        let Some(idx) = self.idx_of(id) else {
            return false;
        };

        let tile = self.tiles.remove(idx);
        let data = self.data.remove(idx);
        self.tiles.insert(0, tile);
        self.data.insert(0, data);
        self.active_window_id = Some(id.clone());

        true
    }

    pub fn set_window_width(&mut self, id: Option<&W::Id>, change: SizeChange, animate: bool) {
        let Some(id) = id.or(self.active_window_id.as_ref()) else {
            return;
        };
        let idx = self.idx_of(id).unwrap();

        let SizeChange::SetFixed(mut win_width) = change else {
            // TODO
            return;
        };

        let tile = &mut self.tiles[idx];
        let win = tile.window_mut();
        let min_w = win.min_size().w;
        let max_w = win.max_size().w;

        if max_w > 0 {
            win_width = min(win_width, max_w);
        }
        if min_w > 0 {
            win_width = max(win_width, min_w);
        }
        win_width = max(1, win_width);

        let win_height = win
            .requested_size()
            .map(|size| size.h)
            // If we requested height = 0, then switch to the current height.
            .filter(|h| *h != 0)
            .unwrap_or_else(|| win.size().h);
        let win_size = Size::from((win_width, win_height));
        win.request_size(win_size, animate, None);
    }

    pub fn set_window_height(&mut self, id: Option<&W::Id>, change: SizeChange, animate: bool) {
        let Some(id) = id.or(self.active_window_id.as_ref()) else {
            return;
        };
        let idx = self.idx_of(id).unwrap();

        let SizeChange::SetFixed(mut win_height) = change else {
            // TODO
            return;
        };

        let tile = &mut self.tiles[idx];
        let win = tile.window_mut();
        let min_h = win.min_size().h;
        let max_h = win.max_size().h;

        if max_h > 0 {
            win_height = min(win_height, max_h);
        }
        if min_h > 0 {
            win_height = max(win_height, min_h);
        }
        win_height = max(1, win_height);

        let win_width = win
            .requested_size()
            .map(|size| size.w)
            // If we requested width = 0, then switch to the current width.
            .filter(|w| *w != 0)
            .unwrap_or_else(|| win.size().w);
        let win_size = Size::from((win_width, win_height));
        win.request_size(win_size, animate, None);
    }

    pub fn update_window(&mut self, id: &W::Id, serial: Option<Serial>) -> bool {
        let Some(tile_idx) = self.idx_of(id) else {
            return false;
        };

        let tile = &mut self.tiles[tile_idx];
        let data = &mut self.data[tile_idx];

        let resize = tile.window_mut().interactive_resize_data();

        // Do this before calling update_window() so it can get up-to-date info.
        if let Some(serial) = serial {
            tile.window_mut().update_interactive_resize(serial);
        }

        let prev_size = data.size;

        tile.update_window();
        data.update(tile);

        // When resizing by top/left edge, update the position accordingly.
        if let Some(resize) = resize {
            let mut offset = Point::from((0., 0.));
            if resize.edges.contains(ResizeEdge::LEFT) {
                offset.x += prev_size.w - data.size.w;
            }
            if resize.edges.contains(ResizeEdge::TOP) {
                offset.y += prev_size.h - data.size.h;
            }
            data.set_logical_pos(data.logical_pos + offset);
        }

        true
    }

    pub fn render_elements<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        scale: Scale<f64>,
        target: RenderTarget,
    ) -> Vec<FloatingSpaceRenderElement<R>> {
        let mut rv = Vec::new();

        let active = self.active_window_id.clone();
        for (tile, tile_pos) in self.tiles_with_render_positions() {
            // For the active tile, draw the focus ring.
            let focus_ring = Some(tile.window().id()) == active.as_ref();

            rv.extend(
                tile.render(renderer, tile_pos, scale, focus_ring, target)
                    .map(Into::into),
            );
        }

        rv
    }

    pub fn interactive_resize_begin(&mut self, window: W::Id, edges: ResizeEdge) -> bool {
        if self.interactive_resize.is_some() {
            return false;
        }

        let tile = self
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

        let original_window_size = resize.original_window_size;
        let edges = resize.data.edges;

        if edges.intersects(ResizeEdge::LEFT_RIGHT) {
            let mut dx = delta.x;
            if edges.contains(ResizeEdge::LEFT) {
                dx = -dx;
            };

            let window_width = (original_window_size.w + dx).round() as i32;
            self.set_window_width(Some(window), SizeChange::SetFixed(window_width), false);
        }

        if edges.intersects(ResizeEdge::TOP_BOTTOM) {
            let mut dy = delta.y;
            if edges.contains(ResizeEdge::TOP) {
                dy = -dy;
            };

            let window_height = (original_window_size.h + dy).round() as i32;
            self.set_window_height(Some(window), SizeChange::SetFixed(window_height), false);
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
        }

        self.interactive_resize = None;
    }

    pub fn refresh(&mut self, is_active: bool) {
        let active = self.active_window_id.clone();
        for tile in &mut self.tiles {
            let win = tile.window_mut();

            win.set_active_in_column(true);

            let is_active = is_active && Some(win.id()) == active.as_ref();
            win.set_activated(is_active);

            let resize_data = self
                .interactive_resize
                .as_ref()
                .filter(|resize| &resize.window == win.id())
                .map(|resize| resize.data);
            win.set_interactive_resize(resize_data);

            let border_config = win.rules().border.resolve_against(self.options.border);
            // TODO
            // let bounds = compute_toplevel_bounds(
            //     border_config,
            //     self.working_area.size,
            //     self.options.gaps,
            // );
            // win.set_bounds(bounds);

            // If transactions are disabled, also disable combined throttling, for more
            // intuitive behavior.
            let intent = if self.options.disable_resize_throttling {
                ConfigureIntent::CanSend
            } else {
                win.configure_intent()
            };

            if matches!(
                intent,
                ConfigureIntent::CanSend | ConfigureIntent::ShouldSend
            ) {
                win.send_pending_configure();
            }

            win.refresh();
        }
    }

    #[cfg(test)]
    pub fn working_area(&self) -> Rectangle<f64, Logical> {
        self.working_area
    }

    #[cfg(test)]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    #[cfg(test)]
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    #[cfg(test)]
    pub fn options(&self) -> &Rc<Options> {
        &self.options
    }

    #[cfg(test)]
    pub fn verify_invariants(&self) {
        assert!(self.scale > 0.);
        assert!(self.scale.is_finite());
        assert_eq!(self.tiles.len(), self.data.len());

        for (tile, data) in zip(&self.tiles, &self.data) {
            assert!(Rc::ptr_eq(&self.options, &tile.options));
            assert_eq!(self.clock, tile.clock);
            assert_eq!(self.scale, tile.scale());
            tile.verify_invariants();

            assert!(
                !tile.window().is_pending_fullscreen(),
                "floating windows cannot be fullscreen"
            );

            data.verify_invariants();

            let mut data2 = *data;
            data2.update(tile);
            data2.update_config(self.working_area);
            assert_eq!(data, &data2, "tile data must be up to date");
        }

        if let Some(id) = &self.active_window_id {
            assert!(!self.tiles.is_empty());
            assert!(self.contains(id), "active window must be present in tiles");
        } else {
            assert!(self.tiles.is_empty());
        }

        if let Some(resize) = &self.interactive_resize {
            assert!(
                self.contains(&resize.window),
                "interactive resize window must be present in tiles"
            );
        }
    }
}
