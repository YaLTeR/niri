//! Snapshot generation for scrolling layout state.
//!
//! This module provides a unified snapshot format used by both the original
//! and refactored scrolling implementations. The snapshot captures the logical
//! layout state in a parsable format for golden tests.
//!
//! The format is designed to be:
//! - Easily parsable for generating both LTR and RTL golden files
//! - Consistent across implementations
//! - Human-readable for debugging

use std::fmt::Debug;

use smithay::utils::{Logical, Rectangle, Size};

use super::LayoutElement;

/// Column width specification - mirrors the one in scrolling types.
/// We duplicate this here to avoid circular dependencies between modules.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    /// Proportion of the working area.
    Proportion(f64),
    /// Fixed width in logical pixels.
    Fixed(f64),
}

/// Trait for columns that can be snapshotted.
pub trait SnapshotColumn<W: LayoutElement> {
    fn snapshot_width(&self) -> ColumnWidth;
    fn active_tile_idx(&self) -> usize;
    fn tile_count(&self) -> usize;
    fn tile_size(&self, idx: usize) -> Size<f64, Logical>;
    fn tile_window_id(&self, idx: usize) -> &W::Id;
}

/// Trait for scrolling spaces that can be snapshotted.
pub trait SnapshotScrollingSpace<W: LayoutElement> {
    type Column: SnapshotColumn<W>;
    
    fn view_size(&self) -> Size<f64, Logical>;
    fn scale(&self) -> f64;
    fn working_area(&self) -> Rectangle<f64, Logical>;
    fn parent_area(&self) -> Rectangle<f64, Logical>;
    fn gaps(&self) -> f64;
    fn view_offset_debug(&self) -> String;
    fn view_pos(&self) -> f64;
    fn active_column_idx(&self) -> usize;
    fn column_xs(&self) -> Vec<f64>;
    fn columns(&self) -> &[Self::Column];
}

/// Generate a snapshot string from any type implementing SnapshotScrollingSpace.
pub fn generate_snapshot<W: LayoutElement, S: SnapshotScrollingSpace<W>>(space: &S) -> String {
    let mut s = String::new();
    
    let view_size = space.view_size();
    let working_area = space.working_area();
    let parent_area = space.parent_area();
    let gaps = space.gaps();
    let view_pos = space.view_pos();
    let active_column_idx = space.active_column_idx();
    let col_xs = space.column_xs();
    let columns = space.columns();
    
    // View/output dimensions
    s.push_str(&format!("view_width={:.0}\n", view_size.w));
    s.push_str(&format!("view_height={:.0}\n", view_size.h));
    
    // Scale factor
    s.push_str(&format!("scale={}\n", space.scale()));
    
    // Working area
    s.push_str(&format!("working_area_x={:.0}\n", working_area.loc.x));
    s.push_str(&format!("working_area_y={:.0}\n", working_area.loc.y));
    s.push_str(&format!("working_area_width={:.0}\n", working_area.size.w));
    s.push_str(&format!("working_area_height={:.0}\n", working_area.size.h));
    
    // Parent area
    s.push_str(&format!("parent_area_x={:.0}\n", parent_area.loc.x));
    s.push_str(&format!("parent_area_y={:.0}\n", parent_area.loc.y));
    s.push_str(&format!("parent_area_width={:.0}\n", parent_area.size.w));
    s.push_str(&format!("parent_area_height={:.0}\n", parent_area.size.h));
    
    // Layout options
    s.push_str(&format!("gaps={}\n", gaps));
    
    // View offset
    s.push_str(&format!("view_offset={}\n", space.view_offset_debug()));
    
    // View position
    s.push_str(&format!("view_pos={:.1}\n", view_pos));
    
    // Active column index
    s.push_str(&format!("active_column={}\n", active_column_idx));
    
    // Active column position
    if !columns.is_empty() {
        let active_col_x = col_xs[active_column_idx];
        s.push_str(&format!("active_column_x={:.1}\n", active_col_x));
    }
    
    // Active tile position in viewport space
    if !columns.is_empty() {
        let active_col = &columns[active_column_idx];
        if active_col.tile_count() > 0 {
            let active_col_x = col_xs[active_column_idx];
            
            let active_tile_viewport_x = active_col_x - view_pos;
            
            let active_tile_idx = active_col.active_tile_idx();
            let mut y = 0.0;
            for i in 0..active_tile_idx {
                y += active_col.tile_size(i).h + gaps;
            }
            
            s.push_str(&format!("active_tile_viewport_x={:.1}\n", active_tile_viewport_x));
            s.push_str(&format!("active_tile_viewport_y={:.1}\n", y));
        }
    }
    
    // Column and tile structure
    for (i, col) in columns.iter().enumerate() {
        let is_active_col = i == active_column_idx;
        let col_x = col_xs[i];
        
        s.push_str(&format!(
            "column[{}]{}: x={:.1} width={:?} active_tile={}\n",
            i,
            if is_active_col { " [ACTIVE]" } else { "" },
            col_x,
            col.snapshot_width(),
            col.active_tile_idx()
        ));
        
        let mut tile_y = 0.0;
        for j in 0..col.tile_count() {
            let is_active_tile = is_active_col && j == col.active_tile_idx();
            let tile_size = col.tile_size(j);
            
            s.push_str(&format!(
                "  tile[{}]{}: x={:.1} y={:.1} w={:.0} h={:.0} window_id={:?}\n",
                j,
                if is_active_tile { " [ACTIVE]" } else { "" },
                col_x,
                tile_y,
                tile_size.w,
                tile_size.h,
                col.tile_window_id(j)
            ));
            
            tile_y += tile_size.h + gaps;
        }
    }
    
    s
}
