// ScrollingSpace module - manages a scrollable tiling space for windows
//
// This module is split into several files to keep each under 400 lines:
// - mod.rs: Struct definition and module organization
// - core.rs: Construction, configuration, animation, basic queries
// - view_offset.rs: View offset computation and animation
// - queries.rs: Window lookup, bounds, insert position

mod core;
mod queries;
pub mod view_offset;

use std::rc::Rc;

use smithay::utils::{Logical, Rectangle, Size};

use super::super::closing_window::ClosingWindow;
use super::super::workspace::InteractiveResize;
use super::super::{LayoutElement, Options};
use super::column::Column;
use super::types::{ColumnData, ViewOffset};

/// A scrollable-tiling space for windows.
#[derive(Debug)]
pub struct ScrollingSpace<W: LayoutElement> {
    /// Columns of windows on this space.
    pub(super) columns: Vec<Column<W>>,

    /// Extra per-column data.
    pub(super) data: Vec<ColumnData>,

    /// Index of the currently active column, if any.
    pub(super) active_column_idx: usize,

    /// Ongoing interactive resize.
    pub(super) interactive_resize: Option<InteractiveResize<W>>,

    /// Offset of the view computed from the active column.
    ///
    /// Any gaps, including left padding from work area left exclusive zone, is handled
    /// with this view offset (rather than added as a constant elsewhere in the code). This allows
    /// for natural handling of fullscreen windows, which must ignore work area padding.
    pub(super) view_offset: ViewOffset,

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
    pub(super) activate_prev_column_on_removal: Option<f64>,

    /// View offset to restore after unfullscreening or unmaximizing.
    pub(super) view_offset_to_restore: Option<f64>,

    /// Windows in the closing animation.
    pub(super) closing_windows: Vec<ClosingWindow>,

    /// View size for this space.
    pub(super) view_size: Size<f64, Logical>,

    /// Working area for this space.
    ///
    /// Takes into account layer-shell exclusive zones and niri struts.
    pub(super) working_area: Rectangle<f64, Logical>,

    /// Working area for this space excluding struts.
    ///
    /// Used for popup unconstraining. Popups can go over struts, but they shouldn't go over
    /// the layer-shell top layer (which renders on top of popups).
    pub(super) parent_area: Rectangle<f64, Logical>,

    /// Scale of the output the space is on (and rounds its sizes to).
    pub(super) scale: f64,

    /// Clock for driving animations.
    pub(super) clock: crate::animation::Clock,

    /// Configurable properties of the layout.
    pub(super) options: Rc<Options>,
}

crate::niri_render_elements! {
    ScrollingSpaceRenderElement<R> => {
        Tile = super::super::tile::TileRenderElement<R>,
        ClosingWindow = super::super::closing_window::ClosingWindowRenderElement,
        TabIndicator = super::super::tab_indicator::TabIndicatorRenderElement,
    }
}
