// Re-export all public types and functions from the original scrolling.rs
// This module acts as a shim to maintain backward compatibility

mod column;
mod space;
mod types;
mod utils;

// New modularized functionality
mod manipulation;
mod gestures;
mod resize;
mod render;

// Re-export public types
pub use column::Column;
pub use space::{ScrollingSpace, ScrollingSpaceRenderElement};
pub use types::{ColumnData, ColumnWidth, ScrollDirection, TileData, WindowHeight, VIEW_GESTURE_WORKING_AREA_MOVEMENT};
pub use utils::{compute_new_view_offset, compute_toplevel_bounds, compute_working_area, resolve_preset_size};
pub use super::workspace::ResolvedSize;
pub use super::super::window::ResolvedWindowRules;
pub use super::SizingMode;

// Re-export internal types needed by other modules
pub(super) use types::ViewOffset;

// Re-export resize-related types
pub use super::workspace::InteractiveResize;
pub use super::InteractiveResizeData;

