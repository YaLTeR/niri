use std::time::Duration;

use niri_config::PresetSize;
use smithay::utils::{Logical, Size};

use crate::animation::Animation;
use crate::input::swipe_tracker::SwipeTracker;
use crate::animation::Clock;

use super::super::LayoutElement;

/// Amount of touchpad movement to scroll the view for the width of one working area.
pub const VIEW_GESTURE_WORKING_AREA_MOVEMENT: f64 = 1200.;

/// Extra per-column data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnData {
    /// Cached actual column width.
    pub width: f64,
}

impl ColumnData {
    pub fn new<W: LayoutElement>(column: &super::column::Column<W>) -> Self {
        let mut rv = Self { width: 0. };
        rv.update(column);
        rv
    }

    pub fn update<W: LayoutElement>(&mut self, column: &super::column::Column<W>) {
        self.width = column.width();
    }
}

/// Extra per-tile data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileData {
    /// Requested height of the window.
    ///
    /// This is window height, not tile height, so it excludes tile decorations.
    pub height: WindowHeight,

    /// Cached actual size of the tile.
    pub size: Size<f64, Logical>,

    /// Cached whether the tile is being interactively resized by its left edge.
    pub interactively_resizing_by_left_edge: bool,
}

impl TileData {
    pub fn new<W: LayoutElement>(tile: &super::super::tile::Tile<W>, height: WindowHeight) -> Self {
        let mut rv = Self {
            height,
            size: Size::default(),
            interactively_resizing_by_left_edge: false,
        };
        rv.update(tile);
        rv
    }

    pub fn update<W: LayoutElement>(&mut self, tile: &super::super::tile::Tile<W>) {
        use crate::utils::ResizeEdge;
        
        self.size = tile.tile_size();
        self.interactively_resizing_by_left_edge = tile
            .window()
            .interactive_resize_data()
            .is_some_and(|data| data.edges.contains(ResizeEdge::LEFT));
    }
}

/// Width of a column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    /// Proportion of the current view width.
    Proportion(f64),
    /// Fixed width in logical pixels.
    Fixed(f64),
}

impl From<PresetSize> for ColumnWidth {
    fn from(value: PresetSize) -> Self {
        match value {
            PresetSize::Proportion(p) => Self::Proportion(p.clamp(0., 10000.)),
            PresetSize::Fixed(f) => Self::Fixed(f64::from(f.clamp(1, 100000))),
        }
    }
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

impl WindowHeight {
    pub const fn auto_1() -> Self {
        Self::Auto { weight: 1. }
    }
}

/// Horizontal direction for an operation.
///
/// As operations often have a symmetrical counterpart, e.g. focus-right/focus-left, methods
/// on `Scrolling` can sometimes be factored using the direction of the operation as a parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDirection {
    Left,
    Right,
}

#[derive(Debug)]
pub enum ViewOffset {
    /// The view offset is static.
    Static(f64),
    /// The view offset is animating.
    Animation(Animation),
    /// The view offset is controlled by the ongoing gesture.
    Gesture(ViewGesture),
}

impl ViewOffset {
    pub fn current(&self) -> f64 {
        match self {
            ViewOffset::Static(offset) => *offset,
            ViewOffset::Animation(anim) => anim.value(),
            ViewOffset::Gesture(gesture) => {
                gesture.current_view_offset + gesture.animation.as_ref().map_or(0., |a| a.value())
            }
        }
    }

    pub fn target(&self) -> f64 {
        match self {
            ViewOffset::Static(offset) => *offset,
            ViewOffset::Animation(anim) => anim.to(),
            ViewOffset::Gesture(gesture) => gesture.current_view_offset,
        }
    }

    pub fn stationary(&self) -> f64 {
        match self {
            ViewOffset::Static(offset) => *offset,
            ViewOffset::Animation(anim) => anim.to(),
            ViewOffset::Gesture(gesture) => gesture.stationary_view_offset,
        }
    }

    pub fn offset(&mut self, offset: f64) {
        match self {
            ViewOffset::Static(cur) => *cur += offset,
            ViewOffset::Animation(anim) => anim.offset(offset),
            ViewOffset::Gesture(gesture) => {
                gesture.current_view_offset += offset;
                gesture.delta_from_tracker += offset;
                gesture.stationary_view_offset += offset;
            }
        }
    }

    pub fn is_static(&self) -> bool {
        matches!(self, ViewOffset::Static(_))
    }

    pub fn is_gesture(&self) -> bool {
        matches!(self, ViewOffset::Gesture(_))
    }

    pub fn is_dnd_scroll(&self) -> bool {
        matches!(
            self,
            ViewOffset::Gesture(ViewGesture {
                dnd_last_event_time: Some(_),
                ..
            })
        )
    }

    pub fn is_animation_ongoing(&self) -> bool {
        match self {
            ViewOffset::Static(_) => false,
            ViewOffset::Animation(_) => true,
            ViewOffset::Gesture(gesture) => gesture.animation.is_some(),
        }
    }

    pub fn stop_anim_and_gesture(&mut self) {
        *self = ViewOffset::Static(self.current());
    }

    pub fn cancel_gesture(&mut self) {
        if let ViewOffset::Gesture(gesture) = self {
            *self = ViewOffset::Static(gesture.stationary_view_offset);
        }
    }
}

#[derive(Debug)]
pub struct ViewGesture {
    pub current_view_offset: f64,
    /// Animation for the extra offset to the current position.
    ///
    /// For example, when we need to activate a specific window during a DnD scroll.
    pub animation: Option<Animation>,
    pub tracker: SwipeTracker,
    pub delta_from_tracker: f64,
    // The view offset we'll use if needed for activate_prev_column_on_removal.
    pub stationary_view_offset: f64,
    /// Whether the gesture is controlled by the touchpad.
    pub is_touchpad: bool,

    // If this gesture is for drag-and-drop scrolling, this is the last event's unadjusted
    // timestamp.
    pub dnd_last_event_time: Option<Duration>,
    // Time when the drag-and-drop scroll delta became non-zero, used for debouncing.
    //
    // If `None` then the scroll delta is currently zero.
    pub dnd_nonzero_start_time: Option<Duration>,
}

impl ViewGesture {
    pub fn animate_from(&mut self, from: f64, clock: Clock, config: niri_config::Animation) {
        let anim = Animation::new(clock, from, 0., 0., config);
        self.animation = Some(anim);
    }
}

#[derive(Debug)]
pub struct MoveAnimation {
    pub anim: Animation,
    pub from: f64,
}
