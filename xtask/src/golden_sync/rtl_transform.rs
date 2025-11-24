//! RTL snapshot transformation
//!
//! Transforms LTR golden snapshots into RTL equivalents by mirroring x-positions.

use std::collections::BTreeMap;

use regex::Regex;

/// Header lines for LTR golden files
pub const LTR_HEADER: &[&str] = &[
    "# AUTO-GENERATED - DO NOT EDIT",
    "# Source: scrolling_original.rs (via snapshot tests)",
    "# Run `cargo xtask sync-golden` to regenerate",
];

/// Header lines for RTL golden files  
const RTL_HEADER: &[&str] = &[
    "# AUTO-GENERATED - DO NOT EDIT",
    "# Source: scrolling_original.rs -> LTR golden -> rtl_calculator.rs",
    "# Run `cargo xtask sync-golden` to regenerate",
];

/// Format header lines into a string with trailing blank line
pub fn format_header(header: &[&str]) -> String {
    let mut result = header.join("\n");
    result.push_str("\n\n");
    result
}

/// Generate an RTL snapshot from an LTR snapshot by mirroring x-positions
pub fn generate_rtl_snapshot(ltr_snapshot: &str) -> String {
    let metadata = parse_ltr_metadata(ltr_snapshot);
    let columns = parse_columns(ltr_snapshot);
    let rtl_positions = calculate_rtl_positions(&metadata, &columns);
    let view_state = calculate_rtl_view_state(&metadata, &columns, &rtl_positions);
    
    generate_rtl_content(ltr_snapshot, &rtl_positions, &view_state)
}

/// Metadata parsed from LTR snapshot
struct LtrMetadata {
    working_area_x: f64,
    working_area_width: f64,
    gaps: f64,
    active_col_idx: usize,
}

/// RTL view state (offsets and positions)
struct RtlViewState {
    view_offset: f64,
    view_pos: f64,
    active_col_x: f64,
    active_tile_viewport_x: f64,
}

/// Parse metadata from LTR snapshot
fn parse_ltr_metadata(ltr_snapshot: &str) -> LtrMetadata {
    let mut working_area_x: f64 = 0.0;
    let mut working_area_width: f64 = 1280.0;
    let mut gaps: f64 = 0.0;
    let mut active_col_idx: usize = 0;
    
    for line in ltr_snapshot.lines() {
        if line.starts_with("working_area_x=") {
            working_area_x = line.trim_start_matches("working_area_x=").parse().unwrap_or(0.0);
        } else if line.starts_with("working_area_width=") {
            working_area_width = line.trim_start_matches("working_area_width=").parse().unwrap_or(1280.0);
        } else if line.starts_with("gaps=") {
            gaps = line.trim_start_matches("gaps=").parse().unwrap_or(0.0);
        } else if line.starts_with("active_column=") {
            active_col_idx = line.trim_start_matches("active_column=").parse().unwrap_or(0);
        }
    }
    
    LtrMetadata {
        working_area_x,
        working_area_width,
        gaps,
        active_col_idx,
    }
}

/// Parse columns and their widths from LTR snapshot
fn parse_columns(ltr_snapshot: &str) -> Vec<(usize, f64)> {
    let mut columns: Vec<(usize, f64)> = Vec::new();
    let mut current_col_idx = 0;
    
    for line in ltr_snapshot.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("column[") {
            // Extract column index
            if let Some(idx_end) = trimmed.find(']') {
                let idx_str = &trimmed[7..idx_end];
                current_col_idx = idx_str.parse().unwrap_or(0);
            }
        } else if trimmed.starts_with("tile[") {
            // Extract width from tile line: "tile[0] [ACTIVE]: x=0.0 y=0.0 w=426 h=720 window_id=1"
            if let Some(w_start) = trimmed.find("w=") {
                let w_str = &trimmed[w_start + 2..];
                if let Some(w_end) = w_str.find(' ') {
                    let width: f64 = w_str[..w_end].parse().unwrap_or(0.0);
                    // Only add if we haven't seen this column yet
                    if !columns.iter().any(|(idx, _)| *idx == current_col_idx) {
                        columns.push((current_col_idx, width));
                    }
                }
            }
        }
    }
    
    // Sort columns by index
    columns.sort_by_key(|(idx, _)| *idx);
    columns
}

