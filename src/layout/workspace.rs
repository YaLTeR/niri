use std::cmp::{max, min};
use std::iter::{self, zip};
use std::rc::Rc;
use std::time::Duration;

use niri_config::{CenterFocusedColumn, PresetWidth, Struts};
use niri_ipc::SizeChange;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{layer_map_for_output, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};
use smithay::wayland::compositor::send_surface_state;

use super::closing_window::{ClosingWindow, ClosingWindowRenderElement};
use super::tile::{Tile, TileRenderElement};
use super::{LayoutElement, Options};
use crate::animation::Animation;
use crate::niri_render_elements;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::RenderTarget;
use crate::swipe_tracker::SwipeTracker;
use crate::utils::id::IdCounter;
use crate::utils::output_size;
use crate::window::ResolvedWindowRules;

/// Amount of touchpad movement to scroll the view for the width of one working area.
const VIEW_GESTURE_WORKING_AREA_MOVEMENT: f64 = 1200.;

#[derive(Debug)]
pub struct Workspace<W: LayoutElement> {
    /// The original output of this workspace.
    ///
    /// Most of the time this will be the workspace's current output, however, after an output
    /// disconnection, it may remain pointing to the disconnected output.
    pub original_output: OutputId,

    /// Current output of this workspace.
    output: Option<Output>,

    /// Latest known view size for this workspace.
    ///
    /// This should be computed from the current workspace output size, or, if all outputs have
    /// been disconnected, preserved until a new output is connected.
    view_size: Size<i32, Logical>,

    /// Latest known working area for this workspace.
    ///
    /// This is similar to view size, but takes into account things like layer shell exclusive
    /// zones.
    working_area: Rectangle<i32, Logical>,

    /// Columns of windows on this workspace.
    pub columns: Vec<Column<W>>,

    /// Index of the currently active column, if any.
    pub active_column_idx: usize,

    /// Offset of the view computed from the active column.
    ///
    /// Any gaps, including left padding from work area left exclusive zone, is handled
    /// with this view offset (rather than added as a constant elsewhere in the code). This allows
    /// for natural handling of fullscreen windows, which must ignore work area padding.
    view_offset: i32,

    /// Adjustment of the view offset, if one is currently ongoing.
    view_offset_adj: Option<ViewOffsetAdjustment>,

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
    activate_prev_column_on_removal: Option<i32>,

    /// View offset to restore after unfullscreening.
    view_offset_before_fullscreen: Option<i32>,

    /// Windows in the closing animation.
    closing_windows: Vec<ClosingWindow>,

    /// Configurable properties of the layout.
    pub options: Rc<Options>,

    /// Unique ID of this workspace.
    id: WorkspaceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputId(String);

static WORKSPACE_ID_COUNTER: IdCounter = IdCounter::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId(u32);

impl WorkspaceId {
    fn next() -> WorkspaceId {
        WorkspaceId(WORKSPACE_ID_COUNTER.next())
    }
}

niri_render_elements! {
    WorkspaceRenderElement<R> => {
        Tile = TileRenderElement<R>,
        ClosingWindow = ClosingWindowRenderElement,
    }
}

#[derive(Debug)]
enum ViewOffsetAdjustment {
    Animation(Animation),
    Gesture(ViewGesture),
}

#[derive(Debug)]
struct ViewGesture {
    current_view_offset: f64,
    tracker: SwipeTracker,
    delta_from_tracker: f64,
    // The view offset we'll use if needed for activate_prev_column_on_removal.
    static_view_offset: i32,
}

/// Width of a column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    /// Proportion of the current view width.
    Proportion(f64),
    /// One of the proportion presets.
    ///
    /// This is separate from Proportion in order to be able to reliably cycle between preset
    /// proportions.
    Preset(usize),
    /// Fixed width in logical pixels.
    Fixed(i32),
}

/// Height of a window in a column.
///
/// Proportional height is intentionally omitted. With column widths you frequently want e.g. two
/// columns side-by-side with 50% width each, and you want them to remain this way when moving to a
/// differently sized monitor. Windows in a column, however, already auto-size to fill the available
/// height, giving you this behavior. The only reason to set a different window height, then, is
/// when you want something in the window to fit exactly, e.g. to fit 30 lines in a terminal, which
/// corresponds to the `Fixed` variant.
///
/// This does not preclude the usual set of binds to set or resize a window proportionally. Just,
/// they are converted to, and stored as fixed height right away, so that once you resize a window
/// to fit the desired content, it can never become smaller than that when moving between monitors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowHeight {
    /// Automatically computed height, evenly distributed across the column.
    Auto,
    /// Fixed height in logical pixels.
    Fixed(i32),
}

#[derive(Debug)]
pub struct Column<W: LayoutElement> {
    /// Tiles in this column.
    ///
    /// Must be non-empty.
    pub tiles: Vec<Tile<W>>,

    /// Heights of the windows.
    ///
    /// Must have the same number of elements as `tiles`.
    ///
    /// These heights are window heights, not tile heights, so they exclude tile decorations.
    heights: Vec<WindowHeight>,

    /// Index of the currently active tile.
    pub active_tile_idx: usize,

    /// Desired width of this column.
    ///
    /// If the column is full-width or full-screened, this is the width that should be restored
    /// upon unfullscreening and untoggling full-width.
    pub width: ColumnWidth,

    /// Whether this column is full-width.
    pub is_full_width: bool,

    /// Whether this column contains a single full-screened window.
    pub is_fullscreen: bool,

    /// Animation of the render offset during window swapping.
    move_animation: Option<Animation>,

    /// Width right before a resize animation on one of the windows of the column.
    pub width_before_resize: Option<i32>,

    /// Latest known view size for this column's workspace.
    view_size: Size<i32, Logical>,

    /// Latest known working area for this column's workspace.
    working_area: Rectangle<i32, Logical>,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

impl OutputId {
    pub fn new(output: &Output) -> Self {
        Self(output.name())
    }
}

impl ViewOffsetAdjustment {
    pub fn is_animation(&self) -> bool {
        matches!(self, Self::Animation(_))
    }

    pub fn target_view_offset(&self) -> f64 {
        match self {
            ViewOffsetAdjustment::Animation(anim) => anim.to(),
            ViewOffsetAdjustment::Gesture(gesture) => gesture.current_view_offset,
        }
    }
}

impl ColumnWidth {
    fn resolve(self, options: &Options, view_width: i32) -> i32 {
        match self {
            ColumnWidth::Proportion(proportion) => {
                ((view_width - options.gaps) as f64 * proportion).floor() as i32 - options.gaps
            }
            ColumnWidth::Preset(idx) => options.preset_widths[idx].resolve(options, view_width),
            ColumnWidth::Fixed(width) => width,
        }
    }
}

impl From<PresetWidth> for ColumnWidth {
    fn from(value: PresetWidth) -> Self {
        match value {
            PresetWidth::Proportion(p) => Self::Proportion(p.clamp(0., 10000.)),
            PresetWidth::Fixed(f) => Self::Fixed(f.clamp(1, 100000)),
        }
    }
}

impl<W: LayoutElement> Workspace<W> {
    pub fn new(output: Output, options: Rc<Options>) -> Self {
        let working_area = compute_working_area(&output, options.struts);
        Self {
            original_output: OutputId::new(&output),
            view_size: output_size(&output),
            working_area,
            output: Some(output),
            columns: vec![],
            active_column_idx: 0,
            view_offset: 0,
            view_offset_adj: None,
            activate_prev_column_on_removal: None,
            view_offset_before_fullscreen: None,
            closing_windows: vec![],
            options,
            id: WorkspaceId::next(),
        }
    }

    pub fn new_no_outputs(options: Rc<Options>) -> Self {
        Self {
            output: None,
            original_output: OutputId(String::new()),
            view_size: Size::from((1280, 720)),
            working_area: Rectangle::from_loc_and_size((0, 0), (1280, 720)),
            columns: vec![],
            active_column_idx: 0,
            view_offset: 0,
            view_offset_adj: None,
            activate_prev_column_on_removal: None,
            view_offset_before_fullscreen: None,
            closing_windows: vec![],
            options,
            id: WorkspaceId::next(),
        }
    }

    pub fn id(&self) -> WorkspaceId {
        self.id
    }

    pub fn advance_animations(&mut self, current_time: Duration, is_active: bool) {
        if let Some(ViewOffsetAdjustment::Animation(anim)) = &mut self.view_offset_adj {
            anim.set_current_time(current_time);
            self.view_offset = anim.value().round() as i32;
            if anim.is_done() {
                self.view_offset_adj = None;
            }
        } else if let Some(ViewOffsetAdjustment::Gesture(gesture)) = &self.view_offset_adj {
            self.view_offset = gesture.current_view_offset.round() as i32;
        }

        for (col_idx, col) in self.columns.iter_mut().enumerate() {
            let is_active = is_active && col_idx == self.active_column_idx;
            col.advance_animations(current_time, is_active);
        }

        self.closing_windows.retain_mut(|closing| {
            closing.advance_animations(current_time);
            closing.are_animations_ongoing()
        });
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.view_offset_adj
            .as_ref()
            .is_some_and(|s| s.is_animation())
            || self.columns.iter().any(Column::are_animations_ongoing)
            || !self.closing_windows.is_empty()
    }

