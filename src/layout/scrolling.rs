use std::cmp::{max, min};
use std::iter::{self, zip};
use std::rc::Rc;
use std::time::Duration;

use niri_config::{CenterFocusedColumn, CornerRadius, PresetSize, Struts};
use niri_ipc::SizeChange;
use ordered_float::NotNan;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Scale, Serial, Size};

use super::closing_window::{ClosingWindow, ClosingWindowRenderElement};
use super::insert_hint_element::{InsertHintElement, InsertHintRenderElement};
use super::tile::{Tile, TileRenderElement, TileRenderSnapshot};
use super::workspace::{InteractiveResize, ResolvedSize};
use super::{ConfigureIntent, InteractiveResizeData, LayoutElement, Options, RemovedTile};
use crate::animation::{Animation, Clock};
use crate::input::swipe_tracker::SwipeTracker;
use crate::niri_render_elements;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::RenderTarget;
use crate::utils::transaction::{Transaction, TransactionBlocker};
use crate::utils::ResizeEdge;
use crate::window::ResolvedWindowRules;

/// Amount of touchpad movement to scroll the view for the width of one working area.
const VIEW_GESTURE_WORKING_AREA_MOVEMENT: f64 = 1200.;

/// A scrollable-tiling space for windows.
#[derive(Debug)]
pub struct ScrollingSpace<W: LayoutElement> {
    /// Columns of windows on this space.
    columns: Vec<Column<W>>,

    /// Extra per-column data.
    data: Vec<ColumnData>,

    /// Index of the currently active column, if any.
    active_column_idx: usize,

    /// Ongoing interactive resize.
    interactive_resize: Option<InteractiveResize<W>>,

    /// Offset of the view computed from the active column.
    ///
    /// Any gaps, including left padding from work area left exclusive zone, is handled
    /// with this view offset (rather than added as a constant elsewhere in the code). This allows
    /// for natural handling of fullscreen windows, which must ignore work area padding.
    view_offset: ViewOffset,

    /// Whether to activate the previous, rather than the next, column upon column removal.
    ///
    /// When a new column is created and removed with no focus changes in-between, it is more
    /// natural to activate the previously-focused column. This variable tracks that.
    ///
    /// Since we only create-and-activate columns immediately to the right of the active column (in
    /// contrast to tabs in Firefox, for example), we can track this as a bool, rather than an
    /// index of the previous column to activate.
    ///
    /// The value is the view offset that the previous column had before, to restore it.
    activate_prev_column_on_removal: Option<f64>,

    /// View offset to restore after unfullscreening.
    view_offset_before_fullscreen: Option<f64>,

    /// Windows in the closing animation.
    closing_windows: Vec<ClosingWindow>,

    /// Indication where an interactively-moved window is about to be placed.
    insert_hint: Option<InsertHint>,

    /// Insert hint element for rendering.
    insert_hint_element: InsertHintElement,

    /// View size for this space.
    view_size: Size<f64, Logical>,

    /// Working area for this space.
    ///
    /// Takes into account layer-shell exclusive zones and niri struts.
    working_area: Rectangle<f64, Logical>,

    /// Scale of the output the space is on (and rounds its sizes to).
    scale: f64,

    /// Clock for driving animations.
    clock: Clock,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

niri_render_elements! {
    ScrollingSpaceRenderElement<R> => {
        Tile = TileRenderElement<R>,
        ClosingWindow = ClosingWindowRenderElement,
        InsertHint = InsertHintRenderElement,
    }
}

#[derive(Debug, PartialEq)]
pub enum InsertPosition {
    NewColumn(usize),
    InColumn(usize, usize),
    Floating,
}

#[derive(Debug)]
pub struct InsertHint {
    pub position: InsertPosition,
    pub width: ColumnWidth,
    pub is_full_width: bool,
    pub corner_radius: CornerRadius,
}

/// Extra per-column data.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ColumnData {
    /// Cached actual column width.
    width: f64,
}

#[derive(Debug)]
enum ViewOffset {
    /// The view offset is static.
    Static(f64),
    /// The view offset is animating.
    Animation(Animation),
    /// The view offset is controlled by the ongoing gesture.
    Gesture(ViewGesture),
}

#[derive(Debug)]
struct ViewGesture {
    current_view_offset: f64,
    tracker: SwipeTracker,
    delta_from_tracker: f64,
    // The view offset we'll use if needed for activate_prev_column_on_removal.
    stationary_view_offset: f64,
    /// Whether the gesture is controlled by the touchpad.
    is_touchpad: bool,
}

#[derive(Debug)]
pub struct Column<W: LayoutElement> {
    /// Tiles in this column.
    ///
    /// Must be non-empty.
    tiles: Vec<Tile<W>>,

    /// Extra per-tile data.
    ///
    /// Must have the same number of elements as `tiles`.
    data: Vec<TileData>,

    /// Index of the currently active tile.
    active_tile_idx: usize,

    /// Desired width of this column.
    ///
    /// If the column is full-width or full-screened, this is the width that should be restored
    /// upon unfullscreening and untoggling full-width.
    width: ColumnWidth,

    /// Whether this column is full-width.
    is_full_width: bool,

    /// Whether this column contains a single full-screened window.
    is_fullscreen: bool,

    /// Animation of the render offset during window swapping.
    move_animation: Option<Animation>,

    /// Latest known view size for this column's workspace.
    view_size: Size<f64, Logical>,

    /// Latest known working area for this column's workspace.
    working_area: Rectangle<f64, Logical>,

    /// Scale of the output the column is on (and rounds its sizes to).
    scale: f64,

    /// Clock for driving animations.
    clock: Clock,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

/// Extra per-tile data.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TileData {
    /// Requested height of the window.
    ///
    /// This is window height, not tile height, so it excludes tile decorations.
    height: WindowHeight,

    /// Cached actual size of the tile.
    size: Size<f64, Logical>,

    /// Cached whether the tile is being interactively resized by its left edge.
    interactively_resizing_by_left_edge: bool,
}

/// Width of a column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    /// Proportion of the current view width.
    Proportion(f64),
    /// Fixed width in logical pixels.
    Fixed(f64),
    /// One of the preset widths.
    Preset(usize),
}

