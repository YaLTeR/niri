// Column module - manages a single column of tiled windows
//
// This module is split into several files to keep each under 400 lines:
// - mod.rs: Struct definition and module organization
// - core.rs: Construction, configuration, animation, rendering
// - sizing.rs: Size calculation and tile height distribution
// - operations.rs: Width/height operations, focus, display mode
// - positioning.rs: Tile positioning, iterators, verification

mod core;
mod operations;
mod positioning;
mod sizing;

use std::rc::Rc;

use niri_ipc::ColumnDisplay;
use smithay::utils::{Logical, Rectangle, Size};

use super::super::tab_indicator::TabIndicator;
use super::super::tile::Tile;
use super::super::{LayoutElement, Options};
use super::types::{ColumnWidth, MoveAnimation, TileData};
use crate::animation::Clock;

/// A column of tiled windows in the scrolling layout.
#[derive(Debug)]
pub struct Column<W: LayoutElement> {
    /// Tiles in this column.
    ///
    /// Must be non-empty.
    pub(super) tiles: Vec<Tile<W>>,

    /// Extra per-tile data.
    ///
    /// Must have the same number of elements as `tiles`.
    pub(super) data: Vec<TileData>,

    /// Index of the currently active tile.
    pub(super) active_tile_idx: usize,

    /// Desired width of this column.
    ///
    /// If the column is full-width or full-screened, this is the width that should be restored
    /// upon unfullscreening and untoggling full-width.
    pub(super) width: ColumnWidth,

    /// Currently selected preset width index.
    pub(super) preset_width_idx: Option<usize>,

    /// Whether this column is full-width.
    pub(super) is_full_width: bool,

    /// Whether this column is going to be fullscreen.
    ///
    /// This is the compositor-side fullscreen state, so it changes immediately upon
    /// set_fullscreen(). The actual tiles will take some time to respond to the fullscreen request
    /// and become fullscreen.
    ///
    /// Similarly, unsetting fullscreen will change this value to false immediately, and tiles will
    /// take some time to catch up and actually unfullscreen.
    pub(super) is_pending_fullscreen: bool,

    /// Whether this column is going to be maximized.
    ///
    /// Can be `true` together with `is_pending_fullscreen`, which means that the column is
    /// effectively pending fullscreen, but unfullscreening should go back to maximized state,
    /// rather than normal.
    pub(super) is_pending_maximized: bool,

    /// How this column displays and arranges windows.
    pub(super) display_mode: ColumnDisplay,

    /// Tab indicator for the tabbed display mode.
    pub(super) tab_indicator: TabIndicator,

    /// Animation of the render offset during window swapping.
    pub(super) move_animation: Option<MoveAnimation>,

    /// Latest known view size for this column's workspace.
    pub(super) view_size: Size<f64, Logical>,

    /// Latest known working area for this column's workspace.
    pub(super) working_area: Rectangle<f64, Logical>,

    /// Working area for this column's workspace excluding struts.
    ///
    /// Used for maximize-to-edges.
    pub(super) parent_area: Rectangle<f64, Logical>,

    /// Scale of the output the column is on (and rounds its sizes to).
    pub(super) scale: f64,

    /// Clock for driving animations.
    pub(super) clock: Clock,

    /// Configurable properties of the layout.
    pub(super) options: Rc<Options>,
}

// Re-export all the implementation pieces