    pub fn are_transitions_ongoing(&self) -> bool {
        self.view_offset_adj.is_some()
            || self.columns.iter().any(Column::are_animations_ongoing)
            || !self.closing_windows.is_empty()
    }

    pub fn update_config(&mut self, options: Rc<Options>) {
        for column in &mut self.columns {
            column.update_config(options.clone());
        }

        self.options = options;
    }

    pub fn windows(&self) -> impl Iterator<Item = &W> + '_ {
        self.columns
            .iter()
            .flat_map(|col| col.tiles.iter())
            .map(Tile::window)
    }

    pub fn windows_mut(&mut self) -> impl Iterator<Item = &mut W> + '_ {
        self.columns
            .iter_mut()
            .flat_map(|col| col.tiles.iter_mut())
            .map(Tile::window_mut)
    }

    pub fn set_output(&mut self, output: Option<Output>) {
        if self.output == output {
            return;
        }

        if let Some(output) = self.output.take() {
            for win in self.windows() {
                win.output_leave(&output);
            }
        }

        self.output = output;

        if let Some(output) = &self.output {
            let working_area = compute_working_area(output, self.options.struts);
            self.set_view_size(output_size(output), working_area);

            for win in self.windows() {
                self.enter_output_for_window(win);
            }
        }
    }

    fn enter_output_for_window(&self, window: &W) {
        if let Some(output) = &self.output {
            set_preferred_scale_transform(window, output);
            window.output_enter(output);
        }
    }

    pub fn set_view_size(
        &mut self,
        size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
    ) {
        if self.view_size == size && self.working_area == working_area {
            return;
        }

        self.view_size = size;
        self.working_area = working_area;

        for col in &mut self.columns {
            col.set_view_size(self.view_size, self.working_area);
        }
    }

    pub fn view_size(&self) -> Size<i32, Logical> {
        self.view_size
    }

    pub fn update_output_scale_transform(&mut self) {
        let Some(output) = self.output.as_ref() else {
            return;
        };
        for window in self.windows() {
            set_preferred_scale_transform(window, output);
        }
    }

    fn toplevel_bounds(&self, rules: &ResolvedWindowRules) -> Size<i32, Logical> {
        let border_config = rules.border.resolve_against(self.options.border);
        compute_toplevel_bounds(border_config, self.working_area.size, self.options.gaps)
    }

    pub fn resolve_default_width(
        &self,
        default_width: Option<Option<ColumnWidth>>,
    ) -> Option<ColumnWidth> {
        match default_width {
            Some(Some(width)) => Some(width),
            Some(None) => None,
            None => self.options.default_width,
        }
    }

    pub fn new_window_size(
        &self,
        width: Option<ColumnWidth>,
        rules: &ResolvedWindowRules,
    ) -> Size<i32, Logical> {
        let border = rules.border.resolve_against(self.options.border);

        let width = if let Some(width) = width {
            let is_fixed = matches!(width, ColumnWidth::Fixed(_));

            let mut width = width.resolve(&self.options, self.working_area.size.w);

            if !is_fixed && !border.off {
                width -= border.width as i32 * 2;
            }

            max(1, width)
        } else {
            0
        };

        let mut height = self.working_area.size.h - self.options.gaps * 2;
        if !border.off {
            height -= border.width as i32 * 2;
        }

        Size::from((width, max(height, 1)))
    }

    pub fn configure_new_window(
        &self,
        window: &Window,
        width: Option<ColumnWidth>,
        rules: &ResolvedWindowRules,
    ) {
        if let Some(output) = self.output.as_ref() {
            let scale = output.current_scale().integer_scale();
            let transform = output.current_transform();
            window.with_surfaces(|surface, data| {
                send_surface_state(surface, data, scale, transform);
            });
        }

        window
            .toplevel()
            .expect("no x11 support")
            .with_pending_state(|state| {
                if state.states.contains(xdg_toplevel::State::Fullscreen) {
                    state.size = Some(self.view_size);
                } else {
                    state.size = Some(self.new_window_size(width, rules));
                }

                state.bounds = Some(self.toplevel_bounds(rules));
            });
    }

    fn compute_new_view_offset_for_column(&self, current_x: i32, idx: usize) -> i32 {
        if self.columns[idx].is_fullscreen {
            return 0;
        }

        let new_col_x = self.column_x(idx);

        let final_x = if let Some(ViewOffsetAdjustment::Animation(anim)) = &self.view_offset_adj {
            current_x - self.view_offset + anim.to().round() as i32
        } else {
            current_x
        };

        let new_offset = compute_new_view_offset(
            final_x + self.working_area.loc.x,
            self.working_area.size.w,
            new_col_x,
            self.columns[idx].width(),
            self.options.gaps,
        );

        // Non-fullscreen windows are always offset at least by the working area position.
        new_offset - self.working_area.loc.x
    }

    fn animate_view_offset(&mut self, current_x: i32, idx: usize, new_view_offset: i32) {
        self.animate_view_offset_with_config(
            current_x,
            idx,
            new_view_offset,
            self.options.animations.horizontal_view_movement.0,
        );
    }

    fn animate_view_offset_with_config(
        &mut self,
        current_x: i32,
        idx: usize,
        new_view_offset: i32,
        config: niri_config::Animation,
    ) {
        let new_col_x = self.column_x(idx);
        let from_view_offset = current_x - new_col_x;
        self.view_offset = from_view_offset;

        // If we're already animating towards that, don't restart it.
        if let Some(ViewOffsetAdjustment::Animation(anim)) = &self.view_offset_adj {
            if anim.value().round() as i32 == self.view_offset
                && anim.to().round() as i32 == new_view_offset
            {
                return;
            }
        }

        // If our view offset is already this, we don't need to do anything.
        if self.view_offset == new_view_offset {
            self.view_offset_adj = None;
            return;
        }

        // FIXME: also compute and use current velocity.
        self.view_offset_adj = Some(ViewOffsetAdjustment::Animation(Animation::new(
            self.view_offset as f64,
            new_view_offset as f64,
            0.,
            config,
        )));
    }

    fn animate_view_offset_to_column_fit(
        &mut self,
        current_x: i32,
        idx: usize,
        config: niri_config::Animation,
    ) {
        let new_view_offset = self.compute_new_view_offset_for_column(current_x, idx);
        self.animate_view_offset_with_config(current_x, idx, new_view_offset, config);
    }

    fn animate_view_offset_to_column_centered(
        &mut self,
        current_x: i32,
        idx: usize,
        config: niri_config::Animation,
    ) {
        if self.columns.is_empty() {
            return;
        }

        let col = &self.columns[idx];
        if col.is_fullscreen {
            self.animate_view_offset_to_column_fit(current_x, idx, config);
            return;
        }

        let width = col.width();

        // If the column is wider than the working area, then on commit it will be shifted to left
        // edge alignment by the usual positioning code, so there's no use in trying to center it
        // here.
        if self.working_area.size.w <= width {
            self.animate_view_offset_to_column_fit(current_x, idx, config);
            return;
        }

        let new_view_offset = -(self.working_area.size.w - width) / 2 - self.working_area.loc.x;

        self.animate_view_offset_with_config(current_x, idx, new_view_offset, config);
    }

    fn animate_view_offset_to_column(
        &mut self,
        current_x: i32,
        idx: usize,
        prev_idx: Option<usize>,
    ) {
        self.animate_view_offset_to_column_with_config(
            current_x,
            idx,
            prev_idx,
            self.options.animations.horizontal_view_movement.0,
        )
    }

    fn animate_view_offset_to_column_with_config(
        &mut self,
        current_x: i32,
        idx: usize,
        prev_idx: Option<usize>,
        config: niri_config::Animation,
    ) {
        match self.options.center_focused_column {
            CenterFocusedColumn::Always => {
                self.animate_view_offset_to_column_centered(current_x, idx, config)
            }
            CenterFocusedColumn::OnOverflow => {
                let Some(prev_idx) = prev_idx else {
                    self.animate_view_offset_to_column_fit(current_x, idx, config);
                    return;
                };

                // Always take the left or right neighbor of the target as the source.
                let source_idx = if prev_idx > idx {
                    min(idx + 1, self.columns.len() - 1)
                } else {
                    idx.saturating_sub(1)
                };

                let source_x = self.column_x(source_idx);
                let source_width = self.columns[source_idx].width();

                let target_x = self.column_x(idx);
                let target_width = self.columns[idx].width();

                let total_width = if source_x < target_x {
                    // Source is left from target.
                    target_x - source_x + target_width
                } else {
                    // Source is right from target.
                    source_x - target_x + source_width
                } + self.options.gaps * 2;

                // If it fits together, do a normal animation, otherwise center the new column.
                if total_width <= self.working_area.size.w {
                    self.animate_view_offset_to_column_fit(current_x, idx, config);
                } else {
                    self.animate_view_offset_to_column_centered(current_x, idx, config);
                }
            }
            CenterFocusedColumn::Never => {
                self.animate_view_offset_to_column_fit(current_x, idx, config)
            }
        };
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

        let current_x = self.view_pos();
        self.animate_view_offset_to_column_with_config(
            current_x,
            idx,
            Some(self.active_column_idx),
            config,
        );

        self.active_column_idx = idx;

        // A different column was activated; reset the flag.
        self.activate_prev_column_on_removal = None;
        self.view_offset_before_fullscreen = None;
    }

    pub fn has_windows(&self) -> bool {
        self.windows().next().is_some()
    }

    pub fn has_window(&self, window: &W::Id) -> bool {
        self.windows().any(|win| win.id() == window)
    }

    pub fn find_wl_surface(&self, wl_surface: &WlSurface) -> Option<&W> {
        self.windows().find(|win| win.is_wl_surface(wl_surface))
    }

    pub fn find_wl_surface_mut(&mut self, wl_surface: &WlSurface) -> Option<&mut W> {
        self.windows_mut().find(|win| win.is_wl_surface(wl_surface))
    }

    /// Computes the X position of the windows in the given column, in logical coordinates.
    pub fn column_x(&self, column_idx: usize) -> i32 {
        let mut x = 0;

        for column in self.columns.iter().take(column_idx) {
            x += column.width() + self.options.gaps;
        }

        x
    }

    pub fn add_window_at(
        &mut self,
        col_idx: usize,
        window: W,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        let tile = Tile::new(window, self.options.clone());
        self.add_tile_at(col_idx, tile, activate, width, is_full_width, None);
    }

    fn add_tile_at(
        &mut self,
        col_idx: usize,
        tile: Tile<W>,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
        anim_config: Option<niri_config::Animation>,
    ) {
        self.enter_output_for_window(tile.window());

        let was_empty = self.columns.is_empty();

        let column = Column::new_with_tile(
            tile,
            self.view_size,
            self.working_area,
            self.options.clone(),
            width,
            is_full_width,
            true,
        );
        let width = column.width();
        self.columns.insert(col_idx, column);

        if activate {
            // If this is the first window on an empty workspace, skip the animation from whatever
            // view_offset was left over.
            if was_empty {
                if self.options.center_focused_column == CenterFocusedColumn::Always {
                    self.view_offset =
                        -(self.working_area.size.w - width) / 2 - self.working_area.loc.x;
                } else {
                    // Try to make the code produce a left-aligned offset, even in presence of left
                    // exclusive zones.
                    self.view_offset = self.compute_new_view_offset_for_column(self.column_x(0), 0);
                }
                self.view_offset_adj = None;
            }

            let prev_offset = (!was_empty).then(|| self.static_view_offset());

            self.activate_column_with_anim_config(
                col_idx,
                anim_config.unwrap_or(self.options.animations.horizontal_view_movement.0),
            );
            self.activate_prev_column_on_removal = prev_offset;
        }

        // Animate movement of other columns.
        let offset = self.column_x(col_idx + 1) - self.column_x(col_idx);
        let config = anim_config.unwrap_or(self.options.animations.window_movement.0);
        if self.active_column_idx <= col_idx {
            for col in &mut self.columns[col_idx + 1..] {
                col.animate_move_from_with_config(-offset, config);
            }
        } else {
            for col in &mut self.columns[..col_idx] {
                col.animate_move_from_with_config(offset, config);
            }
        }
    }

    pub fn add_window(
        &mut self,
        window: W,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        let col_idx = if self.columns.is_empty() {
            0
        } else {
            self.active_column_idx + 1
        };

        self.add_window_at(col_idx, window, activate, width, is_full_width);
    }

    fn add_tile(
        &mut self,
        tile: Tile<W>,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
        anim_config: Option<niri_config::Animation>,
    ) {
        let col_idx = if self.columns.is_empty() {
            0
        } else {
            self.active_column_idx + 1
        };

        self.add_tile_at(col_idx, tile, activate, width, is_full_width, anim_config);
    }

    pub fn add_window_right_of(
        &mut self,
        right_of: &W::Id,
        window: W,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        self.enter_output_for_window(&window);

        let right_of_idx = self
            .columns
            .iter()
            .position(|col| col.contains(right_of))
            .unwrap();
        let idx = right_of_idx + 1;

        let column = Column::new(
            window,
            self.view_size,
            self.working_area,
            self.options.clone(),
            width,
            is_full_width,
            true,
        );
        self.columns.insert(idx, column);

        // Activate the new window if right_of was active.
        if self.active_column_idx == right_of_idx {
            let prev_offset = self.static_view_offset();
            self.activate_column(idx);
            self.activate_prev_column_on_removal = Some(prev_offset);
        } else if idx <= self.active_column_idx {
            self.active_column_idx += 1;
        }

        // Animate movement of other columns.
        let offset = self.column_x(idx + 1) - self.column_x(idx);
        if self.active_column_idx <= idx {
            for col in &mut self.columns[idx + 1..] {
                col.animate_move_from(-offset);
            }
        } else {
            for col in &mut self.columns[..idx] {
                col.animate_move_from(offset);
            }
        }
    }

    pub fn add_column(&mut self, mut column: Column<W>, activate: bool) {
        for tile in &column.tiles {
            self.enter_output_for_window(tile.window());
        }

        let was_empty = self.columns.is_empty();

        let idx = if self.columns.is_empty() {
            0
        } else {
            self.active_column_idx + 1
        };

        column.set_view_size(self.view_size, self.working_area);
        let width = column.width();
        self.columns.insert(idx, column);

        if activate {
            // If this is the first window on an empty workspace, skip the animation from whatever
            // view_offset was left over.
            if was_empty {
                if self.options.center_focused_column == CenterFocusedColumn::Always {
                    self.view_offset =
                        -(self.working_area.size.w - width) / 2 - self.working_area.loc.x;
                } else {
                    // Try to make the code produce a left-aligned offset, even in presence of left
                    // exclusive zones.
                    self.view_offset = self.compute_new_view_offset_for_column(self.column_x(0), 0);
                }
                self.view_offset_adj = None;
            }

            let prev_offset = (!was_empty).then(|| self.static_view_offset());

            self.activate_column(idx);
            self.activate_prev_column_on_removal = prev_offset;
        }

        // Animate movement of other columns.
        let offset = self.column_x(idx + 1) - self.column_x(idx);
        if self.active_column_idx <= idx {
            for col in &mut self.columns[idx + 1..] {
                col.animate_move_from(-offset);
            }
        } else {
            for col in &mut self.columns[..idx] {
                col.animate_move_from(offset);
            }
        }
    }

    pub fn remove_tile_by_idx(
        &mut self,
        column_idx: usize,
        window_idx: usize,
        anim_config: Option<niri_config::Animation>,
    ) -> Tile<W> {
        let offset = self.column_x(column_idx + 1) - self.column_x(column_idx);

        let column = &mut self.columns[column_idx];

        // Animate movement of other tiles.
        let offset_y = column.tile_y(window_idx + 1) - column.tile_y(window_idx);
        for tile in &mut column.tiles[window_idx + 1..] {
            tile.animate_move_y_from(offset_y);
        }

        let tile = column.tiles.remove(window_idx);
        column.heights.remove(window_idx);

        if let Some(output) = &self.output {
            tile.window().output_leave(output);
        }

        if column.tiles.is_empty() {
            if column_idx + 1 == self.active_column_idx {
                // The previous column, that we were going to activate upon removal of the active
                // column, has just been itself removed.
                self.activate_prev_column_on_removal = None;
            }

            if column_idx == self.active_column_idx {
                self.view_offset_before_fullscreen = None;
            }

            // Animate movement of the other columns.
            let movement_config = anim_config.unwrap_or(self.options.animations.window_movement.0);
            if self.active_column_idx <= column_idx {
                for col in &mut self.columns[column_idx + 1..] {
                    col.animate_move_from_with_config(offset, movement_config);
                }
            } else {
                for col in &mut self.columns[..column_idx] {
                    col.animate_move_from_with_config(-offset, movement_config);
                }
            }

            self.columns.remove(column_idx);
            if self.columns.is_empty() {
                return tile;
            }

            let view_config =
                anim_config.unwrap_or(self.options.animations.horizontal_view_movement.0);

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
                    let current_x = self.view_pos();
                    self.animate_view_offset_with_config(
                        current_x,
                        self.active_column_idx,
                        prev_offset,
                        view_config,
                    );
                    self.animate_view_offset_to_column_with_config(
                        current_x,
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

            return tile;
        }

        column.active_tile_idx = min(column.active_tile_idx, column.tiles.len() - 1);
        column.update_tile_sizes(true);

        tile
    }

    pub fn remove_column_by_idx(&mut self, column_idx: usize) -> Column<W> {
        // Animate movement of the other columns.
        let offset = self.column_x(column_idx + 1) - self.column_x(column_idx);
        if self.active_column_idx <= column_idx {
            for col in &mut self.columns[column_idx + 1..] {
                col.animate_move_from(offset);
            }
        } else {
            for col in &mut self.columns[..column_idx] {
                col.animate_move_from(-offset);
            }
        }

        let column = self.columns.remove(column_idx);

        if let Some(output) = &self.output {
            for tile in &column.tiles {
                tile.window().output_leave(output);
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

                self.activate_column(self.active_column_idx - 1);

                // Restore the view offset but make sure to scroll the view in case the
                // previous window had resized.
                let current_x = self.view_pos();
                self.animate_view_offset(current_x, self.active_column_idx, prev_offset);
                self.animate_view_offset_to_column(current_x, self.active_column_idx, None);
            }
        } else {
            self.activate_column(min(self.active_column_idx, self.columns.len() - 1));
        }

        column
    }

    pub fn remove_window(&mut self, window: &W::Id) -> W {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();
        let column = &self.columns[column_idx];

        let window_idx = column.position(window).unwrap();
        self.remove_tile_by_idx(column_idx, window_idx, None)
            .into_window()
    }

    pub fn update_window(&mut self, window: &W::Id) {
        let (col_idx, column) = self
            .columns
            .iter_mut()
            .enumerate()
            .find(|(_, col)| col.contains(window))
            .unwrap();
        let tile_idx = column
            .tiles
            .iter()
            .position(|tile| tile.window().id() == window)
            .unwrap();

        let offset = column
            .width_before_resize
            .take()
            .map_or(0, |prev| prev - column.width());

        let was_fullscreen = column.tiles[tile_idx].is_fullscreen();

        column.update_window(window);
        column.update_tile_sizes(false);

        // Move other columns in tandem with resizing.
        let started_resize_anim =
            column.tiles[tile_idx].resize_animation().is_some() && offset != 0;
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

        if col_idx == self.active_column_idx
            && !matches!(self.view_offset_adj, Some(ViewOffsetAdjustment::Gesture(_)))
        {
            // We might need to move the view to ensure the resized window is still visible.
            let current_x = self.view_pos();

            // Upon unfullscreening, restore the view offset.
            let is_fullscreen = self.columns[col_idx].tiles[tile_idx].is_fullscreen();
            if was_fullscreen && !is_fullscreen {
                if let Some(prev_offset) = self.view_offset_before_fullscreen.take() {
                    self.animate_view_offset(current_x, col_idx, prev_offset);
                }
            }

            // Synchronize the horizontal view movement with the resize so that it looks nice. This
            // is especially important for always-centered view.
            let config = if started_resize_anim {
                self.options.animations.window_resize.anim
            } else {
                self.options.animations.horizontal_view_movement.0
            };

            // FIXME: we will want to skip the animation in some cases here to make continuously
            // resizing windows not look janky.
            self.animate_view_offset_to_column_with_config(current_x, col_idx, None, config);
        }
    }

    pub fn activate_window(&mut self, window: &W::Id) {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();
        let column = &mut self.columns[column_idx];

        column.activate_window(window);
        self.activate_column(column_idx);
    }

    pub fn start_close_animation_for_window(
        &mut self,
        renderer: &mut GlesRenderer,
        window: &W::Id,
    ) {
        let (tile, mut tile_pos) = self
            .tiles_in_render_order()
            .find(|(tile, _)| tile.window().id() == window)
            .unwrap();

        // FIXME: workspaces should probably cache their last used scale so they can be correctly
        // rendered even with no outputs connected.
        let output_scale = self
            .output
            .as_ref()
            .map(|o| Scale::from(o.current_scale().fractional_scale()))
            .unwrap_or(Scale::from(1.));

        let Some(snapshot) =
            tile.take_snapshot_for_close_anim(renderer, output_scale, self.view_size)
        else {
            return;
        };

        let col_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();

        let col = &self.columns[col_idx];
        let removing_last = col.tiles.len() == 1;
        let offset = self.column_x(col_idx + 1) - self.column_x(col_idx);

        let mut center = Point::from((0, 0));
        center.x += tile.tile_size().w / 2;
        center.y += tile.tile_size().h / 2;

        tile_pos.x += self.view_pos();

        if col_idx < self.active_column_idx && removing_last {
            tile_pos.x -= offset;
        }

        // FIXME: this is a bit cursed since it's relying on Tile's internal details.
        let (starting_alpha, starting_scale) = if let Some(anim) = tile.open_animation() {
            (
                anim.clamped_value().clamp(0., 1.) as f32,
                (anim.value() / 2. + 0.5).max(0.),
            )
        } else {
            (1., 1.)
        };

        let anim = Animation::new(1., 0., 0., self.options.animations.window_close.0);

        let res = ClosingWindow::new(
            renderer,
            snapshot,
            output_scale.x as i32,
            center,
            tile_pos,
            anim,
            starting_alpha,
            starting_scale,
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

    pub fn prepare_for_resize_animation(&mut self, window: &W::Id) {
        let column = self
            .columns
            .iter_mut()
            .find(|col| col.contains(window))
            .unwrap();

        column.width_before_resize = Some(column.width());
    }

    #[cfg(test)]
    pub fn verify_invariants(&self) {
        assert!(self.view_size.w > 0);
        assert!(self.view_size.h > 0);

        if !self.columns.is_empty() {
            assert!(self.active_column_idx < self.columns.len());

            for column in &self.columns {
                assert!(Rc::ptr_eq(&self.options, &column.options));
                column.verify_invariants();
            }

            // When we have an unfullscreen view offset stored, the active column should have a
            // fullscreen tile.
            if self.view_offset_before_fullscreen.is_some() {
                let col = &self.columns[self.active_column_idx];
                assert!(
                    col.is_fullscreen
                        || col.tiles.iter().any(|tile| {
                            tile.is_fullscreen() || tile.window().is_pending_fullscreen()
                        })
                );
            }
        }
    }

    pub fn focus_left(&mut self) {
        self.activate_column(self.active_column_idx.saturating_sub(1));
    }

    pub fn focus_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.activate_column(min(self.active_column_idx + 1, self.columns.len() - 1));
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

    pub fn focus_down(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].focus_down();
    }

    pub fn focus_up(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].focus_up();
    }

    fn move_column_to(&mut self, new_idx: usize) {
        if self.active_column_idx == new_idx {
            return;
        }

        let current_col_x = self.column_x(self.active_column_idx);
        let next_col_x = self.column_x(self.active_column_idx + 1);

        let column = self.columns.remove(self.active_column_idx);
        self.columns.insert(new_idx, column);

        // Preserve the camera position when moving to the left.
        let view_offset_delta = -self.column_x(self.active_column_idx) + current_col_x;
        self.view_offset += view_offset_delta;
        if let Some(ViewOffsetAdjustment::Animation(anim)) = &mut self.view_offset_adj {
            anim.offset(view_offset_delta as f64);
        }

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

    pub fn move_left(&mut self) {
        let new_idx = self.active_column_idx.saturating_sub(1);
        self.move_column_to(new_idx);
    }

    pub fn move_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let new_idx = min(self.active_column_idx + 1, self.columns.len() - 1);
        self.move_column_to(new_idx);
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

    pub fn move_down(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].move_down();
    }

    pub fn move_up(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].move_up();
    }

    pub fn consume_or_expel_window_left(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let source_col_idx = self.active_column_idx;
        let source_column = &self.columns[source_col_idx];
        let prev_y = source_column.tile_y(source_column.active_tile_idx);

        if source_column.tiles.len() == 1 {
            if self.active_column_idx == 0 {
                return;
            }

            let offset_x = self.column_x(source_col_idx) - self.column_x(source_col_idx - 1);

            // Move into adjacent column.
            let target_column_idx = source_col_idx - 1;

            // Make sure the previous (target) column is activated so the animation looks right.
            self.activate_prev_column_on_removal = Some(self.static_view_offset() + offset_x);
            let offset_x = offset_x + self.columns[source_col_idx].render_offset().x;
            let tile = self.remove_tile_by_idx(
                source_col_idx,
                0,
                Some(self.options.animations.window_movement.0),
            );
            self.enter_output_for_window(tile.window());

            let next_col_idx = source_col_idx;
            let prev_next_x = self.column_x(next_col_idx);

            let target_column = &mut self.columns[target_column_idx];
            let offset_x = offset_x - target_column.render_offset().x;
            let offset_y = prev_y - target_column.tile_y(target_column.tiles.len());

            target_column.add_tile(tile, true);
            target_column.focus_last();

            let new_tile = target_column.tiles.last_mut().unwrap();
            new_tile.animate_move_from(Point::from((offset_x, offset_y)));

            // Consuming a window into a column could've increased its width if the new window had a
            // larger min width. Move the next columns to account for this.
            let offset_next = prev_next_x - self.column_x(next_col_idx);
            for col in &mut self.columns[next_col_idx..] {
                col.animate_move_from(offset_next);
            }
        } else {
            // Move out of column.
            let width = source_column.width;
            let is_full_width = source_column.is_full_width;

            let offset_x = source_column.render_offset().x;

            let tile = self.remove_tile_by_idx(source_col_idx, source_column.active_tile_idx, None);

            self.add_tile_at(
                self.active_column_idx,
                tile,
                true,
                width,
                is_full_width,
                Some(self.options.animations.window_movement.0),
            );

            // We added to the left, don't activate even further left on removal.
            self.activate_prev_column_on_removal = None;

            let new_col = &mut self.columns[self.active_column_idx];
            let offset_y = prev_y - new_col.tile_y(0);
            new_col.tiles[0].animate_move_from(Point::from((offset_x, offset_y)));
        }
    }

    pub fn consume_or_expel_window_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let source_col_idx = self.active_column_idx;
        let offset_x = self.column_x(source_col_idx) - self.column_x(source_col_idx + 1);

        let source_column = &self.columns[source_col_idx];
        let offset_x = offset_x + source_column.render_offset().x;
        let prev_y = source_column.tile_y(source_column.active_tile_idx);

        if source_column.tiles.len() == 1 {
            if self.active_column_idx + 1 == self.columns.len() {
                return;
            }

            // Move into adjacent column.
            let target_column_idx = source_col_idx;

            let offset_x = offset_x - self.columns[source_col_idx + 1].render_offset().x;

            // Make sure the target column gets activated.
            self.activate_prev_column_on_removal = None;
            let tile = self.remove_tile_by_idx(
                source_col_idx,
                0,
                Some(self.options.animations.window_movement.0),
            );
            self.enter_output_for_window(tile.window());

            let prev_next_x = self.column_x(target_column_idx + 1);

            let target_column = &mut self.columns[target_column_idx];
            let offset_y = prev_y - target_column.tile_y(target_column.tiles.len());

            target_column.add_tile(tile, true);
            target_column.focus_last();

            let new_tile = target_column.tiles.last_mut().unwrap();
            new_tile.animate_move_from(Point::from((offset_x, offset_y)));

            // Consuming a window into a column could've increased its width if the new window had a
            // larger min width. Move the next columns to account for this.
            let offset_next = prev_next_x - self.column_x(target_column_idx + 1);
            for col in &mut self.columns[target_column_idx + 1..] {
                col.animate_move_from(offset_next);
            }
        } else {
            // Move out of column.
            let width = source_column.width;
            let is_full_width = source_column.is_full_width;

            let tile = self.remove_tile_by_idx(source_col_idx, source_column.active_tile_idx, None);

            self.add_tile(
                tile,
                true,
                width,
                is_full_width,
                Some(self.options.animations.window_movement.0),
            );

            let new_col = &mut self.columns[self.active_column_idx];
            let offset_y = prev_y - new_col.tile_y(0);
            new_col.tiles[0].animate_move_from(Point::from((offset_x, offset_y)));
        }
    }

    pub fn consume_into_column(&mut self) {
        if self.columns.len() < 2 {
            return;
        }

        if self.active_column_idx == self.columns.len() - 1 {
            return;
        }

        let source_column_idx = self.active_column_idx + 1;

        let offset_x = self.column_x(source_column_idx)
            + self.columns[source_column_idx].render_offset().x
            - self.column_x(self.active_column_idx);
        let prev_y = self.columns[source_column_idx].tile_y(0);

        let tile = self.remove_tile_by_idx(source_column_idx, 0, None);
        self.enter_output_for_window(tile.window());

        let prev_next_x = self.column_x(self.active_column_idx + 1);

        let target_column = &mut self.columns[self.active_column_idx];
        let was_fullscreen = target_column.tiles[target_column.active_tile_idx].is_fullscreen();
        let offset_y = prev_y - target_column.tile_y(target_column.tiles.len());

        target_column.add_tile(tile, true);

        if !was_fullscreen {
            self.view_offset_before_fullscreen = None;
        }

        let offset_x = offset_x - target_column.render_offset().x;

        let new_tile = target_column.tiles.last_mut().unwrap();
        new_tile.animate_move_from(Point::from((offset_x, offset_y)));

        // Consuming a window into a column could've increased its width if the new window had a
        // larger min width. Move the next columns to account for this.
        let offset_next = prev_next_x - self.column_x(self.active_column_idx + 1);
        for col in &mut self.columns[self.active_column_idx + 1..] {
            col.animate_move_from(offset_next);
        }
    }

    pub fn expel_from_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let offset_x =
            self.column_x(self.active_column_idx) - self.column_x(self.active_column_idx + 1);

        let source_column = &self.columns[self.active_column_idx];
        if source_column.tiles.len() == 1 {
            return;
        }

        let offset_x = offset_x + source_column.render_offset().x;

        let prev_y = source_column.tile_y(source_column.active_tile_idx);

        let width = source_column.width;
        let is_full_width = source_column.is_full_width;
        let tile =
            self.remove_tile_by_idx(self.active_column_idx, source_column.active_tile_idx, None);

        self.add_tile(
            tile,
            true,
            width,
            is_full_width,
            Some(self.options.animations.window_movement.0),
        );

        let new_col = &mut self.columns[self.active_column_idx];
        let offset_y = prev_y - new_col.tile_y(0);
        new_col.tiles[0].animate_move_from(Point::from((offset_x, offset_y)));
    }

    pub fn center_column(&mut self) {
        let center_x = self.view_pos();
        self.animate_view_offset_to_column_centered(
            center_x,
            self.active_column_idx,
            self.options.animations.horizontal_view_movement.0,
        );
    }

    fn view_pos(&self) -> i32 {
        self.column_x(self.active_column_idx) + self.view_offset
    }

    /// Returns a view offset value suitable for saving and later restoration.
    ///
    /// This means that it shouldn't return an in-progress animation or gesture value.
    fn static_view_offset(&self) -> i32 {
        match &self.view_offset_adj {
            // For animations we can return the final value.
            Some(ViewOffsetAdjustment::Animation(anim)) => anim.to().round() as i32,
            Some(ViewOffsetAdjustment::Gesture(gesture)) => gesture.static_view_offset,
            _ => self.view_offset,
        }
    }

    fn tiles_in_render_order(&self) -> impl Iterator<Item = (&'_ Tile<W>, Point<i32, Logical>)> {
        let view_pos = self.view_pos();

        // Start with the active window since it's drawn on top.
        let col = &self.columns[self.active_column_idx];
        let tile = &col.tiles[col.active_tile_idx];
        let tile_pos = Point::from((-self.view_offset, col.tile_y(col.active_tile_idx)))
            + col.render_offset()
            + tile.render_offset();
        let first = iter::once((tile, tile_pos));

        // Next, the rest of the tiles in the active column, since it should be drawn on top as a
        // whole during animations.
        let next =
            zip(&col.tiles, col.tile_ys())
                .enumerate()
                .filter_map(move |(tile_idx, (tile, y))| {
                    if tile_idx == col.active_tile_idx {
                        // Active tile comes first.
                        return None;
                    }

                    let tile_pos = Point::from((-self.view_offset, y))
                        + col.render_offset()
                        + tile.render_offset();
                    Some((tile, tile_pos))
                });

        let mut x = -view_pos;
        let rest = self
            .columns
            .iter()
            .enumerate()
            // Keep track of column X position.
            .map(move |(col_idx, col)| {
                let rv = (col_idx, col, x);
                x += col.width() + self.options.gaps;
                rv
            })
            .filter_map(|(col_idx, col, x)| {
                if col_idx == self.active_column_idx {
                    // Active column comes before.
                    return None;
                }

                Some((col, x))
            })
            .flat_map(move |(col, x)| {
                zip(&col.tiles, col.tile_ys()).map(move |(tile, y)| {
                    let tile_pos = Point::from((x, y)) + col.render_offset() + tile.render_offset();
                    (tile, tile_pos)
                })
            });

        first.chain(next).chain(rest)
    }

    fn active_column_ref(&self) -> Option<&Column<W>> {
        if self.columns.is_empty() {
            return None;
        }
        Some(&self.columns[self.active_column_idx])
    }

    /// Returns the geometry of the active tile relative to and clamped to the view.
    ///
    /// During animations, assumes the final view position.
    pub fn active_tile_visual_rectangle(&self) -> Option<Rectangle<i32, Logical>> {
        let col = self.active_column_ref()?;
        let view_pos = self
            .view_offset_adj
            .as_ref()
            .map_or(self.view_offset, |adj| adj.target_view_offset() as i32);

        let tile_pos = Point::from((-view_pos, col.tile_y(col.active_tile_idx)));
        let tile_size = col.active_tile_ref().tile_size();
        let tile_rect = Rectangle::from_loc_and_size(tile_pos, tile_size);

        let view = Rectangle::from_loc_and_size((0, 0), self.view_size);
        view.intersection(tile_rect)
    }

    pub fn window_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(&W, Option<Point<i32, Logical>>)> {
        if self.columns.is_empty() {
            return None;
        }

        self.tiles_in_render_order().find_map(|(tile, tile_pos)| {
            let pos_within_tile = pos - tile_pos.to_f64();

            if tile.is_in_input_region(pos_within_tile) {
                let pos_within_surface = tile_pos + tile.buf_loc();
                return Some((tile.window(), Some(pos_within_surface)));
            } else if tile.is_in_activation_region(pos_within_tile) {
                return Some((tile.window(), None));
            }

            None
        })
    }

    pub fn toggle_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].toggle_width();
    }

    pub fn toggle_full_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].toggle_full_width();
    }

    pub fn set_column_width(&mut self, change: SizeChange) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].set_column_width(change);
    }

    pub fn set_window_height(&mut self, change: SizeChange) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].set_window_height(change);
    }

    pub fn set_fullscreen(&mut self, window: &W::Id, is_fullscreen: bool) {
        let (mut col_idx, tile_idx) = self
            .columns
            .iter()
            .enumerate()
            .find_map(|(col_idx, col)| col.position(window).map(|tile_idx| (col_idx, tile_idx)))
            .unwrap();

        if is_fullscreen
            && col_idx == self.active_column_idx
            && self.columns[col_idx].tiles.len() == 1
        {
            self.view_offset_before_fullscreen = Some(self.static_view_offset());
        }

        let mut col = &mut self.columns[col_idx];

        if is_fullscreen && col.tiles.len() > 1 {
            // This wasn't the only window in its column; extract it into a separate column.
            let target_window_was_focused =
                self.active_column_idx == col_idx && col.active_tile_idx == tile_idx;
            let window = col.tiles.remove(tile_idx).into_window();
            col.heights.remove(tile_idx);
            col.active_tile_idx = min(col.active_tile_idx, col.tiles.len() - 1);
            col.update_tile_sizes(false);
            let width = col.width;
            let is_full_width = col.is_full_width;

            col_idx += 1;
            self.columns.insert(
                col_idx,
                Column::new(
                    window,
                    self.view_size,
                    self.working_area,
                    self.options.clone(),
                    width,
                    is_full_width,
                    false,
                ),
            );

            if target_window_was_focused {
                self.activate_column(col_idx);
                self.view_offset_before_fullscreen = Some(self.static_view_offset());
            } else if self.active_column_idx >= col_idx {
                self.active_column_idx += 1;
            }

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
    }

    pub fn toggle_fullscreen(&mut self, window: &W::Id) {
        let col = self
            .columns
            .iter_mut()
            .find(|col| col.contains(window))
            .unwrap();
        let value = !col.is_fullscreen;
        self.set_fullscreen(window, value);
    }

    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer if we're on a fullscreen window and the view is stationary.
        if self.columns.is_empty() {
            return false;
        }

        if self.view_offset_adj.is_some() {
            return false;
        }

        self.columns[self.active_column_idx].is_fullscreen
    }

    pub fn render_elements<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        target: RenderTarget,
    ) -> Vec<WorkspaceRenderElement<R>> {
        // FIXME: workspaces should probably cache their last used scale so they can be correctly
        // rendered even with no outputs connected.
        let output_scale = self
            .output
            .as_ref()
            .map(|o| Scale::from(o.current_scale().fractional_scale()))
            .unwrap_or(Scale::from(1.));

        let mut rv = vec![];

        // Draw the closing windows on top.
        let view_pos = self.view_pos();
        for closing in &self.closing_windows {
            rv.push(closing.render(view_pos, output_scale, target).into());
        }

        if self.columns.is_empty() {
            return rv;
        }

        let mut first = true;
        for (tile, tile_pos) in self.tiles_in_render_order() {
            // For the active tile (which comes first), draw the focus ring.
            let focus_ring = first;
            first = false;

            rv.extend(
                tile.render(
                    renderer,
                    tile_pos,
                    output_scale,
                    self.view_size,
                    focus_ring,
                    target,
                )
                .map(Into::into),
            );
        }

        rv
    }

    pub fn view_offset_gesture_begin(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let gesture = ViewGesture {
            current_view_offset: self.view_offset as f64,
            tracker: SwipeTracker::new(),
            delta_from_tracker: self.view_offset as f64,
            static_view_offset: self.static_view_offset(),
        };
        self.view_offset_adj = Some(ViewOffsetAdjustment::Gesture(gesture));
    }

    pub fn view_offset_gesture_update(
        &mut self,
        delta_x: f64,
        timestamp: Duration,
    ) -> Option<bool> {
        let Some(ViewOffsetAdjustment::Gesture(gesture)) = &mut self.view_offset_adj else {
            return None;
        };

        gesture.tracker.push(delta_x, timestamp);

        let norm_factor = self.working_area.size.w as f64 / VIEW_GESTURE_WORKING_AREA_MOVEMENT;
        let pos = gesture.tracker.pos() * norm_factor;
        let view_offset = pos + gesture.delta_from_tracker;
        gesture.current_view_offset = view_offset;

        Some(true)
    }

    pub fn view_offset_gesture_end(&mut self, _cancelled: bool) -> bool {
        let Some(ViewOffsetAdjustment::Gesture(gesture)) = &self.view_offset_adj else {
            return false;
        };

        // We do not handle cancelling, just like GNOME Shell doesn't. For this gesture, proper
        // cancelling would require keeping track of the original active column, and then updating
        // it in all the right places (adding columns, removing columns, etc.) -- quite a bit of
        // effort and bug potential.

        let norm_factor = self.working_area.size.w as f64 / VIEW_GESTURE_WORKING_AREA_MOVEMENT;
        let velocity = gesture.tracker.velocity() * norm_factor;
        let pos = gesture.tracker.pos() * norm_factor;
        let current_view_offset = pos + gesture.delta_from_tracker;

        if self.columns.is_empty() {
            self.view_offset = current_view_offset.round() as i32;
            self.view_offset_adj = None;
            return true;
        }

        // Figure out where the gesture would stop after deceleration.
        let end_pos = gesture.tracker.projected_end_pos() * norm_factor;
        let target_view_offset = end_pos + gesture.delta_from_tracker;

        // Compute the snapping points. These are where the view aligns with column boundaries on
        // either side.
        struct Snap {
            // View position relative to x = 0 (the first column).
            view_pos: i32,
            // Column to activate for this snapping point.
            col_idx: usize,
        }

        let mut snapping_points = Vec::new();

        let left_strut = self.working_area.loc.x;
        let right_strut = self.view_size.w - self.working_area.size.w - self.working_area.loc.x;

        if self.options.center_focused_column == CenterFocusedColumn::Always {
            let mut col_x = 0;
            for (col_idx, col) in self.columns.iter().enumerate() {
                let col_w = col.width();

                let view_pos = if col.is_fullscreen {
                    col_x
                } else if self.working_area.size.w <= col_w {
                    col_x - left_strut
                } else {
                    col_x - (self.working_area.size.w - col_w) / 2 - left_strut
                };
                snapping_points.push(Snap { view_pos, col_idx });

                col_x += col_w + self.options.gaps;
            }
        } else {
            let view_width = self.view_size.w;
            let mut push = |col_idx, left, right| {
                snapping_points.push(Snap {
                    view_pos: left,
                    col_idx,
                });
                snapping_points.push(Snap {
                    view_pos: right - view_width,
                    col_idx,
                });
            };

            let mut col_x = 0;
            for (col_idx, col) in self.columns.iter().enumerate() {
                let col_w = col.width();

                // Normal columns align with the working area, but fullscreen columns align with the
                // view size.
                if col.is_fullscreen {
                    let left = col_x;
                    let right = col_x + col_w;
                    push(col_idx, left, right);
                } else {
                    // Logic from compute_new_view_offset.
                    let padding =
                        ((self.working_area.size.w - col_w) / 2).clamp(0, self.options.gaps);
                    let left = col_x - padding - left_strut;
                    let right = col_x + col_w + padding + right_strut;
                    push(col_idx, left, right);
                }

                col_x += col_w + self.options.gaps;
            }
        }

        // Find the closest snapping point.
        snapping_points.sort_by_key(|snap| snap.view_pos);

        let active_col_x = self.column_x(self.active_column_idx);
        let target_view_pos = (active_col_x as f64 + target_view_offset).round() as i32;
        let target_snap = snapping_points
            .iter()
            .min_by_key(|snap| snap.view_pos.abs_diff(target_view_pos))
            .unwrap();

        let mut new_col_idx = target_snap.col_idx;

        if self.options.center_focused_column != CenterFocusedColumn::Always {
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
                            ((self.working_area.size.w - col_w) / 2).clamp(0, self.options.gaps);
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
                            ((self.working_area.size.w - col_w) / 2).clamp(0, self.options.gaps);
                        if col_x - padding < target_snap.view_pos + left_strut {
                            break;
                        }
                    }

                    new_col_idx = col_idx;
                }
            }
        }

        let new_col_x = self.column_x(new_col_idx);
        let delta = (active_col_x - new_col_x) as f64;
        self.view_offset = (current_view_offset + delta).round() as i32;

        if self.active_column_idx != new_col_idx {
            self.view_offset_before_fullscreen = None;
        }

        self.active_column_idx = new_col_idx;

        let target_view_offset = target_snap.view_pos - new_col_x;

        self.view_offset_adj = Some(ViewOffsetAdjustment::Animation(Animation::new(
            current_view_offset + delta,
            target_view_offset as f64,
            velocity,
            self.options.animations.horizontal_view_movement.0,
        )));

        // HACK: deal with things like snapping to the right edge of a larger-than-view window.
        self.animate_view_offset_to_column(self.view_pos(), new_col_idx, None);

        true
    }

    pub fn refresh(&mut self, is_active: bool) {
        for (col_idx, col) in self.columns.iter_mut().enumerate() {
            for (tile_idx, tile) in col.tiles.iter_mut().enumerate() {
                let win = tile.window_mut();
                let active = is_active
                    && self.active_column_idx == col_idx
                    && col.active_tile_idx == tile_idx;
                win.set_activated(active);

                let border_config = win.rules().border.resolve_against(self.options.border);
                let bounds = compute_toplevel_bounds(
                    border_config,
                    self.working_area.size,
                    self.options.gaps,
                );
                win.set_bounds(bounds);

                win.send_pending_configure();
                win.refresh();
            }
        }
    }
}