/// Height of a window in a column.
///
/// Every window but one in a column must be `Auto`-sized so that the total height can add up to
/// the workspace height. Resizing a window converts all other windows to `Auto`, weighted to
/// preserve their visual heights at the moment of the conversion.
///
/// In contrast to column widths, proportional height changes are converted to, and stored as,
/// fixed height right away. With column widths you frequently want e.g. two columns side-by-side
/// with 50% width each, and you want them to remain this way when moving to a differently sized
/// monitor. Windows in a column, however, already auto-size to fill the available height, giving
/// you this behavior. The main reason to set a different window height, then, is when you want
/// something in the window to fit exactly, e.g. to fit 30 lines in a terminal, which corresponds
/// to the `Fixed` variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowHeight {
    /// Automatically computed *tile* height, distributed across the column according to weights.
    ///
    /// This controls the tile height rather than the window height because it's easier in the auto
    /// height distribution algorithm.
    Auto { weight: f64 },
    /// Fixed *window* height in logical pixels.
    Fixed(f64),
    /// One of the preset heights (tile or window).
    Preset(usize),
}

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn new(
        view_size: Size<f64, Logical>,
        working_area: Rectangle<f64, Logical>,
        scale: f64,
        clock: Clock,
        options: Rc<Options>,
    ) -> Self {
        let working_area = compute_working_area(working_area, scale, options.struts);

        Self {
            columns: Vec::new(),
            data: Vec::new(),
            active_column_idx: 0,
            interactive_resize: None,
            view_offset: ViewOffset::Static(0.),
            activate_prev_column_on_removal: None,
            view_offset_before_fullscreen: None,
            closing_windows: Vec::new(),
            insert_hint: None,
            insert_hint_element: InsertHintElement::new(options.insert_hint),
            view_size,
            working_area,
            scale,
            clock,
            options,
        }
    }

    pub fn update_config(
        &mut self,
        view_size: Size<f64, Logical>,
        working_area: Rectangle<f64, Logical>,
        scale: f64,
        options: Rc<Options>,
    ) {
        let working_area = compute_working_area(working_area, scale, options.struts);

        for (column, data) in zip(&mut self.columns, &mut self.data) {
            column.update_config(view_size, working_area, scale, options.clone());
            data.update(column);
        }

        self.insert_hint_element.update_config(options.insert_hint);

        self.view_size = view_size;
        self.working_area = working_area;
        self.scale = scale;
        self.options = options;
    }

    pub fn update_shaders(&mut self) {
        for tile in self.tiles_mut() {
            tile.update_shaders();
        }

        self.insert_hint_element.update_shaders();
    }

    pub fn advance_animations(&mut self) {
        if let ViewOffset::Animation(anim) = &self.view_offset {
            if anim.is_done() {
                self.view_offset = ViewOffset::Static(anim.to());
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
        self.view_offset.is_animation()
            || self.columns.iter().any(Column::are_animations_ongoing)
            || !self.closing_windows.is_empty()
    }

    pub fn are_transitions_ongoing(&self) -> bool {
        !self.view_offset.is_static()
            || self.columns.iter().any(Column::are_animations_ongoing)
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
            let view_rect = Rectangle::from_loc_and_size(col_pos, view_size);
            col.update_render_elements(is_active, view_rect);
        }

        if let Some(insert_hint) = &self.insert_hint {
            if let Some(area) = self.insert_hint_area(insert_hint) {
                let view_rect = Rectangle::from_loc_and_size(area.loc.upscale(-1.), view_size);
                self.insert_hint_element.update_render_elements(
                    area.size,
                    view_rect,
                    insert_hint.corner_radius,
                    self.scale,
                );
            }
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

    pub fn active_tile_mut(&mut self) -> Option<&mut Tile<W>> {
        if self.columns.is_empty() {
            return None;
        }

        let col = &mut self.columns[self.active_column_idx];
        Some(&mut col.tiles[col.active_tile_idx])
    }

    pub fn is_active_fullscreen(&self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        let col = &self.columns[self.active_column_idx];
        col.is_fullscreen
    }

    pub fn toplevel_bounds(&self, rules: &ResolvedWindowRules) -> Size<i32, Logical> {
        let border_config = rules.border.resolve_against(self.options.border);
        compute_toplevel_bounds(border_config, self.working_area.size, self.options.gaps)
    }

    pub fn new_window_size(
        &self,
        width: Option<ColumnWidth>,
        height: Option<PresetSize>,
        rules: &ResolvedWindowRules,
    ) -> Size<i32, Logical> {
        let border = rules.border.resolve_against(self.options.border);

        let width = if let Some(width) = width {
            let is_fixed = matches!(width, ColumnWidth::Fixed(_));

            let mut width = width.resolve(&self.options, self.working_area.size.w);

            if !is_fixed && !border.off {
                width -= border.width.0 * 2.;
            }

            max(1, width.floor() as i32)
        } else {
            0
        };

        let mut full_height = self.working_area.size.h - self.options.gaps * 2.;
        if !border.off {
            full_height -= border.width.0 * 2.;
        }

        let height = if let Some(height) = height {
            let height = match resolve_preset_size(height, &self.options, self.working_area.size.h)
            {
                ResolvedSize::Tile(mut size) => {
                    if !border.off {
                        size -= border.width.0 * 2.;
                    }
                    size
                }
                ResolvedSize::Window(size) => size,
            };
            f64::min(height, full_height)
        } else {
            full_height
        };

        Size::from((width, max(height.floor() as i32, 1)))
    }

    pub fn is_centering_focused_column(&self) -> bool {
        self.options.center_focused_column == CenterFocusedColumn::Always
            || (self.options.always_center_single_column && self.columns.len() <= 1)
    }

    fn compute_new_view_offset_fit(
        &self,
        target_x: Option<f64>,
        col_x: f64,
        width: f64,
        is_fullscreen: bool,
    ) -> f64 {
        if is_fullscreen {
            return 0.;
        }

        let target_x = target_x.unwrap_or_else(|| self.target_view_pos());

        let new_offset = compute_new_view_offset(
            target_x + self.working_area.loc.x,
            self.working_area.size.w,
            col_x,
            width,
            self.options.gaps,
        );

        // Non-fullscreen windows are always offset at least by the working area position.
        new_offset - self.working_area.loc.x
    }

    fn compute_new_view_offset_centered(
        &self,
        target_x: Option<f64>,
        col_x: f64,
        width: f64,
        is_fullscreen: bool,
    ) -> f64 {
        if is_fullscreen {
            return self.compute_new_view_offset_fit(target_x, col_x, width, is_fullscreen);
        }

        // Columns wider than the view are left-aligned (the fit code can deal with that).
        if self.working_area.size.w <= width {
            return self.compute_new_view_offset_fit(target_x, col_x, width, is_fullscreen);
        }

        -(self.working_area.size.w - width) / 2. - self.working_area.loc.x
    }

    fn compute_new_view_offset_for_column_fit(&self, target_x: Option<f64>, idx: usize) -> f64 {
        let col = &self.columns[idx];
        self.compute_new_view_offset_fit(
            target_x,
            self.column_x(idx),
            col.width(),
            col.is_fullscreen,
        )
    }

    fn compute_new_view_offset_for_column_centered(
        &self,
        target_x: Option<f64>,
        idx: usize,
    ) -> f64 {
        let col = &self.columns[idx];
        self.compute_new_view_offset_centered(
            target_x,
            self.column_x(idx),
            col.width(),
            col.is_fullscreen,
        )
    }

    fn compute_new_view_offset_for_column(
        &self,
        target_x: Option<f64>,
        idx: usize,
        prev_idx: Option<usize>,
    ) -> f64 {
        if self.is_centering_focused_column() {
            return self.compute_new_view_offset_for_column_centered(target_x, idx);
        }

        match self.options.center_focused_column {
            CenterFocusedColumn::Always => {
                self.compute_new_view_offset_for_column_centered(target_x, idx)
            }
            CenterFocusedColumn::OnOverflow => {
                let Some(prev_idx) = prev_idx else {
                    return self.compute_new_view_offset_for_column_fit(target_x, idx);
                };

                // Always take the left or right neighbor of the target as the source.
                let source_idx = if prev_idx > idx {
                    min(idx + 1, self.columns.len() - 1)
                } else {
                    idx.saturating_sub(1)
                };

                let source_col_x = self.column_x(source_idx);
                let source_col_width = self.columns[source_idx].width();

                let target_col_x = self.column_x(idx);
                let target_col_width = self.columns[idx].width();

                let total_width = if source_col_x < target_col_x {
                    // Source is left from target.
                    target_col_x - source_col_x + target_col_width
                } else {
                    // Source is right from target.
                    source_col_x - target_col_x + source_col_width
                } + self.options.gaps * 2.;

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

    fn animate_view_offset(&mut self, idx: usize, new_view_offset: f64) {
        self.animate_view_offset_with_config(
            idx,
            new_view_offset,
            self.options.animations.horizontal_view_movement.0,
        );
    }

    fn animate_view_offset_with_config(
        &mut self,
        idx: usize,
        new_view_offset: f64,
        config: niri_config::Animation,
    ) {
        self.view_offset.cancel_gesture();

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

        // FIXME: also compute and use current velocity.
        self.view_offset = ViewOffset::Animation(Animation::new(
            self.clock.clone(),
            self.view_offset.current(),
            new_view_offset,
            0.,
            config,
        ));
    }

    fn animate_view_offset_to_column_centered(
        &mut self,
        target_x: Option<f64>,
        idx: usize,
        config: niri_config::Animation,
    ) {
        let new_view_offset = self.compute_new_view_offset_for_column_centered(target_x, idx);
        self.animate_view_offset_with_config(idx, new_view_offset, config);
    }

    fn animate_view_offset_to_column_with_config(
        &mut self,
        target_x: Option<f64>,
        idx: usize,
        prev_idx: Option<usize>,
        config: niri_config::Animation,
    ) {
        let new_view_offset = self.compute_new_view_offset_for_column(target_x, idx, prev_idx);
        self.animate_view_offset_with_config(idx, new_view_offset, config);
    }

    fn animate_view_offset_to_column(
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

    fn activate_column(&mut self, idx: usize) {
        self.activate_column_with_anim_config(
            idx,
            self.options.animations.horizontal_view_movement.0,
        );
    }

    fn activate_column_with_anim_config(&mut self, idx: usize, config: niri_config::Animation) {
        if self.active_column_idx == idx {
            return;
        }

        self.animate_view_offset_to_column_with_config(
            None,
            idx,
            Some(self.active_column_idx),
            config,
        );

        self.active_column_idx = idx;

        // A different column was activated; reset the flag.
        self.activate_prev_column_on_removal = None;
        self.view_offset_before_fullscreen = None;
        self.interactive_resize = None;
    }

    pub fn set_insert_hint(&mut self, insert_hint: InsertHint) {
        if self.options.insert_hint.off {
            return;
        }
        self.insert_hint = Some(insert_hint);
    }

    pub fn clear_insert_hint(&mut self) {
        self.insert_hint = None;
    }

    pub fn get_insert_position(&self, pos: Point<f64, Logical>) -> InsertPosition {
        if self.columns.is_empty() {
            return InsertPosition::NewColumn(0);
        }

        let x = pos.x + self.view_pos();

        // Aim for the center of the gap.
        let x = x + self.options.gaps / 2.;
        let y = pos.y + self.options.gaps / 2.;

        // Insert position is before the first column.
        if x < 0. {
            return InsertPosition::NewColumn(0);
        }

        // Find the closest gap between columns.
        let (closest_col_idx, col_x) = self
            .column_xs(self.data.iter().copied())
            .enumerate()
            .min_by_key(|(_, col_x)| NotNan::new((col_x - x).abs()).unwrap())
            .unwrap();

        // Find the column containing the position.
        let (col_idx, _) = self
            .column_xs(self.data.iter().copied())
            .enumerate()
            .take_while(|(_, col_x)| *col_x <= x)
            .last()
            .unwrap_or((0, 0.));

        // Insert position is past the last column.
        if col_idx == self.columns.len() {
            return InsertPosition::NewColumn(closest_col_idx);
        }

        // Find the closest gap between tiles.
        let col = &self.columns[col_idx];
        let (closest_tile_idx, tile_off) = col
            .tile_offsets()
            .enumerate()
            .min_by_key(|(_, tile_off)| NotNan::new((tile_off.y - y).abs()).unwrap())
            .unwrap();

        // Return the closest among the vertical and the horizontal gap.
        let vert_dist = (col_x - x).abs();
        let hor_dist = (tile_off.y - y).abs();
        if vert_dist <= hor_dist {
            InsertPosition::NewColumn(closest_col_idx)
        } else {
            InsertPosition::InColumn(col_idx, closest_tile_idx)
        }
    }

    pub fn add_tile(
        &mut self,
        col_idx: Option<usize>,
        tile: Tile<W>,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
        anim_config: Option<niri_config::Animation>,
    ) {
        let column = Column::new_with_tile(
            tile,
            self.view_size,
            self.working_area,
            self.scale,
            width,
            is_full_width,
            true,
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
        let was_fullscreen = target_column.tiles[target_column.active_tile_idx].is_fullscreen();

        target_column.add_tile_at(tile_idx, tile, true);
        self.data[col_idx].update(target_column);

        // If the target column is the active column and its window was requested to, but hasn't
        // gone into fullscreen yet, then clear the stored view offset, since we just asked it to
        // stop going into fullscreen.
        if col_idx == self.active_column_idx && !was_fullscreen {
            self.view_offset_before_fullscreen = None;
        }

        if activate {
            target_column.active_tile_idx = tile_idx;
            if self.active_column_idx != col_idx {
                self.activate_column(col_idx);
            }
        } else if tile_idx <= target_column.active_tile_idx {
            target_column.active_tile_idx += 1;
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
        anim_config: Option<niri_config::Animation>,
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
        anim_config: Option<niri_config::Animation>,
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

        // Animate movement of other tiles.
        // FIXME: tiles can move by X too, in a centered or resizing layout with one window smaller
        // than the others.
        let offset_y = column.tile_offset(tile_idx + 1).y - column.tile_offset(tile_idx).y;
        for tile in &mut column.tiles[tile_idx + 1..] {
            tile.animate_move_y_from(offset_y);
        }

        let tile = column.tiles.remove(tile_idx);
        column.data.remove(tile_idx);

        // If one window is left, reset its weight to 1.
        if column.data.len() == 1 {
            if let WindowHeight::Auto { weight } = &mut column.data[0].height {
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

        column.active_tile_idx = min(column.active_tile_idx, column.tiles.len() - 1);
        column.update_tile_sizes_with_transaction(true, transaction);
        self.data[column_idx].update(column);
        let offset = prev_width - column.width();

        // Animate movement of the other columns.
        let movement_config = anim_config.unwrap_or(self.options.animations.window_movement.0);
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
        anim_config: Option<niri_config::Animation>,
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
            self.view_offset_before_fullscreen = None;
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
        let (tile_idx, tile) = column
            .tiles
            .iter_mut()
            .enumerate()
            .find(|(_, tile)| tile.window().id() == window)
            .unwrap();

        let was_fullscreen = tile.is_fullscreen();
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
        let started_resize_anim =
            column.tiles[tile_idx].resize_animation().is_some() && offset != 0.;
        if started_resize_anim {
            if self.active_column_idx <= col_idx {
                for col in &mut self.columns[col_idx + 1..] {
                    col.animate_move_from_with_config(
                        offset,
                        self.options.animations.window_resize.anim,
                    );
                }
            } else {
                for col in &mut self.columns[..=col_idx] {
                    col.animate_move_from_with_config(
                        -offset,
                        self.options.animations.window_resize.anim,
                    );
                }
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

            if self.interactive_resize.is_none() && !self.view_offset.is_gesture() {
                // We might need to move the view to ensure the resized window is still visible.

                // Upon unfullscreening, restore the view offset.
                let is_fullscreen = self.columns[col_idx].tiles[tile_idx].is_fullscreen();
                if was_fullscreen && !is_fullscreen {
                    if let Some(prev_offset) = self.view_offset_before_fullscreen.take() {
                        self.animate_view_offset(col_idx, prev_offset);
                    }
                }

                // Synchronize the horizontal view movement with the resize so that it looks nice.
                // This is especially important for always-centered view.
                let config = if started_resize_anim {
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

    pub fn start_close_animation_for_window(
        &mut self,
        renderer: &mut GlesRenderer,
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
                col.tiles
                    .iter()
                    .position(|tile| tile.window().id() == window)
                    .map(move |tile_idx| (col_idx, tile_idx))
            })
            .unwrap();

        let col = &self.columns[col_idx];
        let removing_last = col.tiles.len() == 1;

        tile_pos.x += self.view_pos();

        if col_idx < self.active_column_idx {
            let offset = if removing_last {
                self.column_x(col_idx + 1) - self.column_x(col_idx)
            } else {
                self.data[col_idx].width
                    - col
                        .data
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, data)| {
                            (idx != tile_idx).then_some(NotNan::new(data.size.w).unwrap())
                        })
                        .max()
                        .map(NotNan::into_inner)
                        .unwrap()
            };
            tile_pos.x -= offset;
        }

        self.start_close_animation_for_tile(renderer, snapshot, tile_size, tile_pos, blocker);
    }

    fn start_close_animation_for_tile(
        &mut self,
        renderer: &mut GlesRenderer,
        snapshot: TileRenderSnapshot,
        tile_size: Size<f64, Logical>,
        tile_pos: Point<f64, Logical>,
        blocker: TransactionBlocker,
    ) {
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

    fn move_column_to(&mut self, new_idx: usize) {
        if self.active_column_idx == new_idx {
            return;
        }

        let current_col_x = self.column_x(self.active_column_idx);
        let next_col_x = self.column_x(self.active_column_idx + 1);

        let mut column = self.columns.remove(self.active_column_idx);
        let data = self.data.remove(self.active_column_idx);
        cancel_resize_for_column(&mut self.interactive_resize, &mut column);
        self.columns.insert(new_idx, column);
        self.data.insert(new_idx, data);

        // Preserve the camera position when moving to the left.
        let view_offset_delta = -self.column_x(self.active_column_idx) + current_col_x;
        self.view_offset.cancel_gesture();
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
                self.activate_prev_column_on_removal =
                    Some(self.view_offset.stationary() + offset.x);
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

    pub fn view_pos(&self) -> f64 {
        self.column_x(self.active_column_idx) + self.view_offset.current()
    }

    pub fn target_view_pos(&self) -> f64 {
        self.column_x(self.active_column_idx) + self.view_offset.target()
    }

    // HACK: pass a self.data iterator in manually as a workaround for the lack of method partial
    // borrowing. Note that this method's return value does not borrow the entire &Self!
    fn column_xs(&self, data: impl Iterator<Item = ColumnData>) -> impl Iterator<Item = f64> {
        let gaps = self.options.gaps;
        let mut x = 0.;

        // Chain with a dummy value to be able to get one past all columns' X.
        let dummy = ColumnData { width: 0. };
        let data = data.chain(iter::once(dummy));

        data.map(move |data| {
            let rv = x;
            x += data.width + gaps;
            rv
        })
    }

    fn column_x(&self, column_idx: usize) -> f64 {
        self.column_xs(self.data.iter().copied())
            .nth(column_idx)
            .unwrap()
    }

    fn column_xs_in_render_order(
        &self,
        data: impl Iterator<Item = ColumnData>,
    ) -> impl Iterator<Item = f64> {
        let active_idx = self.active_column_idx;
        let active_pos = self.column_x(active_idx);
        let offsets = self
            .column_xs(data)
            .enumerate()
            .filter_map(move |(idx, pos)| (idx != active_idx).then_some(pos));
        iter::once(active_pos).chain(offsets)
    }

    fn columns_mut(&mut self) -> impl Iterator<Item = (&mut Column<W>, f64)> + '_ {
        let offsets = self.column_xs(self.data.iter().copied());
        zip(&mut self.columns, offsets)
    }

    fn columns_in_render_order(&self) -> impl Iterator<Item = (&Column<W>, f64)> + '_ {
        let offsets = self.column_xs_in_render_order(self.data.iter().copied());

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

    fn columns_in_render_order_mut(&mut self) -> impl Iterator<Item = (&mut Column<W>, f64)> + '_ {
        let offsets = self.column_xs_in_render_order(self.data.iter().copied());

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

    pub fn tiles_with_render_positions(
        &self,
    ) -> impl Iterator<Item = (&Tile<W>, Point<f64, Logical>)> {
        let scale = self.scale;
        let view_off = Point::from((-self.view_pos(), 0.));
        self.columns_in_render_order()
            .flat_map(move |(col, col_x)| {
                let col_off = Point::from((col_x, 0.));
                let col_render_off = col.render_offset();
                col.tiles_in_render_order().map(move |(tile, tile_off)| {
                    let pos = view_off + col_off + col_render_off + tile_off + tile.render_offset();
                    // Round to physical pixels.
                    let pos = pos.to_physical_precise_round(scale).to_logical(scale);
                    (tile, pos)
                })
            })
    }

    pub fn tiles_with_render_positions_mut(
        &mut self,
        round: bool,
    ) -> impl Iterator<Item = (&mut Tile<W>, Point<f64, Logical>)> {
        let scale = self.scale;
        let view_off = Point::from((-self.view_pos(), 0.));
        self.columns_in_render_order_mut()
            .flat_map(move |(col, col_x)| {
                let col_off = Point::from((col_x, 0.));
                let col_render_off = col.render_offset();
                col.tiles_in_render_order_mut()
                    .map(move |(tile, tile_off)| {
                        let mut pos =
                            view_off + col_off + col_render_off + tile_off + tile.render_offset();
                        // Round to physical pixels.
                        if round {
                            pos = pos.to_physical_precise_round(scale).to_logical(scale);
                        }
                        (tile, pos)
                    })
            })
    }

    fn insert_hint_area(&self, insert_hint: &InsertHint) -> Option<Rectangle<f64, Logical>> {
        let mut hint_area = match insert_hint.position {
            InsertPosition::NewColumn(column_index) => {
                if column_index == 0 || column_index == self.columns.len() {
                    let size =
                        Size::from((300., self.working_area.size.h - self.options.gaps * 2.));
                    let mut loc = Point::from((
                        self.column_x(column_index),
                        self.working_area.loc.y + self.options.gaps,
                    ));
                    if column_index == 0 && !self.columns.is_empty() {
                        loc.x -= size.w + self.options.gaps;
                    }
                    Rectangle::from_loc_and_size(loc, size)
                } else if column_index > self.columns.len() {
                    error!("insert hint column index is out of range");
                    return None;
                } else {
                    let size =
                        Size::from((300., self.working_area.size.h - self.options.gaps * 2.));
                    let loc = Point::from((
                        self.column_x(column_index) - size.w / 2. - self.options.gaps / 2.,
                        self.working_area.loc.y + self.options.gaps,
                    ));
                    Rectangle::from_loc_and_size(loc, size)
                }
            }
            InsertPosition::InColumn(column_index, tile_index) => {
                if column_index > self.columns.len() {
                    error!("insert hint column index is out of range");
                    return None;
                }
                if tile_index > self.columns[column_index].tiles.len() {
                    error!("insert hint tile index is out of range");
                    return None;
                }

                let (height, y) = if tile_index == 0 {
                    (150., self.columns[column_index].tile_offset(tile_index).y)
                } else if tile_index == self.columns[column_index].tiles.len() {
                    (
                        150.,
                        self.columns[column_index].tile_offset(tile_index).y
                            - self.options.gaps
                            - 150.,
                    )
                } else {
                    (
                        300.,
                        self.columns[column_index].tile_offset(tile_index).y
                            - self.options.gaps / 2.
                            - 150.,
                    )
                };

                let size = Size::from((self.data[column_index].width, height));
                let loc = Point::from((self.column_x(column_index), y));
                Rectangle::from_loc_and_size(loc, size)
            }
            InsertPosition::Floating => return None,
        };

        // First window on an empty workspace will cancel out any view offset. Replicate this
        // effect here.
        if self.columns.is_empty() {
            let view_offset = if self.is_centering_focused_column() {
                self.compute_new_view_offset_centered(Some(0.), 0., hint_area.size.w, false)
            } else {
                self.compute_new_view_offset_fit(Some(0.), 0., hint_area.size.w, false)
            };
            hint_area.loc.x -= view_offset;
        } else {
            hint_area.loc.x -= self.view_pos();
        }

        let view_size = self.view_size;

        // Make sure the hint is at least partially visible.
        if matches!(insert_hint.position, InsertPosition::NewColumn(_)) {
            hint_area.loc.x = hint_area.loc.x.max(-hint_area.size.w / 2.);
            hint_area.loc.x = hint_area.loc.x.min(view_size.w - hint_area.size.w / 2.);
        }

        // Round to physical pixels.
        hint_area = hint_area
            .to_physical_precise_round(self.scale)
            .to_logical(self.scale);

        Some(hint_area)
    }

    /// Returns the geometry of the active tile relative to and clamped to the view.
    ///
    /// During animations, assumes the final view position.
    pub fn active_tile_visual_rectangle(&self) -> Option<Rectangle<f64, Logical>> {
        let col = self.columns.get(self.active_column_idx)?;

        let final_view_offset = self.view_offset.target();
        let view_off = Point::from((-final_view_offset, 0.));

        let (tile, tile_off) = col.tiles().nth(col.active_tile_idx).unwrap();

        let tile_pos = view_off + tile_off;
        let tile_size = tile.tile_size();
        let tile_rect = Rectangle::from_loc_and_size(tile_pos, tile_size);

        let view = Rectangle::from_loc_and_size((0., 0.), self.view_size);
        view.intersection(tile_rect)
    }

    pub fn popup_target_rect(&self, id: &W::Id) -> Option<Rectangle<f64, Logical>> {
        self.columns
            .iter()
            .find_map(|col| col.popup_target_rect(id))
    }

    pub fn toggle_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        col.toggle_width();

        cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn toggle_full_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let col = &mut self.columns[self.active_column_idx];
        col.toggle_full_width();

        cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn set_window_width(&mut self, window: Option<&W::Id>, change: SizeChange) {
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

        col.set_column_width(change, tile_idx, true);

        cancel_resize_for_column(&mut self.interactive_resize, col);
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

        cancel_resize_for_column(&mut self.interactive_resize, col);
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

        col.reset_window_height(tile_idx, true);

        cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn toggle_window_width(&mut self, window: Option<&W::Id>) {
        if self.columns.is_empty() {
            return;
        }

        let col = if let Some(window) = window {
            self.columns
                .iter_mut()
                .find(|col| col.contains(window))
                .unwrap()
        } else {
            &mut self.columns[self.active_column_idx]
        };

        // FIXME: when we fix preset fixed width to be per-window, this should also be made
        // window-specific to resolve the correct window width.
        col.toggle_width();

        cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn toggle_window_height(&mut self, window: Option<&W::Id>) {
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

        col.toggle_window_height(tile_idx, true);

        cancel_resize_for_column(&mut self.interactive_resize, col);
    }

    pub fn set_fullscreen(&mut self, window: &W::Id, is_fullscreen: bool) -> bool {
        let (mut col_idx, tile_idx) = self
            .columns
            .iter()
            .enumerate()
            .find_map(|(col_idx, col)| col.position(window).map(|tile_idx| (col_idx, tile_idx)))
            .unwrap();

        if is_fullscreen == self.columns[col_idx].is_fullscreen {
            return false;
        }

        if is_fullscreen
            && col_idx == self.active_column_idx
            && self.columns[col_idx].tiles.len() == 1
        {
            self.view_offset_before_fullscreen = Some(self.view_offset.stationary());
        }

        let mut col = &mut self.columns[col_idx];

        cancel_resize_for_column(&mut self.interactive_resize, col);

        if is_fullscreen && col.tiles.len() > 1 {
            // This wasn't the only window in its column; extract it into a separate column.
            let activate = self.active_column_idx == col_idx && col.active_tile_idx == tile_idx;

            let removed = self.remove_tile_by_idx(col_idx, tile_idx, Transaction::new(), None);
            // Create a column manually to disable the resize animation.
            let column = Column::new_with_tile(
                removed.tile,
                self.view_size,
                self.working_area,
                self.scale,
                removed.width,
                removed.is_full_width,
                false,
            );
            self.add_column(Some(col_idx + 1), column, activate, None);

            col_idx += 1;
            col = &mut self.columns[col_idx];
        }

        col.set_fullscreen(is_fullscreen);

        // If we quickly fullscreen and unfullscreen before any window has a chance to receive the
        // request, we need to reset the offset.
        if col_idx == self.active_column_idx
            && !is_fullscreen
            && !col
                .tiles
                .iter()
                .any(|tile| tile.is_fullscreen() || tile.window().is_pending_fullscreen())
        {
            self.view_offset_before_fullscreen = None;
        }

        true
    }

    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer if we're on a fullscreen window and the view is stationary.
        if self.columns.is_empty() {
            return false;
        }

        if !self.view_offset.is_static() {
            return false;
        }

        self.columns[self.active_column_idx].is_fullscreen
    }

    pub fn render_elements<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        scale: Scale<f64>,
        target: RenderTarget,
        focus_ring: bool,
    ) -> Vec<ScrollingSpaceRenderElement<R>> {
        let mut rv = vec![];

        // Draw the insert hint.
        if let Some(insert_hint) = &self.insert_hint {
            if let Some(area) = self.insert_hint_area(insert_hint) {
                rv.extend(
                    self.insert_hint_element
                        .render(renderer, area.loc)
                        .map(ScrollingSpaceRenderElement::InsertHint),
                );
            }
        }

        // Draw the closing windows on top of the other windows.
        let view_rect = Rectangle::from_loc_and_size((self.view_pos(), 0.), self.view_size);
        for closing in self.closing_windows.iter().rev() {
            let elem = closing.render(renderer.as_gles_renderer(), view_rect, scale, target);
            rv.push(elem.into());
        }

        if self.columns.is_empty() {
            return rv;
        }

        let mut first = true;
        for (tile, tile_pos) in self.tiles_with_render_positions() {
            // For the active tile (which comes first), draw the focus ring.
            let focus_ring = focus_ring && first;
            first = false;

            rv.extend(
                tile.render(renderer, tile_pos, scale, focus_ring, target)
                    .map(Into::into),
            );
        }

        rv
    }

    pub fn view_offset_gesture_begin(&mut self, is_touchpad: bool) {
        if self.columns.is_empty() {
            return;
        }

        if self.interactive_resize.is_some() {
            return;
        }

        let gesture = ViewGesture {
            current_view_offset: self.view_offset.current(),
            tracker: SwipeTracker::new(),
            delta_from_tracker: self.view_offset.current(),
            stationary_view_offset: self.view_offset.stationary(),
            is_touchpad,
        };
        self.view_offset = ViewOffset::Gesture(gesture);
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

        if gesture.is_touchpad != is_touchpad {
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

    pub fn view_offset_gesture_end(&mut self, _cancelled: bool, is_touchpad: Option<bool>) -> bool {
        let ViewOffset::Gesture(gesture) = &mut self.view_offset else {
            return false;
        };

        if is_touchpad.map_or(false, |x| gesture.is_touchpad != x) {
            return false;
        }

        // We do not handle cancelling, just like GNOME Shell doesn't. For this gesture, proper
        // cancelling would require keeping track of the original active column, and then updating
        // it in all the right places (adding columns, removing columns, etc.) -- quite a bit of
        // effort and bug potential.

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

        let left_strut = self.working_area.loc.x;
        let right_strut = self.view_size.w - self.working_area.size.w - self.working_area.loc.x;

        if self.is_centering_focused_column() {
            let mut col_x = 0.;
            for (col_idx, col) in self.columns.iter().enumerate() {
                let col_w = col.width();

                let view_pos = if col.is_fullscreen {
                    col_x
                } else if self.working_area.size.w <= col_w {
                    col_x - left_strut
                } else {
                    col_x - (self.working_area.size.w - col_w) / 2. - left_strut
                };
                snapping_points.push(Snap { view_pos, col_idx });

                col_x += col_w + self.options.gaps;
            }
        } else {
            let view_width = self.view_size.w;
            let working_area_width = self.working_area.size.w;
            let gaps = self.options.gaps;

            let snap_points = |col_x, col: &Column<W>| {
                let col_w = col.width();

                // Normal columns align with the working area, but fullscreen columns align with the
                // view size.
                if col.is_fullscreen {
                    let left = col_x;
                    let right = col_x + col_w;
                    (left, right)
                } else {
                    // Logic from compute_new_view_offset.
                    let padding = ((working_area_width - col_w) / 2.).clamp(0., gaps);
                    let left = col_x - padding - left_strut;
                    let right = col_x + col_w + padding + right_strut;
                    (left, right)
                }
            };

            // Prevent the gesture from snapping further than the first/last column, as this is
            // generally undesired.
            //
            // It's ok if leftmost_snap is > rightmost_snap (this happens if the columns on a
            // workspace total up to less than the workspace width).
            let leftmost_snap = snap_points(0., &self.columns[0]).0;
            let last_col_idx = self.columns.len() - 1;
            let last_col_x = self
                .columns
                .iter()
                .take(last_col_idx)
                .fold(0., |col_x, col| col_x + col.width() + gaps);
            let rightmost_snap =
                snap_points(last_col_x, &self.columns[last_col_idx]).1 - view_width;

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
                let (left, right) = snap_points(col_x, col);
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

                    if col.is_fullscreen {
                        if target_snap.view_pos + self.view_size.w < col_x + col_w {
                            break;
                        }
                    } else {
                        let padding =
                            ((self.working_area.size.w - col_w) / 2.).clamp(0., self.options.gaps);
                        if target_snap.view_pos + left_strut + self.working_area.size.w
                            < col_x + col_w + padding
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

                    if col.is_fullscreen {
                        if col_x < target_snap.view_pos {
                            break;
                        }
                    } else {
                        let padding =
                            ((self.working_area.size.w - col_w) / 2.).clamp(0., self.options.gaps);
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
            self.view_offset_before_fullscreen = None;
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

    pub fn interactive_resize_begin(&mut self, window: W::Id, edges: ResizeEdge) -> bool {
        if self.interactive_resize.is_some() {
            return false;
        }

        let col = self
            .columns
            .iter_mut()
            .find(|col| col.contains(&window))
            .unwrap();

        if col.is_fullscreen {
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

    pub fn refresh(&mut self, is_active: bool) {
        for (col_idx, col) in self.columns.iter_mut().enumerate() {
            let mut col_resize_data = None;
            if let Some(resize) = &self.interactive_resize {
                if col.contains(&resize.window) {
                    col_resize_data = Some(resize.data);
                }
            }

            let intent = if self.options.disable_resize_throttling {
                ConfigureIntent::CanSend
            } else if self.options.disable_transactions {
                // When transactions are disabled, we don't use combined throttling, but rather
                // compute throttling individually below.
                ConfigureIntent::CanSend
            } else {
                col.tiles
                    .iter()
                    .fold(ConfigureIntent::NotNeeded, |intent, tile| {
                        match (intent, tile.window().configure_intent()) {
                            (_, ConfigureIntent::ShouldSend) => ConfigureIntent::ShouldSend,
                            (ConfigureIntent::NotNeeded, tile_intent) => tile_intent,
                            (ConfigureIntent::CanSend, ConfigureIntent::Throttled) => {
                                ConfigureIntent::Throttled
                            }
                            (intent, _) => intent,
                        }
                    })
            };

            for (tile_idx, tile) in col.tiles.iter_mut().enumerate() {
                let win = tile.window_mut();

                let active_in_column = col.active_tile_idx == tile_idx;
                win.set_active_in_column(active_in_column);
                win.set_floating(false);

                let active = is_active && self.active_column_idx == col_idx && active_in_column;
                win.set_activated(active);

                win.set_interactive_resize(col_resize_data);

                let border_config = win.rules().border.resolve_against(self.options.border);
                let bounds = compute_toplevel_bounds(
                    border_config,
                    self.working_area.size,
                    self.options.gaps,
                );
                win.set_bounds(bounds);

                // If transactions are disabled, also disable combined throttling, for more
                // intuitive behavior.
                let intent = if self.options.disable_transactions {
                    win.configure_intent()
                } else {
                    intent
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
    }

    #[cfg(test)]
    pub fn view_size(&self) -> Size<f64, Logical> {
        self.view_size
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
    pub fn active_column_idx(&self) -> usize {
        self.active_column_idx
    }

    #[cfg(test)]
    pub fn verify_invariants(&self, working_area: Rectangle<f64, Logical>) {
        assert!(self.view_size.w > 0.);
        assert!(self.view_size.h > 0.);
        assert!(self.scale > 0.);
        assert!(self.scale.is_finite());
        assert_eq!(self.columns.len(), self.data.len());
        assert_eq!(
            self.working_area,
            compute_working_area(working_area, self.scale, self.options.struts)
        );

        if !self.columns.is_empty() {
            assert!(self.active_column_idx < self.columns.len());

            for (column, data) in zip(&self.columns, &self.data) {
                assert!(Rc::ptr_eq(&self.options, &column.options));
                assert_eq!(self.clock, column.clock);
                assert_eq!(self.scale, column.scale);
                column.verify_invariants();

                let mut data2 = *data;
                data2.update(column);
                assert_eq!(data, &data2, "column data must be up to date");
            }

            let col = &self.columns[self.active_column_idx];

            // When we have an unfullscreen view offset stored, the active column should have a
            // fullscreen tile.
            if self.view_offset_before_fullscreen.is_some() {
                assert!(
                    col.is_fullscreen
                        || col.tiles.iter().any(|tile| {
                            tile.is_fullscreen() || tile.window().is_pending_fullscreen()
                        })
                );
            }
        }

        if let Some(resize) = &self.interactive_resize {
            assert!(
                self.columns
                    .iter()
                    .flat_map(|col| &col.tiles)
                    .any(|tile| tile.window().id() == &resize.window),
                "interactive resize window must be present in the layout"
            );
        }
    }
}

impl ViewOffset {
    /// Returns the current view offset.
    pub fn current(&self) -> f64 {
        match self {
            ViewOffset::Static(offset) => *offset,
            ViewOffset::Animation(anim) => anim.value(),
            ViewOffset::Gesture(gesture) => gesture.current_view_offset,
        }
    }

    /// Returns the target view offset suitable for computing the new view offset.
    pub fn target(&self) -> f64 {
        match self {
            ViewOffset::Static(offset) => *offset,
            ViewOffset::Animation(anim) => anim.to(),
            // This can be used for example if a gesture is interrupted.
            ViewOffset::Gesture(gesture) => gesture.current_view_offset,
        }
    }

    /// Returns a view offset value suitable for saving and later restoration.
    ///
    /// This means that it shouldn't return an in-progress animation or gesture value.
    fn stationary(&self) -> f64 {
        match self {
            ViewOffset::Static(offset) => *offset,
            // For animations we can return the final value.
            ViewOffset::Animation(anim) => anim.to(),
            ViewOffset::Gesture(gesture) => gesture.stationary_view_offset,
        }
    }

    pub fn is_static(&self) -> bool {
        matches!(self, Self::Static(_))
    }

    pub fn is_animation(&self) -> bool {
        matches!(self, Self::Animation(_))
    }

    pub fn is_gesture(&self) -> bool {
        matches!(self, Self::Gesture(_))
    }

    pub fn offset(&mut self, delta: f64) {
        match self {
            ViewOffset::Static(offset) => *offset += delta,
            ViewOffset::Animation(anim) => anim.offset(delta),
            ViewOffset::Gesture(_gesture) => {
                // Is this needed?
                error!("cancel gesture before offsetting");
            }
        }
    }

    pub fn cancel_gesture(&mut self) {
        if let ViewOffset::Gesture(gesture) = self {
            *self = ViewOffset::Static(gesture.current_view_offset);
        }
    }

    pub fn stop_anim_and_gesture(&mut self) {
        *self = ViewOffset::Static(self.current());
    }
}

impl ColumnData {
    pub fn new<W: LayoutElement>(column: &Column<W>) -> Self {
        let mut rv = Self { width: 0. };
        rv.update(column);
        rv
    }

    pub fn update<W: LayoutElement>(&mut self, column: &Column<W>) {
        self.width = column.width();
    }
}

impl TileData {
    pub fn new<W: LayoutElement>(tile: &Tile<W>, height: WindowHeight) -> Self {
        let mut rv = Self {
            height,
            size: Size::default(),
            interactively_resizing_by_left_edge: false,
        };
        rv.update(tile);
        rv
    }

    pub fn update<W: LayoutElement>(&mut self, tile: &Tile<W>) {
        self.size = tile.tile_size();
        self.interactively_resizing_by_left_edge = tile
            .window()
            .interactive_resize_data()
            .map_or(false, |data| data.edges.contains(ResizeEdge::LEFT));
    }
}

impl ColumnWidth {
    fn resolve(self, options: &Options, view_width: f64) -> f64 {
        match self {
            ColumnWidth::Proportion(proportion) => {
                (view_width - options.gaps) * proportion - options.gaps
            }
            ColumnWidth::Preset(idx) => {
                options.preset_column_widths[idx].resolve(options, view_width)
            }
            ColumnWidth::Fixed(width) => width,
        }
    }

    pub fn resolve_no_gaps(self, options: &Options, view_width: f64) -> ResolvedSize {
        match self {
            ColumnWidth::Proportion(proportion) => ResolvedSize::Tile(view_width * proportion),
            ColumnWidth::Preset(idx) => {
                options.preset_column_widths[idx].resolve_no_gaps(options, view_width)
            }
            ColumnWidth::Fixed(width) => ResolvedSize::Window(width),
        }
    }
}

impl From<PresetSize> for ColumnWidth {
    fn from(value: PresetSize) -> Self {
        match value {
            PresetSize::Proportion(p) => Self::Proportion(p.clamp(0., 10000.)),
            PresetSize::Fixed(f) => Self::Fixed(f64::from(f.clamp(1, 100000))),
        }
    }
}

impl WindowHeight {
    const fn auto_1() -> Self {
        Self::Auto { weight: 1. }
    }
}

impl<W: LayoutElement> Column<W> {
    #[allow(clippy::too_many_arguments)]
    fn new_with_tile(
        tile: Tile<W>,
        view_size: Size<f64, Logical>,
        working_area: Rectangle<f64, Logical>,
        scale: f64,
        width: ColumnWidth,
        is_full_width: bool,
        animate_resize: bool,
    ) -> Self {
        let mut rv = Self {
            tiles: vec![],
            data: vec![],
            active_tile_idx: 0,
            width,
            is_full_width,
            is_fullscreen: false,
            move_animation: None,
            view_size,
            working_area,
            scale,
            clock: tile.clock.clone(),
            options: tile.options.clone(),
        };

        let is_pending_fullscreen = tile.window().is_pending_fullscreen();

        rv.add_tile_at(0, tile, animate_resize);

        if is_pending_fullscreen {
            rv.set_fullscreen(true);
        }

        rv
    }

    fn update_config(
        &mut self,
        view_size: Size<f64, Logical>,
        working_area: Rectangle<f64, Logical>,
        scale: f64,
        options: Rc<Options>,
    ) {
        let mut update_sizes = false;

        if self.view_size != view_size || self.working_area != working_area {
            update_sizes = true;
        }

        // If preset widths changed, make our width non-preset.
        if self.options.preset_column_widths != options.preset_column_widths {
            if let ColumnWidth::Preset(idx) = self.width {
                self.width = self.options.preset_column_widths[idx];
            }
        }

        // If preset heights changed, make our heights non-preset.
        if self.options.preset_window_heights != options.preset_window_heights {
            self.convert_heights_to_auto();
            update_sizes = true;
        }

        if self.options.gaps != options.gaps {
            update_sizes = true;
        }

        if self.options.border.off != options.border.off
            || self.options.border.width != options.border.width
        {
            update_sizes = true;
        }

        for (tile, data) in zip(&mut self.tiles, &mut self.data) {
            tile.update_config(view_size, scale, options.clone());
            data.update(tile);
        }

        self.view_size = view_size;
        self.working_area = working_area;
        self.scale = scale;
        self.options = options;

        if update_sizes {
            self.update_tile_sizes(false);
        }
    }

    fn set_width(&mut self, width: ColumnWidth, animate: bool) {
        self.width = width;
        self.is_full_width = false;
        self.update_tile_sizes(animate);
    }

    pub fn advance_animations(&mut self) {
        if let Some(anim) = &mut self.move_animation {
            if anim.is_done() {
                self.move_animation = None;
            }
        }

        for tile in &mut self.tiles {
            tile.advance_animations();
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.move_animation.is_some() || self.tiles.iter().any(Tile::are_animations_ongoing)
    }

    pub fn update_render_elements(&mut self, is_active: bool, view_rect: Rectangle<f64, Logical>) {
        let active_idx = self.active_tile_idx;
        for (tile_idx, (tile, tile_off)) in self.tiles_mut().enumerate() {
            let is_active = is_active && tile_idx == active_idx;

            let mut tile_view_rect = view_rect;
            tile_view_rect.loc -= tile_off + tile.render_offset();
            tile.update(is_active, tile_view_rect);
        }
    }

    pub fn render_offset(&self) -> Point<f64, Logical> {
        let mut offset = Point::from((0., 0.));

        if let Some(anim) = &self.move_animation {
            offset.x += anim.value();
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
        let current_offset = self.move_animation.as_ref().map_or(0., Animation::value);

        self.move_animation = Some(Animation::new(
            self.clock.clone(),
            from_x_offset + current_offset,
            0.,
            0.,
            config,
        ));
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

    fn activate_window(&mut self, window: &W::Id) {
        let idx = self.position(window).unwrap();
        self.active_tile_idx = idx;
    }

    fn add_tile_at(&mut self, idx: usize, mut tile: Tile<W>, animate: bool) {
        tile.update_config(self.view_size, self.scale, self.options.clone());

        // Inserting a tile pushes down all tiles below it, but also in always-centering mode it
        // will affect the X position of all tiles in the column.
        let mut prev_offsets = Vec::with_capacity(self.tiles.len() + 1);
        prev_offsets.extend(self.tile_offsets().take(self.tiles.len()));

        self.is_fullscreen = false;
        self.data
            .insert(idx, TileData::new(&tile, WindowHeight::auto_1()));
        self.tiles.insert(idx, tile);
        self.update_tile_sizes(animate);

        // Animate tiles according to the offset changes.
        prev_offsets.insert(idx, Point::default());
        for (i, ((tile, offset), prev)) in zip(self.tiles_mut(), prev_offsets).enumerate() {
            if i == idx {
                continue;
            }

            tile.animate_move_from(prev - offset);
        }
    }

    fn update_window(&mut self, window: &W::Id) {
        let (tile_idx, tile) = self
            .tiles
            .iter_mut()
            .enumerate()
            .find(|(_, tile)| tile.window().id() == window)
            .unwrap();

        let height = f64::from(tile.window().size().h);
        let offset = tile
            .window()
            .animation_snapshot()
            .map_or(0., |from| from.size.h - height);

        tile.update_window();
        self.data[tile_idx].update(tile);

        // Move windows below in tandem with resizing.
        //
        // FIXME: in always-centering mode, window resizing will affect the offsets of all other
        // windows in the column, so they should all be animated. How should this interact with
        // animated vs. non-animated resizes? For example, an animated +20 resize followed by two
        // non-animated -10 resizes.
        if tile.resize_animation().is_some() && offset != 0. {
            for tile in &mut self.tiles[tile_idx + 1..] {
                tile.animate_move_y_from_with_config(
                    offset,
                    self.options.animations.window_resize.anim,
                );
            }
        }
    }

    fn update_tile_sizes(&mut self, animate: bool) {
        self.update_tile_sizes_with_transaction(animate, Transaction::new());
    }

    fn update_tile_sizes_with_transaction(&mut self, animate: bool, transaction: Transaction) {
        if self.is_fullscreen {
            self.tiles[0].request_fullscreen();
            return;
        }

        let min_size: Vec<_> = self
            .tiles
            .iter()
            .map(Tile::min_size)
            .map(|mut size| {
                size.w = size.w.max(1.);
                size.h = size.h.max(1.);
                size
            })
            .collect();
        let max_size: Vec<_> = self.tiles.iter().map(Tile::max_size).collect();

        // Compute the column width.
        let min_width = min_size
            .iter()
            .map(|size| NotNan::new(size.w).unwrap())
            .max()
            .map(NotNan::into_inner)
            .unwrap();
        let max_width = max_size
            .iter()
            .filter_map(|size| {
                let w = size.w;
                if w == 0. {
                    None
                } else {
                    Some(NotNan::new(w).unwrap())
                }
            })
            .min()
            .map(NotNan::into_inner)
            .unwrap_or(f64::from(i32::MAX));
        let max_width = f64::max(max_width, min_width);

        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let width = width.resolve(&self.options, self.working_area.size.w);
        let width = f64::max(f64::min(width, max_width), min_width);
        let height = self.working_area.size.h;

        // If there are multiple windows in a column, clamp the non-auto window's height according
        // to other windows' min sizes.
        let mut max_non_auto_window_height = None;
        if self.tiles.len() > 1 {
            if let Some(non_auto_idx) = self
                .data
                .iter()
                .position(|data| !matches!(data.height, WindowHeight::Auto { .. }))
            {
                let min_height_taken = min_size
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| *idx != non_auto_idx)
                    .map(|(_, min_size)| min_size.h + self.options.gaps)
                    .sum::<f64>();

                let tile = &self.tiles[non_auto_idx];
                let height_left = self.working_area.size.h
                    - self.options.gaps
                    - min_height_taken
                    - self.options.gaps;
                max_non_auto_window_height = Some(f64::max(
                    1.,
                    tile.window_height_for_tile_height(height_left).round(),
                ));
            }
        }

        // Compute the tile heights. Start by converting window heights to tile heights.
        let mut heights = zip(&self.tiles, &self.data)
            .map(|(tile, data)| match data.height {
                auto @ WindowHeight::Auto { .. } => auto,
                WindowHeight::Fixed(height) => {
                    let mut window_height = height.round().max(1.);
                    if let Some(max) = max_non_auto_window_height {
                        window_height = f64::min(window_height, max);
                    }

                    WindowHeight::Fixed(tile.tile_height_for_window_height(window_height))
                }
                WindowHeight::Preset(idx) => {
                    let preset = self.options.preset_window_heights[idx];
                    let window_height = match resolve_preset_size(preset, &self.options, height) {
                        ResolvedSize::Tile(h) => tile.window_height_for_tile_height(h),
                        ResolvedSize::Window(h) => h,
                    };

                    let mut window_height = window_height.round().clamp(1., 100000.);
                    if let Some(max) = max_non_auto_window_height {
                        window_height = f64::min(window_height, max);
                    }

                    let tile_height = tile.tile_height_for_window_height(window_height);
                    WindowHeight::Fixed(tile_height)
                }
            })
            .collect::<Vec<_>>();

        let gaps_left = self.options.gaps * (self.tiles.len() + 1) as f64;
        let mut height_left = self.working_area.size.h - gaps_left;
        let mut auto_tiles_left = self.tiles.len();

        // Subtract all fixed-height tiles.
        for (h, (min_size, max_size)) in zip(&mut heights, zip(&min_size, &max_size)) {
            // Check if the tile has an exact height constraint.
            if min_size.h == max_size.h {
                *h = WindowHeight::Fixed(min_size.h);
            }

            if let WindowHeight::Fixed(h) = h {
                if max_size.h > 0. {
                    *h = f64::min(*h, max_size.h);
                }
                *h = f64::max(*h, min_size.h);

                height_left -= *h;
                auto_tiles_left -= 1;
            }
        }

        let mut total_weight: f64 = heights
            .iter()
            .filter_map(|h| {
                if let WindowHeight::Auto { weight } = *h {
                    Some(weight)
                } else {
                    None
                }
            })
            .sum();

        // Iteratively try to distribute the remaining height, checking against tile min heights.
        // Pick an auto height according to the current sizes, then check if it satisfies all
        // remaining min heights. If not, allocate fixed height to those tiles and repeat the
        // loop. On each iteration the auto height will get smaller.
        //
        // NOTE: we do not respect max height here. Doing so would complicate things: if the current
        // auto height is above some tile's max height, then the auto height can become larger.
        // Combining this with the min height loop is where the complexity appears.
        //
        // However, most max height uses are for fixed-size dialogs, where min height == max_height.
        // This case is separately handled above.
        'outer: while auto_tiles_left > 0 {
            // Wayland requires us to round the requested size for a window to integer logical
            // pixels, therefore we compute the remaining auto height dynamically.
            let mut height_left_2 = height_left;
            let mut total_weight_2 = total_weight;
            for ((h, tile), min_size) in zip(zip(&mut heights, &self.tiles), &min_size) {
                let weight = match *h {
                    WindowHeight::Auto { weight } => weight,
                    WindowHeight::Fixed(_) => continue,
                    WindowHeight::Preset(_) => unreachable!(),
                };
                let factor = weight / total_weight_2;

                // Compute the current auto height.
                let mut auto = height_left_2 * factor;

                // Check if the auto height satisfies the min height.
                if min_size.h > auto {
                    auto = min_size.h;
                    *h = WindowHeight::Fixed(auto);
                    height_left -= auto;
                    total_weight -= weight;
                    auto_tiles_left -= 1;

                    // If a min height was unsatisfied, then we allocate the tile more than the
                    // auto height, which means that the remaining auto tiles now have less height
                    // to work with, and the loop must run again.
                    //
                    // If we keep going in this loop and break out later, we may allocate less
                    // height to the subsequent tiles than would be available next iteration and
                    // potentially trip their min height check earlier than necessary, leading to
                    // visible snapping.
                    continue 'outer;
                }

                auto = tile.tile_height_for_window_height(
                    tile.window_height_for_tile_height(auto).round().max(1.),
                );

                height_left_2 -= auto;
                total_weight_2 -= weight;
            }

            // All min heights were satisfied, fill them in.
            for (h, tile) in zip(&mut heights, &self.tiles) {
                let weight = match *h {
                    WindowHeight::Auto { weight } => weight,
                    WindowHeight::Fixed(_) => continue,
                    WindowHeight::Preset(_) => unreachable!(),
                };
                let factor = weight / total_weight;

                // Compute the current auto height.
                let auto = height_left * factor;
                let auto = tile.tile_height_for_window_height(
                    tile.window_height_for_tile_height(auto).round().max(1.),
                );

                *h = WindowHeight::Fixed(auto);
                height_left -= auto;
                total_weight -= weight;
                auto_tiles_left -= 1;
            }

            assert_eq!(auto_tiles_left, 0);
        }

        for (tile, h) in zip(&mut self.tiles, heights) {
            let WindowHeight::Fixed(height) = h else {
                unreachable!()
            };

            let size = Size::from((width, height));
            tile.request_tile_size(size, animate, Some(transaction.clone()));
        }
    }

    fn width(&self) -> f64 {
        self.data
            .iter()
            .map(|data| NotNan::new(data.size.w).unwrap())
            .max()
            .map(NotNan::into_inner)
            .unwrap()
    }

    fn focus_up(&mut self) -> bool {
        if self.active_tile_idx == 0 {
            return false;
        }

        self.active_tile_idx -= 1;
        true
    }

    fn focus_down(&mut self) -> bool {
        if self.active_tile_idx == self.tiles.len() - 1 {
            return false;
        }

        self.active_tile_idx += 1;
        true
    }

    fn move_up(&mut self) -> bool {
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

    fn move_down(&mut self) -> bool {
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

    fn toggle_width(&mut self) {
        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let idx = match width {
            ColumnWidth::Preset(idx) => (idx + 1) % self.options.preset_column_widths.len(),
            _ => {
                let current = self.width();
                self.options
                    .preset_column_widths
                    .iter()
                    .position(|prop| {
                        let resolved = prop.resolve(&self.options, self.working_area.size.w);
                        // Some allowance for fractional scaling purposes.
                        current + 1. < resolved
                    })
                    .unwrap_or(0)
            }
        };
        let width = ColumnWidth::Preset(idx);
        self.set_width(width, true);
    }

    fn toggle_full_width(&mut self) {
        self.is_full_width = !self.is_full_width;
        self.update_tile_sizes(true);
    }

    fn set_column_width(&mut self, change: SizeChange, tile_idx: Option<usize>, animate: bool) {
        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let current_px = width.resolve(&self.options, self.working_area.size.w);

        let current = match width {
            ColumnWidth::Preset(idx) => self.options.preset_column_widths[idx],
            current => current,
        };

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
                let full = self.working_area.size.w - self.options.gaps;
                let current = if full == 0. {
                    1.
                } else {
                    (current_px + self.options.gaps) / full
                };
                let proportion = (current + delta / 100.).clamp(0., MAX_F);
                ColumnWidth::Proportion(proportion)
            }
            (ColumnWidth::Preset(_), _) => unreachable!(),
        };

        self.set_width(width, animate);
    }

    fn set_window_height(&mut self, change: SizeChange, tile_idx: Option<usize>, animate: bool) {
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

        let full = self.working_area.size.h - self.options.gaps;
        let current_prop = if full == 0. {
            1.
        } else {
            (current_tile_px + self.options.gaps) / (full)
        };

        // FIXME: fix overflows then remove limits.
        const MAX_PX: f64 = 100000.;

        let mut window_height = match change {
            SizeChange::SetFixed(fixed) => f64::from(fixed),
            SizeChange::SetProportion(proportion) => {
                let tile_height = (self.working_area.size.h - self.options.gaps)
                    * (proportion / 100.)
                    - self.options.gaps;
                tile.window_height_for_tile_height(tile_height)
            }
            SizeChange::AdjustFixed(delta) => current_window_px + f64::from(delta),
            SizeChange::AdjustProportion(delta) => {
                let proportion = current_prop + delta / 100.;
                let tile_height =
                    (self.working_area.size.h - self.options.gaps) * proportion - self.options.gaps;
                tile.window_height_for_tile_height(tile_height)
            }
        };

        // Clamp the height according to other windows' min sizes, or simply to working area height.
        let min_height_taken = self
            .tiles
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != tile_idx)
            .map(|(_, tile)| f64::max(1., tile.min_size().h) + self.options.gaps)
            .sum::<f64>();
        let height_left =
            self.working_area.size.h - self.options.gaps - min_height_taken - self.options.gaps;
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
        self.update_tile_sizes(animate);
    }

    fn reset_window_height(&mut self, tile_idx: Option<usize>, animate: bool) {
        let tile_idx = tile_idx.unwrap_or(self.active_tile_idx);
        self.data[tile_idx].height = WindowHeight::auto_1();
        self.update_tile_sizes(animate);
    }

    fn toggle_window_height(&mut self, tile_idx: Option<usize>, animate: bool) {
        let tile_idx = tile_idx.unwrap_or(self.active_tile_idx);

        // Start by converting all heights to automatic, since only one window in the column can be
        // non-auto-height. If the current tile is already non-auto, however, we can skip that
        // step. Which is not only for optimization, but also preserves automatic weights in case
        // one window is resized in such a way that other windows hit their min size, and then
        // back.
        if matches!(self.data[tile_idx].height, WindowHeight::Auto { .. }) {
            self.convert_heights_to_auto();
        }

        let preset_idx = match self.data[tile_idx].height {
            WindowHeight::Preset(idx) => (idx + 1) % self.options.preset_window_heights.len(),
            _ => {
                let current = self.data[tile_idx].size.h;
                let tile = &self.tiles[tile_idx];
                self.options
                    .preset_window_heights
                    .iter()
                    .copied()
                    .position(|preset| {
                        let resolved =
                            resolve_preset_size(preset, &self.options, self.working_area.size.h);
                        let window_height = match resolved {
                            ResolvedSize::Tile(h) => tile.window_height_for_tile_height(h),
                            ResolvedSize::Window(h) => h,
                        };
                        let resolved = tile.tile_height_for_window_height(
                            window_height.round().clamp(1., 100000.),
                        );

                        // Some allowance for fractional scaling purposes.
                        current + 1. < resolved
                    })
                    .unwrap_or(0)
            }
        };
        self.data[tile_idx].height = WindowHeight::Preset(preset_idx);
        self.update_tile_sizes(animate);
    }

    /// Converts all heights in the column to automatic, preserving the apparent heights.
    ///
    /// All weights are recomputed to preserve the current tile heights while "centering" the
    /// weights at the median window height (it gets weight = 1).
    ///
    /// One case where apparent heights will not be preserved is when the column is taller than the
    /// working area.
    fn convert_heights_to_auto(&mut self) {
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

    fn set_fullscreen(&mut self, is_fullscreen: bool) {
        if self.is_fullscreen == is_fullscreen {
            return;
        }

        assert_eq!(self.tiles.len(), 1);
        self.is_fullscreen = is_fullscreen;
        self.update_tile_sizes(false);
    }

    fn popup_target_rect(&self, id: &W::Id) -> Option<Rectangle<f64, Logical>> {
        for (tile, pos) in self.tiles() {
            if tile.window().id() == id {
                // In the scrolling layout, we try to position popups horizontally within the
                // window geometry (so they remain visible even if the window scrolls flush with
                // the left/right edge of the screen), and vertically wihin the whole view size.
                let width = tile.window_size().w;
                let height = self.view_size.h;

                let mut target = Rectangle::from_loc_and_size((0., 0.), (width, height));
                target.loc.y -= pos.y;
                target.loc.y -= tile.window_loc().y;

                return Some(target);
            }
        }
        None
    }

    // HACK: pass a self.data iterator in manually as a workaround for the lack of method partial
    // borrowing. Note that this method's return value does not borrow the entire &Self!
    fn tile_offsets_iter(
        &self,
        data: impl Iterator<Item = TileData>,
    ) -> impl Iterator<Item = Point<f64, Logical>> {
        // FIXME: this should take into account always-center-single-column, which means that
        // Column should somehow know when it is being centered due to being the single column on
        // the workspace or some other reason.
        let center = self.options.center_focused_column == CenterFocusedColumn::Always;
        let gaps = self.options.gaps;
        let col_width = if self.tiles.is_empty() {
            0.
        } else {
            self.width()
        };
        let mut y = 0.;

        if !self.is_fullscreen {
            y = self.working_area.loc.y + self.options.gaps;
        }

        // Chain with a dummy value to be able to get one past all tiles' Y.
        let dummy = TileData {
            height: WindowHeight::auto_1(),
            size: Size::default(),
            interactively_resizing_by_left_edge: false,
        };
        let data = data.chain(iter::once(dummy));

        data.map(move |data| {
            let mut pos = Point::from((0., y));

            if center {
                pos.x = (col_width - data.size.w) / 2.;
            } else if data.interactively_resizing_by_left_edge {
                pos.x = col_width - data.size.w;
            }

            y += data.size.h + gaps;
            pos
        })
    }

    fn tile_offsets(&self) -> impl Iterator<Item = Point<f64, Logical>> + '_ {
        self.tile_offsets_iter(self.data.iter().copied())
    }

    fn tile_offset(&self, tile_idx: usize) -> Point<f64, Logical> {
        self.tile_offsets().nth(tile_idx).unwrap()
    }

    fn tile_offsets_in_render_order(
        &self,
        data: impl Iterator<Item = TileData>,
    ) -> impl Iterator<Item = Point<f64, Logical>> {
        let active_idx = self.active_tile_idx;
        let active_pos = self.tile_offset(active_idx);
        let offsets = self
            .tile_offsets_iter(data)
            .enumerate()
            .filter_map(move |(idx, pos)| (idx != active_idx).then_some(pos));
        iter::once(active_pos).chain(offsets)
    }

    pub fn tiles(&self) -> impl Iterator<Item = (&Tile<W>, Point<f64, Logical>)> + '_ {
        let offsets = self.tile_offsets_iter(self.data.iter().copied());
        zip(&self.tiles, offsets)
    }

    fn tiles_mut(&mut self) -> impl Iterator<Item = (&mut Tile<W>, Point<f64, Logical>)> + '_ {
        let offsets = self.tile_offsets_iter(self.data.iter().copied());
        zip(&mut self.tiles, offsets)
    }

    fn tiles_in_render_order(&self) -> impl Iterator<Item = (&Tile<W>, Point<f64, Logical>)> + '_ {
        let offsets = self.tile_offsets_in_render_order(self.data.iter().copied());

        let (first, rest) = self.tiles.split_at(self.active_tile_idx);
        let (active, rest) = rest.split_at(1);

        let tiles = active.iter().chain(first).chain(rest);
        zip(tiles, offsets)
    }

    fn tiles_in_render_order_mut(
        &mut self,
    ) -> impl Iterator<Item = (&mut Tile<W>, Point<f64, Logical>)> + '_ {
        let offsets = self.tile_offsets_in_render_order(self.data.iter().copied());

        let (first, rest) = self.tiles.split_at_mut(self.active_tile_idx);
        let (active, rest) = rest.split_at_mut(1);

        let tiles = active.iter_mut().chain(first).chain(rest);
        zip(tiles, offsets)
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        assert!(!self.tiles.is_empty(), "columns can't be empty");
        assert!(self.active_tile_idx < self.tiles.len());
        assert_eq!(self.tiles.len(), self.data.len());

        if self.is_fullscreen {
            assert_eq!(self.tiles.len(), 1);
        }

        let tile_count = self.tiles.len();
        if tile_count == 1 {
            if let WindowHeight::Auto { weight } = self.data[0].height {
                assert_eq!(
                    weight, 1.,
                    "auto height weight must reset to 1 for a single window"
                );
            }
        }

        let mut found_fixed = false;
        let mut total_height = 0.;
        let mut total_min_height = 0.;
        for (tile, data) in zip(&self.tiles, &self.data) {
            assert!(Rc::ptr_eq(&self.options, &tile.options));
            assert_eq!(self.clock, tile.clock);
            assert_eq!(self.scale, tile.scale());
            assert_eq!(self.is_fullscreen, tile.window().is_pending_fullscreen());
            assert_eq!(self.view_size, tile.view_size());
            tile.verify_invariants();

            let mut data2 = *data;
            data2.update(tile);
            assert_eq!(data, &data2, "tile data must be up to date");

            if matches!(data.height, WindowHeight::Fixed(_)) {
                assert!(
                    !found_fixed,
                    "there can only be one fixed-height window in a column"
                );
                found_fixed = true;
            }

            if let WindowHeight::Preset(idx) = data.height {
                assert!(self.options.preset_window_heights.len() > idx);
            }

            let requested_size = tile.window().requested_size().unwrap();
            total_height += tile.tile_height_for_window_height(f64::from(requested_size.h));
            total_min_height += f64::max(1., tile.min_size().h);
        }

        if tile_count > 1
            && self.scale.round() == self.scale
            && self.working_area.size.h.round() == self.working_area.size.h
            && self.options.gaps.round() == self.options.gaps
        {
            total_height += self.options.gaps * (tile_count + 1) as f64;
            total_min_height += self.options.gaps * (tile_count + 1) as f64;
            let max_height = f64::max(total_min_height, self.working_area.size.h);
            assert!(
                total_height <= max_height,
                "multiple tiles in a column mustn't go beyond working area height \
                 (total height {total_height} > max height {max_height})"
            );
        }
    }
}

fn compute_new_view_offset(
    cur_x: f64,
    view_width: f64,
    new_col_x: f64,
    new_col_width: f64,
    gaps: f64,
) -> f64 {
    // If the column is wider than the view, always left-align it.
    if view_width <= new_col_width {
        return 0.;
    }

    // Compute the padding in case it needs to be smaller due to large tile width.
    let padding = ((view_width - new_col_width) / 2.).clamp(0., gaps);

    // Compute the desired new X with padding.
    let new_x = new_col_x - padding;
    let new_right_x = new_col_x + new_col_width + padding;

    // If the column is already fully visible, leave the view as is.
    if cur_x <= new_x && new_right_x <= cur_x + view_width {
        return -(new_col_x - cur_x);
    }

    // Otherwise, prefer the alignment that results in less motion from the current position.
    let dist_to_left = (cur_x - new_x).abs();
    let dist_to_right = ((cur_x + view_width) - new_right_x).abs();
    if dist_to_left <= dist_to_right {
        -padding
    } else {
        -(view_width - padding - new_col_width)
    }
}

fn compute_working_area(
    parent_area: Rectangle<f64, Logical>,
    scale: f64,
    struts: Struts,
) -> Rectangle<f64, Logical> {
    let mut working_area = parent_area;

    // Add struts.
    working_area.size.w = f64::max(0., working_area.size.w - struts.left.0 - struts.right.0);
    working_area.loc.x += struts.left.0;

    working_area.size.h = f64::max(0., working_area.size.h - struts.top.0 - struts.bottom.0);
    working_area.loc.y += struts.top.0;

    // Round location to start at a physical pixel.
    let loc = working_area
        .loc
        .to_physical_precise_ceil(scale)
        .to_logical(scale);

    let mut size_diff = (loc - working_area.loc).to_size();
    size_diff.w = f64::min(working_area.size.w, size_diff.w);
    size_diff.h = f64::min(working_area.size.h, size_diff.h);

    working_area.size -= size_diff;
    working_area.loc = loc;

    working_area
}

fn compute_toplevel_bounds(
    border_config: niri_config::Border,
    working_area_size: Size<f64, Logical>,
    gaps: f64,
) -> Size<i32, Logical> {
    let mut border = 0.;
    if !border_config.off {
        border = border_config.width.0 * 2.;
    }

    Size::from((
        f64::max(working_area_size.w - gaps * 2. - border, 1.),
        f64::max(working_area_size.h - gaps * 2. - border, 1.),
    ))
    .to_i32_floor()
}

fn cancel_resize_for_column<W: LayoutElement>(
    interactive_resize: &mut Option<InteractiveResize<W>>,
    column: &mut Column<W>,
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

fn resolve_preset_size(preset: PresetSize, options: &Options, view_size: f64) -> ResolvedSize {
    match preset {
        PresetSize::Proportion(proportion) => {
            ResolvedSize::Tile((view_size - options.gaps) * proportion - options.gaps)
        }
        PresetSize::Fixed(width) => ResolvedSize::Window(f64::from(width)),
    }
}

#[cfg(test)]
mod tests {
    use niri_config::FloatOrInt;

    use super::*;
    use crate::utils::round_logical_in_physical;

    #[test]
    fn working_area_starts_at_physical_pixel() {
        let struts = Struts {
            left: FloatOrInt(0.5),
            right: FloatOrInt(1.),
            top: FloatOrInt(0.75),
            bottom: FloatOrInt(1.),
        };

        let parent_area = Rectangle::from_loc_and_size((0., 0.), (1280., 720.));
        let area = compute_working_area(parent_area, 1., struts);

        assert_eq!(round_logical_in_physical(1., area.loc.x), area.loc.x);
        assert_eq!(round_logical_in_physical(1., area.loc.y), area.loc.y);
    }

    #[test]
    fn large_fractional_strut() {
        let struts = Struts {
            left: FloatOrInt(0.),
            right: FloatOrInt(0.),
            top: FloatOrInt(50000.5),
            bottom: FloatOrInt(0.),
        };

        let parent_area = Rectangle::from_loc_and_size((0., 0.), (1280., 720.));
        compute_working_area(parent_area, 1., struts);
    }
}
