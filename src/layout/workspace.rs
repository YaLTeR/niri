use std::cmp::{max, min};
use std::iter::zip;
use std::rc::Rc;
use std::time::Duration;

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::ImportAll;
use smithay::desktop::space::SpaceElement;
use smithay::desktop::{layer_map_for_output, Window};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::render_elements;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use super::focus_ring::{FocusRing, FocusRingRenderElement};
use super::{LayoutElement, Options};
use crate::animation::Animation;
use crate::config::{PresetWidth, SizeChange, Struts};
use crate::utils::output_size;

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

    /// Focus ring buffer and parameters.
    focus_ring: FocusRing,

    /// Offset of the view computed from the active column.
    ///
    /// Any gaps, including left padding from work area left exclusive zone, is handled
    /// with this view offset (rather than added as a constant elsewhere in the code). This allows
    /// for natural handling of fullscreen windows, which must ignore work area padding.
    view_offset: i32,

    /// Animation of the view offset, if one is currently ongoing.
    view_offset_anim: Option<Animation>,

    /// Whether to activate the previous, rather than the next, column upon column removal.
    ///
    /// When a new column is created and removed with no focus changes in-between, it is more
    /// natural to activate the previously-focused column. This variable tracks that.
    ///
    /// Since we only create-and-activate columns immediately to the right of the active column (in
    /// contrast to tabs in Firefox, for example), we can track this as a bool, rather than an
    /// index of the previous column to activate.
    activate_prev_column_on_removal: bool,

    /// Configurable properties of the layout.
    pub options: Rc<Options>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputId(String);