impl<W: LayoutElement> Column<W> {
    fn new(
        window: W,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
        options: Rc<Options>,
        width: ColumnWidth,
        is_full_width: bool,
        animate_resize: bool,
    ) -> Self {
        let tile = Tile::new(window, options.clone());
        Self::new_with_tile(
            tile,
            view_size,
            working_area,
            options,
            width,
            is_full_width,
            animate_resize,
        )
    }

    fn new_with_tile(
        tile: Tile<W>,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
        options: Rc<Options>,
        width: ColumnWidth,
        is_full_width: bool,
        animate_resize: bool,
    ) -> Self {
        let mut rv = Self {
            tiles: vec![],
            heights: vec![],
            active_tile_idx: 0,
            width,
            is_full_width,
            is_fullscreen: false,
            move_animation: None,
            width_before_resize: None,
            view_size,
            working_area,
            options,
        };

        let is_pending_fullscreen = tile.window().is_pending_fullscreen();

        rv.add_tile(tile, animate_resize);

        if is_pending_fullscreen {
            rv.set_fullscreen(true);
        }

        rv
    }

    fn set_view_size(&mut self, size: Size<i32, Logical>, working_area: Rectangle<i32, Logical>) {
        if self.view_size == size && self.working_area == working_area {
            return;
        }

        self.view_size = size;
        self.working_area = working_area;

        self.update_tile_sizes(false);
    }