/// Calculate RTL x-positions for each column
fn calculate_rtl_positions(metadata: &LtrMetadata, columns: &[(usize, f64)]) -> BTreeMap<usize, f64> {
    let mut rtl_positions: BTreeMap<usize, f64> = BTreeMap::new();
    let right_edge = metadata.working_area_x + metadata.working_area_width;
    let mut x = right_edge;
    
    for (col_idx, width) in columns {
        x -= width;
        rtl_positions.insert(*col_idx, x);
        x -= metadata.gaps;
    }
    
    rtl_positions
}

/// Calculate RTL view state (view_offset, view_pos, etc.)
fn calculate_rtl_view_state(
    metadata: &LtrMetadata,
    columns: &[(usize, f64)],
    rtl_positions: &BTreeMap<usize, f64>,
) -> RtlViewState {
    let active_col_rtl_x = rtl_positions.get(&metadata.active_col_idx).copied().unwrap_or(0.0);
    let active_col_width = columns
        .iter()
        .find(|(idx, _)| *idx == metadata.active_col_idx)
        .map(|(_, w)| *w)
        .unwrap_or(0.0);
    
    // RTL view_offset calculation
    // In RTL mode, view_pos = view_offset (unlike LTR where view_pos = column_x + view_offset)
    // If active column is within viewport, no scrolling needed
    // Otherwise, scroll to show active column at left edge
    let rtl_view_offset = if active_col_rtl_x >= 0.0 
        && active_col_rtl_x + active_col_width <= metadata.working_area_width 
    {
        0.0
    } else {
        active_col_rtl_x
    };
    
    let rtl_view_pos = rtl_view_offset;
    let rtl_active_tile_viewport_x = active_col_rtl_x - rtl_view_pos;
    
    RtlViewState {
        view_offset: rtl_view_offset,
        view_pos: rtl_view_pos,
        active_col_x: active_col_rtl_x,
        active_tile_viewport_x: rtl_active_tile_viewport_x,
    }
}

/// Generate the RTL snapshot content
fn generate_rtl_content(
    ltr_snapshot: &str,
    rtl_positions: &BTreeMap<usize, f64>,
    view_state: &RtlViewState,
) -> String {
    let mut result = Vec::new();
    
    // Add header (same format as LTR)
    for line in RTL_HEADER {
        result.push(line.to_string());
    }
    result.push(String::new()); // Blank line after header
    
    let mut current_col_for_tiles = 0usize;
    
    for line in ltr_snapshot.lines() {
        let trimmed = line.trim();
        
        // Skip comment lines and empty lines at the start (from LTR header)
        if trimmed.starts_with('#') || (trimmed.is_empty() && result.len() <= 4) {
            continue;
        }
        
        if trimmed.starts_with("view_offset=") {
            result.push(format!("view_offset=Static({:.1})", view_state.view_offset));
        } else if trimmed.starts_with("view_pos=") {
            result.push(format!("view_pos={:.1}", view_state.view_pos));
        } else if trimmed.starts_with("active_column_x=") {
            result.push(format!("active_column_x={:.1}", view_state.active_col_x));
        } else if trimmed.starts_with("active_tile_viewport_x=") {
            result.push(format!("active_tile_viewport_x={:.1}", view_state.active_tile_viewport_x));
        } else if trimmed.starts_with("column[") {
            // Extract column index and transform x
            if let Some(idx_end) = trimmed.find(']') {
                let idx_str = &trimmed[7..idx_end];
                let col_idx: usize = idx_str.parse().unwrap_or(0);
                current_col_for_tiles = col_idx;
                
                if let Some(&rtl_x) = rtl_positions.get(&col_idx) {
                    let new_line = replace_x_in_line(trimmed, rtl_x);
                    result.push(new_line);
                } else {
                    result.push(trimmed.to_string());
                }
            } else {
                result.push(trimmed.to_string());
            }
        } else if trimmed.starts_with("tile[") {
            // Use the current column's RTL x position
            if let Some(&rtl_x) = rtl_positions.get(&current_col_for_tiles) {
                let new_line = replace_x_in_line(trimmed, rtl_x);
                result.push(format!("  {}", new_line));
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }
    
    result.join("\n")
}

/// Replace x= value in a line with a new value
fn replace_x_in_line(line: &str, new_x: f64) -> String {
    let re = Regex::new(r"x=[0-9.-]+").unwrap();
    re.replace(line, format!("x={:.1}", new_x)).to_string()
}