render_elements! {
    #[derive(Debug)]
    pub WorkspaceRenderElement<R> where R: ImportAll;
    Wayland = WaylandSurfaceRenderElement<R>,
    FocusRing = FocusRingRenderElement,
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
    /// Windows in this column.
    ///
    /// Must be non-empty.
    pub windows: Vec<W>,

    /// Heights of the windows.
    ///
    /// Must have the same number of elements as `windows`.
    heights: Vec<WindowHeight>,

    /// Index of the currently active window.
    pub active_window_idx: usize,

    /// Desired width of this column.
    ///
    /// If the column is full-width or full-screened, this is the width that should be restored
    /// upon unfullscreening and untoggling full-width.
    pub width: ColumnWidth,

    /// Whether this column is full-width.
    pub is_full_width: bool,

    /// Whether this column contains a single full-screened window.
    pub is_fullscreen: bool,

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
            focus_ring: FocusRing::new(options.focus_ring),
            view_offset: 0,
            view_offset_anim: None,
            activate_prev_column_on_removal: false,
            options,
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
            focus_ring: FocusRing::new(options.focus_ring),
            view_offset: 0,
            view_offset_anim: None,
            activate_prev_column_on_removal: false,
            options,
        }
    }

    pub fn advance_animations(&mut self, current_time: Duration, is_active: bool) {
        match &mut self.view_offset_anim {
            Some(anim) => {
                anim.set_current_time(current_time);
                self.view_offset = anim.value().round() as i32;
                if anim.is_done() {
                    self.view_offset_anim = None;
                }
            }
            None => (),
        }

        let view_pos = self.view_pos();

        // This shall one day become a proper animation.
        if !self.columns.is_empty() {
            let col = &self.columns[self.active_column_idx];
            let active_win = &col.windows[col.active_window_idx];
            let geom = active_win.geometry();
            let has_ssd = active_win.has_ssd();

            let win_pos = Point::from((
                self.column_x(self.active_column_idx) - view_pos,
                col.window_y(col.active_window_idx),
            ));

            self.focus_ring.update(win_pos, geom.size, has_ssd);
            self.focus_ring.set_active(is_active);
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.view_offset_anim.is_some()
    }

    pub fn update_config(&mut self, options: Rc<Options>) {
        self.focus_ring.update_config(options.focus_ring);
        // The focus ring buffer will be updated in a subsequent update_animations call.

        for column in &mut self.columns {
            column.update_config(options.clone());
        }

        self.options = options;
    }

    pub fn windows(&self) -> impl Iterator<Item = &W> + '_ {
        self.columns.iter().flat_map(|col| col.windows.iter())
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
            prepare_for_output(window, output);
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

    fn toplevel_bounds(&self) -> Size<i32, Logical> {
        Size::from((
            max(self.working_area.size.w - self.options.gaps * 2, 1),
            max(self.working_area.size.h - self.options.gaps * 2, 1),
        ))
    }

    pub fn configure_new_window(&self, window: &Window) {
        let width = if let Some(width) = self.options.default_width {
            max(1, width.resolve(&self.options, self.working_area.size.w))
        } else {
            0
        };

        let height = self.working_area.size.h - self.options.gaps * 2;
        let size = Size::from((width, max(height, 1)));

        let bounds = self.toplevel_bounds();

        if let Some(output) = self.output.as_ref() {
            prepare_for_output(window, output);
        }

        window.toplevel().with_pending_state(|state| {
            state.size = Some(size);
            state.bounds = Some(bounds);
        });
    }

    fn compute_new_view_offset_for_column(&self, current_x: i32, idx: usize) -> i32 {
        if self.columns[idx].is_fullscreen {
            return 0;
        }

        let new_col_x = self.column_x(idx);

        let final_x = if let Some(anim) = &self.view_offset_anim {
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

    fn animate_view_offset_to_column(&mut self, current_x: i32, idx: usize) {
        let new_view_offset = self.compute_new_view_offset_for_column(current_x, idx);

        let new_col_x = self.column_x(idx);
        let from_view_offset = current_x - new_col_x;
        self.view_offset = from_view_offset;

        // If we're already animating towards that, don't restart it.
        if let Some(anim) = &self.view_offset_anim {
            if anim.value().round() as i32 == self.view_offset
                && anim.to().round() as i32 == new_view_offset
            {
                return;
            }
        }

        // If our view offset is already this, we don't need to do anything.
        if self.view_offset == new_view_offset {
            self.view_offset_anim = None;
            return;
        }

        self.view_offset_anim = Some(Animation::new(
            self.view_offset as f64,
            new_view_offset as f64,
            Duration::from_millis(250),
        ));
    }

    fn activate_column(&mut self, idx: usize) {
        if self.active_column_idx == idx {
            return;
        }

        let current_x = self.view_pos();
        self.animate_view_offset_to_column(current_x, idx);

        self.active_column_idx = idx;

        // A different column was activated; reset the flag.
        self.activate_prev_column_on_removal = false;
    }

    pub fn has_windows(&self) -> bool {
        self.windows().next().is_some()
    }

    pub fn has_window(&self, window: &W) -> bool {
        self.windows().any(|win| win == window)
    }

    pub fn find_wl_surface(&self, wl_surface: &WlSurface) -> Option<&W> {
        self.windows().find(|win| win.is_wl_surface(wl_surface))
    }

    /// Computes the X position of the windows in the given column, in logical coordinates.
    fn column_x(&self, column_idx: usize) -> i32 {
        let mut x = 0;

        for column in self.columns.iter().take(column_idx) {
            x += column.width() + self.options.gaps;
        }

        x
    }

    pub fn add_window(
        &mut self,
        window: W,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        self.enter_output_for_window(&window);

        let was_empty = self.columns.is_empty();

        let idx = if self.columns.is_empty() {
            0
        } else {
            self.active_column_idx + 1
        };

        let column = Column::new(
            window,
            self.view_size,
            self.working_area,
            self.options.clone(),
            width,
            is_full_width,
        );
        self.columns.insert(idx, column);

        if activate {
            // If this is the first window on an empty workspace, skip the animation from whatever
            // view_offset was left over.
            if was_empty {
                // Try to make the code produce a left-aligned offset, even in presence of left
                // exclusive zones.
                self.view_offset = self.compute_new_view_offset_for_column(self.column_x(0), 0);
                self.view_offset_anim = None;
            }

            self.activate_column(idx);
            self.activate_prev_column_on_removal = true;
        }
    }

    pub fn remove_window_by_idx(&mut self, column_idx: usize, window_idx: usize) -> W {
        let column = &mut self.columns[column_idx];
        let window = column.windows.remove(window_idx);
        column.heights.remove(window_idx);

        if let Some(output) = &self.output {
            window.output_leave(output);
        }

        if column.windows.is_empty() {
            if column_idx + 1 == self.active_column_idx {
                // The previous column, that we were going to activate upon removal of the active
                // column, has just been itself removed.
                self.activate_prev_column_on_removal = false;
            }

            // FIXME: activate_column below computes current view position to compute the new view
            // position, which can include the column we're removing here. This leads to unwanted
            // view jumps.
            self.columns.remove(column_idx);
            if self.columns.is_empty() {
                return window;
            }

            if self.active_column_idx > column_idx
                || (self.active_column_idx == column_idx && self.activate_prev_column_on_removal)
            {
                // A column to the left was removed; preserve the current position.
                // FIXME: preserve activate_prev_column_on_removal.
                // Or, the active column was removed, and we needed to activate the previous column.
                self.activate_column(self.active_column_idx.saturating_sub(1));
            } else {
                self.activate_column(min(self.active_column_idx, self.columns.len() - 1));
            }

            return window;
        }

        column.active_window_idx = min(column.active_window_idx, column.windows.len() - 1);
        column.update_window_sizes();

        window
    }

    pub fn remove_window(&mut self, window: &W) {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();
        let column = &self.columns[column_idx];

        let window_idx = column.windows.iter().position(|win| win == window).unwrap();
        self.remove_window_by_idx(column_idx, window_idx);
    }

    pub fn update_window(&mut self, window: &W) {
        let (idx, column) = self
            .columns
            .iter_mut()
            .enumerate()
            .find(|(_, col)| col.contains(window))
            .unwrap();
        column.update_window_sizes();

        if idx == self.active_column_idx {
            // We might need to move the view to ensure the resized window is still visible.
            let current_x = self.view_pos();
            self.animate_view_offset_to_column(current_x, idx);
        }
    }

    pub fn activate_window(&mut self, window: &W) {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();
        let column = &mut self.columns[column_idx];

        column.activate_window(window);
        self.activate_column(column_idx);
    }

    #[cfg(test)]
    pub fn verify_invariants(&self) {
        assert!(self.view_size.w > 0);
        assert!(self.view_size.h > 0);

        if !self.columns.is_empty() {
            assert!(self.active_column_idx < self.columns.len());

            for column in &self.columns {
                column.verify_invariants();
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

    pub fn move_left(&mut self) {
        let new_idx = self.active_column_idx.saturating_sub(1);
        if self.active_column_idx == new_idx {
            return;
        }

        let current_x = self.view_pos();

        self.columns.swap(self.active_column_idx, new_idx);

        self.view_offset =
            self.compute_new_view_offset_for_column(current_x, self.active_column_idx);

        self.activate_column(new_idx);
    }

    pub fn move_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let new_idx = min(self.active_column_idx + 1, self.columns.len() - 1);
        if self.active_column_idx == new_idx {
            return;
        }

        let current_x = self.view_pos();

        self.columns.swap(self.active_column_idx, new_idx);

        self.view_offset =
            self.compute_new_view_offset_for_column(current_x, self.active_column_idx);

        self.activate_column(new_idx);
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

    pub fn consume_into_column(&mut self) {
        if self.columns.len() < 2 {
            return;
        }

        if self.active_column_idx == self.columns.len() - 1 {
            return;
        }

        let source_column_idx = self.active_column_idx + 1;
        let window = self.remove_window_by_idx(source_column_idx, 0);

        let target_column = &mut self.columns[self.active_column_idx];
        target_column.add_window(window);
    }

    pub fn expel_from_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let source_column = &self.columns[self.active_column_idx];
        if source_column.windows.len() == 1 {
            return;
        }

        let width = source_column.width;
        let is_full_width = source_column.is_full_width;
        let window =
            self.remove_window_by_idx(self.active_column_idx, source_column.active_window_idx);

        self.add_window(window, true, width, is_full_width);
    }

    pub fn center_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let col = &self.columns[self.active_column_idx];
        if col.is_fullscreen {
            return;
        }

        let width = col.width();

        // If the column is wider than the working area, then on commit it will be shifted to left
        // edge alignment by the usual positioning code, so there's no use in doing anything here.
        if self.working_area.size.w <= width {
            return;
        }

        let new_view_offset = -(self.working_area.size.w - width) / 2 - self.working_area.loc.x;

        // If we're already animating towards that, don't restart it.
        if let Some(anim) = &self.view_offset_anim {
            if anim.to().round() as i32 == new_view_offset {
                return;
            }
        }

        // If our view offset is already this, we don't need to do anything.
        if self.view_offset == new_view_offset {
            return;
        }

        self.view_offset_anim = Some(Animation::new(
            self.view_offset as f64,
            new_view_offset as f64,
            Duration::from_millis(250),
        ));
    }

    fn view_pos(&self) -> i32 {
        self.column_x(self.active_column_idx) + self.view_offset
    }

    pub fn window_under(&self, pos: Point<f64, Logical>) -> Option<(&W, Point<i32, Logical>)> {
        if self.columns.is_empty() {
            return None;
        }

        let view_pos = self.view_pos();

        // Prefer the active window since it's drawn on top.
        let col = &self.columns[self.active_column_idx];
        let active_win = &col.windows[col.active_window_idx];
        let geom = active_win.geometry();
        let buf_pos = Point::from((
            self.column_x(self.active_column_idx) - view_pos,
            col.window_y(col.active_window_idx),
        )) - geom.loc;
        if active_win.is_in_input_region(pos - buf_pos.to_f64()) {
            return Some((active_win, buf_pos));
        }

        let mut x = -view_pos;
        for col in &self.columns {
            for (win, y) in zip(&col.windows, col.window_ys()) {
                if win == active_win {
                    // Already handled it above.
                    continue;
                }

                let geom = win.geometry();
                let buf_pos = Point::from((x, y)) - geom.loc;
                if win.is_in_input_region(pos - buf_pos.to_f64()) {
                    return Some((win, buf_pos));
                }
            }

            x += col.width() + self.options.gaps;
        }

        None
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

    pub fn set_fullscreen(&mut self, window: &W, is_fullscreen: bool) {
        let (mut col_idx, win_idx) = self
            .columns
            .iter()
            .enumerate()
            .find_map(|(col_idx, col)| {
                col.windows
                    .iter()
                    .position(|w| w == window)
                    .map(|win_idx| (col_idx, win_idx))
            })
            .unwrap();

        let mut col = &mut self.columns[col_idx];

        if is_fullscreen && col.windows.len() > 1 {
            // This wasn't the only window in its column; extract it into a separate column.
            let target_window_was_focused =
                self.active_column_idx == col_idx && col.active_window_idx == win_idx;
            let window = col.windows.remove(win_idx);
            col.heights.remove(win_idx);
            col.active_window_idx = min(col.active_window_idx, col.windows.len() - 1);
            col.update_window_sizes();
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
                ),
            );
            if self.active_column_idx >= col_idx || target_window_was_focused {
                self.active_column_idx += 1;
            }
            col = &mut self.columns[col_idx];
        }

        col.set_fullscreen(is_fullscreen);
    }

    pub fn toggle_fullscreen(&mut self, window: &W) {
        let col = self
            .columns
            .iter_mut()
            .find(|col| col.windows.contains(window))
            .unwrap();
        let value = !col.is_fullscreen;
        self.set_fullscreen(window, value);
    }

    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer if we're on a fullscreen window and the view is stationary.
        if self.columns.is_empty() {
            return false;
        }

        if self.view_offset_anim.is_some() {
            return false;
        }

        self.columns[self.active_column_idx].is_fullscreen
    }
}

impl Workspace<Window> {
    pub fn refresh(&self) {
        let bounds = self.toplevel_bounds();

        for (col_idx, col) in self.columns.iter().enumerate() {
            for (win_idx, win) in col.windows.iter().enumerate() {
                let active = self.active_column_idx == col_idx && col.active_window_idx == win_idx;
                win.set_activated(active);

                win.toplevel().with_pending_state(|state| {
                    state.bounds = Some(bounds);
                });

                win.toplevel().send_pending_configure();
                win.refresh();
            }
        }
    }

    pub fn render_elements(
        &self,
        renderer: &mut GlesRenderer,
    ) -> Vec<WorkspaceRenderElement<GlesRenderer>> {
        if self.columns.is_empty() {
            return vec![];
        }

        // FIXME: workspaces should probably cache their last used scale so they can be correctly
        // rendered even with no outputs connected.
        let output_scale = self
            .output
            .as_ref()
            .map(|o| Scale::from(o.current_scale().fractional_scale()))
            .unwrap_or(Scale::from(1.));

        let mut rv = vec![];
        let view_pos = self.view_pos();

        // Draw the active window on top.
        let col = &self.columns[self.active_column_idx];
        let active_win = &col.windows[col.active_window_idx];
        let win_pos = Point::from((
            self.column_x(self.active_column_idx) - view_pos,
            col.window_y(col.active_window_idx),
        ));

        // Draw the window itself.
        let geom = active_win.geometry();
        let buf_pos = win_pos - geom.loc;
        rv.extend(active_win.render_elements(
            renderer,
            buf_pos.to_physical_precise_round(output_scale),
            output_scale,
            1.,
        ));

        // Draw the focus ring.
        rv.extend(self.focus_ring.render(output_scale).map(Into::into));

        let mut x = -view_pos;
        for col in &self.columns {
            for (win, y) in zip(&col.windows, col.window_ys()) {
                if win == active_win {
                    // Already handled it above.
                    continue;
                }

                let geom = win.geometry();
                let buf_pos = Point::from((x, y)) - geom.loc;
                rv.extend(win.render_elements(
                    renderer,
                    buf_pos.to_physical_precise_round(output_scale),
                    output_scale,
                    1.,
                ));
            }

            x += col.width() + self.options.gaps;
        }

        rv
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
    ) -> Self {
        let mut rv = Self {
            windows: vec![],
            heights: vec![],
            active_window_idx: 0,
            width,
            is_full_width,
            is_fullscreen: false,
            view_size,
            working_area,
            options,
        };

        rv.add_window(window);

        rv
    }

    fn set_view_size(&mut self, size: Size<i32, Logical>, working_area: Rectangle<i32, Logical>) {
        if self.view_size == size && self.working_area == working_area {
            return;
        }

        self.view_size = size;
        self.working_area = working_area;

        self.update_window_sizes();
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

        self.options = options;

        if update_sizes {
            self.update_window_sizes();
        }
    }

    fn set_width(&mut self, width: ColumnWidth) {
        self.width = width;
        self.is_full_width = false;
        self.update_window_sizes();
    }

    fn contains(&self, window: &W) -> bool {
        self.windows.iter().any(|win| win == window)
    }

    fn activate_window(&mut self, window: &W) {
        let idx = self.windows.iter().position(|win| win == window).unwrap();
        self.active_window_idx = idx;
    }

    fn add_window(&mut self, window: W) {
        self.is_fullscreen = false;
        self.windows.push(window);
        self.heights.push(WindowHeight::Auto);
        self.update_window_sizes();
    }

    fn update_window_sizes(&mut self) {
        if self.is_fullscreen {
            self.windows[0].request_fullscreen(self.view_size);
            return;
        }

        let min_size: Vec<_> = self.windows.iter().map(LayoutElement::min_size).collect();
        let max_size: Vec<_> = self.windows.iter().map(LayoutElement::max_size).collect();

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

        // Compute the window heights.
        let mut heights = self.heights.clone();
        let mut height_left = self.working_area.size.h - self.options.gaps;
        let mut auto_windows_left = self.windows.len();

        // Subtract all fixed-height windows.
        for (h, (min_size, max_size)) in zip(&mut heights, zip(&min_size, &max_size)) {
            // Check if the window has an exact height constraint.
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
                auto_windows_left -= 1;
            }
        }

        // Iteratively try to distribute the remaining height, checking against window min heights.
        // Pick an auto height according to the current sizes, then check if it satisfies all
        // remaining min heights. If not, allocate fixed height to those windows and repeat the
        // loop. On each iteration the auto height will get smaller.
        //
        // NOTE: we do not respect max height here. Doing so would complicate things: if the current
        // auto height is above some window's max height, then the auto height can become larger.
        // Combining this with the min height loop is where the complexity appears.
        //
        // However, most max height uses are for fixed-size dialogs, where min height == max_height.
        // This case is separately handled above.
        while auto_windows_left > 0 {
            // Compute the current auto height.
            let auto_height = height_left / auto_windows_left as i32 - self.options.gaps;
            let auto_height = max(auto_height, 1);

            // Integer division above can result in imperfect height distribution. We will make some
            // windows 1 px taller to account for this.
            let mut ones_left = height_left
                .saturating_sub((auto_height + self.options.gaps) * auto_windows_left as i32);

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
                    auto_windows_left -= 1;
                    unsatisfied_min = true;
                }
            }

            // If some min height was unsatisfied, then we allocated the window more than the auto
            // height, which means that the remaining auto windows now have less height to work
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
                auto_windows_left -= 1;
            }

            assert_eq!(auto_windows_left, 0);
        }

        for (win, h) in zip(&self.windows, heights) {
            let WindowHeight::Fixed(height) = h else {
                unreachable!()
            };

            let size = Size::from((width, height));
            win.request_size(size);
        }
    }

    fn width(&self) -> i32 {
        self.windows
            .iter()
            .map(|win| win.geometry().size.w)
            .max()
            .unwrap()
    }

    fn focus_up(&mut self) {
        self.active_window_idx = self.active_window_idx.saturating_sub(1);
    }

    fn focus_down(&mut self) {
        self.active_window_idx = min(self.active_window_idx + 1, self.windows.len() - 1);
    }

    fn move_up(&mut self) {
        let new_idx = self.active_window_idx.saturating_sub(1);
        if self.active_window_idx == new_idx {
            return;
        }

        self.windows.swap(self.active_window_idx, new_idx);
        self.heights.swap(self.active_window_idx, new_idx);
        self.active_window_idx = new_idx;
    }

    fn move_down(&mut self) {
        let new_idx = min(self.active_window_idx + 1, self.windows.len() - 1);
        if self.active_window_idx == new_idx {
            return;
        }

        self.windows.swap(self.active_window_idx, new_idx);
        self.heights.swap(self.active_window_idx, new_idx);
        self.active_window_idx = new_idx;
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        assert!(!self.windows.is_empty(), "columns can't be empty");
        assert!(self.active_window_idx < self.windows.len());
        assert_eq!(self.windows.len(), self.heights.len());

        if self.is_fullscreen {
            assert_eq!(self.windows.len(), 1);
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
        self.update_window_sizes();
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
            (_, SizeChange::SetFixed(fixed)) => ColumnWidth::Fixed(fixed.clamp(1, MAX_PX)),
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
        let current = self.heights[self.active_window_idx];
        let current_px = match current {
            WindowHeight::Auto => self.windows[self.active_window_idx].geometry().size.h,
            WindowHeight::Fixed(height) => height,
        };
        let current_prop = (current_px + self.options.gaps) as f64
            / (self.working_area.size.h - self.options.gaps) as f64;

        // FIXME: fix overflows then remove limits.
        const MAX_PX: i32 = 100000;

        let mut height = match change {
            SizeChange::SetFixed(fixed) => fixed,
            SizeChange::SetProportion(proportion) => {
                ((self.working_area.size.h - self.options.gaps) as f64 * proportion
                    - self.options.gaps as f64)
                    .round() as i32
            }
            SizeChange::AdjustFixed(delta) => current_px.saturating_add(delta),
            SizeChange::AdjustProportion(delta) => {
                let proportion = current_prop + delta / 100.;
                ((self.working_area.size.h - self.options.gaps) as f64 * proportion
                    - self.options.gaps as f64)
                    .round() as i32
            }
        };

        // Clamp it against the window height constraints.
        let win = &self.windows[self.active_window_idx];
        let min_h = win.min_size().h;
        let max_h = win.max_size().h;

        if max_h > 0 {
            height = height.min(max_h);
        }
        if min_h > 0 {
            height = height.max(min_h);
        }

        self.heights[self.active_window_idx] = WindowHeight::Fixed(height.clamp(1, MAX_PX));
        self.update_window_sizes();
    }

    fn set_fullscreen(&mut self, is_fullscreen: bool) {
        assert_eq!(self.windows.len(), 1);
        self.is_fullscreen = is_fullscreen;
        self.update_window_sizes();
    }

    pub fn window_y(&self, window_idx: usize) -> i32 {
        self.window_ys().nth(window_idx).unwrap()
    }

    fn window_ys(&self) -> impl Iterator<Item = i32> + '_ {
        let mut y = 0;

        if !self.is_fullscreen {
            y = self.working_area.loc.y + self.options.gaps;
        }

        self.windows.iter().map(move |win| {
            let pos = y;
            y += win.geometry().size.h + self.options.gaps;
            pos
        })
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

    // Compute the padding in case it needs to be smaller due to large window width.
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

fn prepare_for_output(window: &impl LayoutElement, output: &Output) {
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