    fn update_config(&mut self, options: Rc<Options>) {
        let mut update_sizes = false;

        // If preset widths changed, make our width non-preset.
        if self.options.preset_widths != options.preset_widths {
            if let ColumnWidth::Preset(idx) = self.width {
                self.width = self.options.preset_widths[idx];
            }
        }

        if self.options.gaps != options.gaps {
            update_sizes = true;
        }

        if self.options.border.off != options.border.off
            || self.options.border.width != options.border.width
        {
            update_sizes = true;
        }

        for tile in &mut self.tiles {
            tile.update_config(options.clone());
        }

        self.options = options;

        if update_sizes {
            self.update_tile_sizes(false);
        }
    }

    fn set_width(&mut self, width: ColumnWidth) {
        self.width = width;
        self.is_full_width = false;
        self.update_tile_sizes(true);
    }

    pub fn advance_animations(&mut self, current_time: Duration, is_active: bool) {
        match &mut self.move_animation {
            Some(anim) => {
                anim.set_current_time(current_time);
                if anim.is_done() {
                    self.move_animation = None;
                }
            }
            None => (),
        }

        for (tile_idx, tile) in self.tiles.iter_mut().enumerate() {
            let is_active = is_active && tile_idx == self.active_tile_idx;
            tile.advance_animations(current_time, is_active);
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.move_animation.is_some() || self.tiles.iter().any(Tile::are_animations_ongoing)
    }

    pub fn render_offset(&self) -> Point<i32, Logical> {
        let mut offset = Point::from((0., 0.));

        if let Some(anim) = &self.move_animation {
            offset.x += anim.value();
        }

        offset.to_i32_round()
    }

    pub fn animate_move_from(&mut self, from_x_offset: i32) {
        self.animate_move_from_with_config(
            from_x_offset,
            self.options.animations.window_movement.0,
        );
    }

    pub fn animate_move_from_with_config(
        &mut self,
        from_x_offset: i32,
        config: niri_config::Animation,
    ) {
        let current_offset = self.move_animation.as_ref().map_or(0., Animation::value);

        self.move_animation = Some(Animation::new(
            f64::from(from_x_offset) + current_offset,
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

    fn add_tile(&mut self, tile: Tile<W>, animate: bool) {
        self.is_fullscreen = false;
        self.tiles.push(tile);
        self.heights.push(WindowHeight::Auto);
        self.update_tile_sizes(animate);
    }

    fn update_window(&mut self, window: &W::Id) {
        let (tile_idx, tile) = self
            .tiles
            .iter_mut()
            .enumerate()
            .find(|(_, tile)| tile.window().id() == window)
            .unwrap();

        let height = tile.window().size().h;
        let offset = tile
            .window()
            .animation_snapshot()
            .map_or(0, |from| from.size.h - height);

        tile.update_window();

        // Move windows below in tandem with resizing.
        if tile.resize_animation().is_some() && offset != 0 {
            for tile in &mut self.tiles[tile_idx + 1..] {
                tile.animate_move_y_from_with_config(
                    offset,
                    self.options.animations.window_resize.anim,
                );
            }
        }
    }

    fn update_tile_sizes(&mut self, animate: bool) {
        if self.is_fullscreen {
            self.tiles[0].request_fullscreen(self.view_size);
            return;
        }

        let min_size: Vec<_> = self.tiles.iter().map(Tile::min_size).collect();
        let max_size: Vec<_> = self.tiles.iter().map(Tile::max_size).collect();

        // Compute the column width.
        let min_width = min_size
            .iter()
            .filter_map(|size| {
                let w = size.w;
                if w == 0 {
                    None
                } else {
                    Some(w)
                }
            })
            .max()
            .unwrap_or(1);
        let max_width = max_size
            .iter()
            .filter_map(|size| {
                let w = size.w;
                if w == 0 {
                    None
                } else {
                    Some(w)
                }
            })
            .min()
            .unwrap_or(i32::MAX);
        let max_width = max(max_width, min_width);

        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let width = width.resolve(&self.options, self.working_area.size.w);
        let width = max(min(width, max_width), min_width);

        // Compute the tile heights. Start by converting window heights to tile heights.
        let mut heights = zip(&self.tiles, &self.heights)
            .map(|(tile, height)| match *height {
                WindowHeight::Auto => WindowHeight::Auto,
                WindowHeight::Fixed(height) => {
                    WindowHeight::Fixed(tile.tile_height_for_window_height(height))
                }
            })
            .collect::<Vec<_>>();
        let mut height_left = self.working_area.size.h - self.options.gaps;
        let mut auto_tiles_left = self.tiles.len();

        // Subtract all fixed-height tiles.
        for (h, (min_size, max_size)) in zip(&mut heights, zip(&min_size, &max_size)) {
            // Check if the tile has an exact height constraint.
            if min_size.h > 0 && min_size.h == max_size.h {
                *h = WindowHeight::Fixed(min_size.h);
            }

            if let WindowHeight::Fixed(h) = h {
                if max_size.h > 0 {
                    *h = min(*h, max_size.h);
                }
                if min_size.h > 0 {
                    *h = max(*h, min_size.h);
                }
                *h = max(*h, 1);

                height_left -= *h + self.options.gaps;
                auto_tiles_left -= 1;
            }
        }

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
        while auto_tiles_left > 0 {
            // Compute the current auto height.
            let auto_height = height_left / auto_tiles_left as i32 - self.options.gaps;
            let auto_height = max(auto_height, 1);

            // Integer division above can result in imperfect height distribution. We will make some
            // tiles 1 px taller to account for this.
            let mut ones_left = height_left
                .saturating_sub((auto_height + self.options.gaps) * auto_tiles_left as i32);

            let mut unsatisfied_min = false;
            let mut ones_left_2 = ones_left;
            for (h, min_size) in zip(&mut heights, &min_size) {
                if matches!(h, WindowHeight::Fixed(_)) {
                    continue;
                }

                let mut auto = auto_height;
                if ones_left_2 > 0 {
                    auto += 1;
                    ones_left_2 -= 1;
                }

                // Check if the auto height satisfies the min height.
                if min_size.h > 0 && min_size.h > auto {
                    *h = WindowHeight::Fixed(min_size.h);
                    height_left -= min_size.h + self.options.gaps;
                    auto_tiles_left -= 1;
                    unsatisfied_min = true;
                }
            }

            // If some min height was unsatisfied, then we allocated the tile more than the auto
            // height, which means that the remaining auto tiles now have less height to work
            // with, and the loop must run again.
            if unsatisfied_min {
                continue;
            }

            // All min heights were satisfied, fill them in.
            for h in &mut heights {
                if matches!(h, WindowHeight::Fixed(_)) {
                    continue;
                }

                let mut auto = auto_height;
                if ones_left > 0 {
                    auto += 1;
                    ones_left -= 1;
                }

                *h = WindowHeight::Fixed(auto);
                auto_tiles_left -= 1;
            }

            assert_eq!(auto_tiles_left, 0);
        }

        for (tile, h) in zip(&mut self.tiles, heights) {
            let WindowHeight::Fixed(height) = h else {
                unreachable!()
            };

            let size = Size::from((width, height));
            tile.request_tile_size(size, animate);
        }
    }

    fn width(&self) -> i32 {
        self.tiles
            .iter()
            .map(|tile| tile.tile_size().w)
            .max()
            .unwrap()
    }

    fn focus_up(&mut self) {
        self.active_tile_idx = self.active_tile_idx.saturating_sub(1);
    }

    fn focus_down(&mut self) {
        self.active_tile_idx = min(self.active_tile_idx + 1, self.tiles.len() - 1);
    }

    fn focus_last(&mut self) {
        self.active_tile_idx = self.tiles.len() - 1;
    }

    fn move_up(&mut self) {
        let new_idx = self.active_tile_idx.saturating_sub(1);
        if self.active_tile_idx == new_idx {
            return;
        }

        let mut ys = self.tile_ys().skip(self.active_tile_idx);
        let active_y = ys.next().unwrap();
        let next_y = ys.next().unwrap();
        drop(ys);

        self.tiles.swap(self.active_tile_idx, new_idx);
        self.heights.swap(self.active_tile_idx, new_idx);
        self.active_tile_idx = new_idx;

        // Animate the movement.
        let new_active_y = self.tile_y(new_idx);
        self.tiles[new_idx].animate_move_y_from(active_y - new_active_y);
        self.tiles[new_idx + 1].animate_move_y_from(active_y - next_y);
    }

    fn move_down(&mut self) {
        let new_idx = min(self.active_tile_idx + 1, self.tiles.len() - 1);
        if self.active_tile_idx == new_idx {
            return;
        }

        let mut ys = self.tile_ys().skip(self.active_tile_idx);
        let active_y = ys.next().unwrap();
        let next_y = ys.next().unwrap();
        drop(ys);

        self.tiles.swap(self.active_tile_idx, new_idx);
        self.heights.swap(self.active_tile_idx, new_idx);
        self.active_tile_idx = new_idx;

        // Animate the movement.
        let new_active_y = self.tile_y(new_idx);
        self.tiles[new_idx].animate_move_y_from(active_y - new_active_y);
        self.tiles[new_idx - 1].animate_move_y_from(next_y - active_y);
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        assert!(!self.tiles.is_empty(), "columns can't be empty");
        assert!(self.active_tile_idx < self.tiles.len());
        assert_eq!(self.tiles.len(), self.heights.len());

        if self.is_fullscreen {
            assert_eq!(self.tiles.len(), 1);
        }

        for tile in &self.tiles {
            assert!(Rc::ptr_eq(&self.options, &tile.options));
            assert_eq!(self.is_fullscreen, tile.window().is_pending_fullscreen());
        }
    }

    fn toggle_width(&mut self) {
        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let idx = match width {
            ColumnWidth::Preset(idx) => (idx + 1) % self.options.preset_widths.len(),
            _ => {
                let current = self.width();
                self.options
                    .preset_widths
                    .iter()
                    .position(|prop| {
                        prop.resolve(&self.options, self.working_area.size.w) > current
                    })
                    .unwrap_or(0)
            }
        };
        let width = ColumnWidth::Preset(idx);
        self.set_width(width);
    }

    fn toggle_full_width(&mut self) {
        self.is_full_width = !self.is_full_width;
        self.update_tile_sizes(true);
    }

    fn set_column_width(&mut self, change: SizeChange) {
        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let current_px = width.resolve(&self.options, self.working_area.size.w);

        let current = match width {
            ColumnWidth::Preset(idx) => self.options.preset_widths[idx],
            current => current,
        };

        // FIXME: fix overflows then remove limits.
        const MAX_PX: i32 = 100000;
        const MAX_F: f64 = 10000.;

        let width = match (current, change) {
            (_, SizeChange::SetFixed(fixed)) => {
                // As a special case, setting a fixed column width will compute it in such a way
                // that the active window gets that width. This is the intention behind the ability
                // to set a fixed size.
                let tile = &self.tiles[self.active_tile_idx];
                ColumnWidth::Fixed(tile.tile_width_for_window_width(fixed).clamp(1, MAX_PX))
            }
            (_, SizeChange::SetProportion(proportion)) => {
                ColumnWidth::Proportion((proportion / 100.).clamp(0., MAX_F))
            }
            (_, SizeChange::AdjustFixed(delta)) => {
                let width = current_px.saturating_add(delta).clamp(1, MAX_PX);
                ColumnWidth::Fixed(width)
            }
            (ColumnWidth::Proportion(current), SizeChange::AdjustProportion(delta)) => {
                let proportion = (current + delta / 100.).clamp(0., MAX_F);
                ColumnWidth::Proportion(proportion)
            }
            (ColumnWidth::Fixed(_), SizeChange::AdjustProportion(delta)) => {
                let current = (current_px + self.options.gaps) as f64
                    / (self.working_area.size.w - self.options.gaps) as f64;
                let proportion = (current + delta / 100.).clamp(0., MAX_F);
                ColumnWidth::Proportion(proportion)
            }
            (ColumnWidth::Preset(_), _) => unreachable!(),
        };

        self.set_width(width);
    }

    fn set_window_height(&mut self, change: SizeChange) {
        let current = self.heights[self.active_tile_idx];
        let tile = &self.tiles[self.active_tile_idx];
        let current_window_px = match current {
            WindowHeight::Auto => tile.window_size().h,
            WindowHeight::Fixed(height) => height,
        };
        let current_tile_px = tile.tile_height_for_window_height(current_window_px);
        let current_prop = (current_tile_px + self.options.gaps) as f64
            / (self.working_area.size.h - self.options.gaps) as f64;

        // FIXME: fix overflows then remove limits.
        const MAX_PX: i32 = 100000;

        let mut window_height = match change {
            SizeChange::SetFixed(fixed) => fixed,
            SizeChange::SetProportion(proportion) => {
                let tile_height = ((self.working_area.size.h - self.options.gaps) as f64
                    * proportion
                    - self.options.gaps as f64)
                    .round() as i32;
                tile.window_height_for_tile_height(tile_height)
            }
            SizeChange::AdjustFixed(delta) => current_window_px.saturating_add(delta),
            SizeChange::AdjustProportion(delta) => {
                let proportion = current_prop + delta / 100.;
                let tile_height = ((self.working_area.size.h - self.options.gaps) as f64
                    * proportion
                    - self.options.gaps as f64)
                    .round() as i32;
                tile.window_height_for_tile_height(tile_height)
            }
        };

        // Clamp it against the window height constraints.
        let win = &self.tiles[self.active_tile_idx].window();
        let min_h = win.min_size().h;
        let max_h = win.max_size().h;

        if max_h > 0 {
            window_height = window_height.min(max_h);
        }
        if min_h > 0 {
            window_height = window_height.max(min_h);
        }

        self.heights[self.active_tile_idx] = WindowHeight::Fixed(window_height.clamp(1, MAX_PX));
        self.update_tile_sizes(true);
    }

    fn set_fullscreen(&mut self, is_fullscreen: bool) {
        assert_eq!(self.tiles.len(), 1);
        self.is_fullscreen = is_fullscreen;
        self.update_tile_sizes(false);
    }

    pub fn window_y(&self, tile_idx: usize) -> i32 {
        let (tile, tile_y) = zip(&self.tiles, self.tile_ys()).nth(tile_idx).unwrap();
        tile_y + tile.window_loc().y
    }

    fn tile_y(&self, tile_idx: usize) -> i32 {
        self.tile_ys().nth(tile_idx).unwrap()
    }

    fn tile_ys(&self) -> impl Iterator<Item = i32> + '_ {
        let mut y = 0;

        if !self.is_fullscreen {
            y = self.working_area.loc.y + self.options.gaps;
        }

        let heights = self.tiles.iter().map(|tile| tile.tile_size().h);

        // Chain an arbitrary height to be able to get the Y that the next tile past the end would
        // have.
        heights.chain(iter::once(0)).map(move |h| {
            let pos = y;
            y += h + self.options.gaps;
            pos
        })
    }

    fn active_tile_ref(&self) -> &Tile<W> {
        &self.tiles[self.active_tile_idx]
    }
}

fn compute_new_view_offset(
    cur_x: i32,
    view_width: i32,
    new_col_x: i32,
    new_col_width: i32,
    gaps: i32,
) -> i32 {
    // If the column is wider than the view, always left-align it.
    if view_width <= new_col_width {
        return 0;
    }

    // Compute the padding in case it needs to be smaller due to large tile width.
    let padding = ((view_width - new_col_width) / 2).clamp(0, gaps);

    // Compute the desired new X with padding.
    let new_x = new_col_x - padding;
    let new_right_x = new_col_x + new_col_width + padding;

    // If the column is already fully visible, leave the view as is.
    if cur_x <= new_x && new_right_x <= cur_x + view_width {
        return -(new_col_x - cur_x);
    }

    // Otherwise, prefer the aligment that results in less motion from the current position.
    let dist_to_left = cur_x.abs_diff(new_x);
    let dist_to_right = (cur_x + view_width).abs_diff(new_right_x);
    if dist_to_left <= dist_to_right {
        -padding
    } else {
        -(view_width - padding - new_col_width)
    }
}

fn set_preferred_scale_transform(window: &impl LayoutElement, output: &Output) {
    // FIXME: cache this on the workspace.
    let scale = output.current_scale().integer_scale();
    let transform = output.current_transform();
    window.set_preferred_scale_transform(scale, transform);
}

pub fn compute_working_area(output: &Output, struts: Struts) -> Rectangle<i32, Logical> {
    // Start with the layer-shell non-exclusive zone.
    let mut working_area = layer_map_for_output(output).non_exclusive_zone();

    // Add struts.
    let w = working_area.size.w;
    let h = working_area.size.h;

    working_area.size.w = w
        .saturating_sub(struts.left.into())
        .saturating_sub(struts.right.into());
    working_area.loc.x += struts.left as i32;

    working_area.size.h = h
        .saturating_sub(struts.top.into())
        .saturating_sub(struts.bottom.into());
    working_area.loc.y += struts.top as i32;

    working_area
}

fn compute_toplevel_bounds(
    border_config: niri_config::Border,
    working_area_size: Size<i32, Logical>,
    gaps: i32,
) -> Size<i32, Logical> {
    let mut border = 0;
    if !border_config.off {
        border = border_config.width as i32 * 2;
    }

    Size::from((
        max(working_area_size.w - gaps * 2 - border, 1),
        max(working_area_size.h - gaps * 2 - border, 1),
    ))
}
