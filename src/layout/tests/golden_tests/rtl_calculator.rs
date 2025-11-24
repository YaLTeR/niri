//! LTR to RTL transformation calculator
//!
//! This module provides utilities to calculate expected RTL geometry from LTR snapshots.
//! RTL is a mathematical mirror transformation of LTR, not a separate specification.

/// Parsed tile geometry from a snapshot
#[derive(Debug, Clone, PartialEq)]
pub struct TileGeometry {
    pub width: f64,
    pub height: f64,
    pub window_id: usize,
}

/// Expected RTL position for a tile
#[derive(Debug, Clone, PartialEq)]
pub struct RtlPosition {
    pub left: f64,
    pub right: f64,
    pub width: f64,
}

impl RtlPosition {
    /// Format as a string for comparison with format_column_edges output
    pub fn format(&self) -> String {
        format!(
            "left:{:>4.0} right:{:>4.0} width:{:>4.0}",
            self.left, self.right, self.width
        )
    }
}

/// Calculate RTL x-position from LTR position and width.
/// 
/// In LTR: column starts at x=0, grows rightward
/// In RTL: column starts at x=view_width, grows leftward
/// 
/// Transformation: rtl_left = view_width - ltr_left - width
pub fn mirror_x(ltr_x: f64, width: f64, view_width: f64) -> f64 {
    view_width - ltr_x - width
}

/// Parse a snapshot to extract tile geometries.
pub fn parse_tiles(snapshot: &str) -> Vec<TileGeometry> {
    let mut tiles = Vec::new();
    
    for line in snapshot.lines() {
        if line.trim().starts_with("tile[") {
            // Parse: "  tile[0]: w=426 h=720 window_id=1"
            let mut width = None;
            let mut height = None;
            let mut window_id = None;
            
            if let Some(w_start) = line.find("w=") {
                let w_str = &line[w_start + 2..];
                if let Some(w_end) = w_str.find(' ') {
                    width = w_str[..w_end].parse::<f64>().ok();
                }
            }
            
            if let Some(h_start) = line.find("h=") {
                let h_str = &line[h_start + 2..];
                if let Some(h_end) = h_str.find(' ') {
                    height = h_str[..h_end].parse::<f64>().ok();
                }
            }
            
            if let Some(id_start) = line.find("window_id=") {
                let id_str = &line[id_start + 10..];
                // Parse until end of line or whitespace
                let id_end = id_str.find(|c: char| c.is_whitespace()).unwrap_or(id_str.len());
                window_id = id_str[..id_end].parse::<usize>().ok();
            }
            
            if let (Some(w), Some(h), Some(id)) = (width, height, window_id) {
                tiles.push(TileGeometry {
                    width: w,
                    height: h,
                    window_id: id,
                });
            }
        }
    }
    
    tiles
}

/// Parse view_width from snapshot.
/// Returns None if not found.
fn parse_view_width(snapshot: &str) -> Option<f64> {
    for line in snapshot.lines() {
        if let Some(value) = line.strip_prefix("view_width=") {
            return value.trim().parse::<f64>().ok();
        }
    }
    None
}

/// Calculate expected RTL positions from LTR snapshot.
/// 
/// Assumes LTR tiles start at x=0 (left-aligned).
/// RTL tiles should be right-aligned at x=view_width.
/// 
/// The view_width is parsed from the snapshot for deterministic calculation.
pub fn calculate_rtl_positions(ltr_snapshot: &str) -> Vec<RtlPosition> {
    let tiles = parse_tiles(ltr_snapshot);
    let view_width = parse_view_width(ltr_snapshot)
        .expect("view_width not found in snapshot - ensure snapshot includes view dimensions");
    
    tiles.iter().map(|tile| {
        // In LTR, single column starts at x=0
        let ltr_x = 0.0;
        let rtl_left = mirror_x(ltr_x, tile.width, view_width);
        let rtl_right = rtl_left + tile.width;
        
        RtlPosition {
            left: rtl_left,
            right: rtl_right,
            width: tile.width,
        }
    }).collect()
}

/// Calculate expected RTL positions for multiple columns.
/// 
/// For multi-column layouts, we need to know the LTR x-positions.
/// This can be extracted from format_column_edges output.
pub fn calculate_rtl_positions_multi_column(
    ltr_edges: &str,
    view_width: f64,
) -> Vec<RtlPosition> {
    let mut positions = Vec::new();
    
    for line in ltr_edges.lines() {
        // Parse: "left:   0 right: 426 width: 426"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 {
            if let (Ok(ltr_left), Ok(width)) = (
                parts[0].trim_start_matches("left:").parse::<f64>(),
                parts[4].trim_start_matches("width:").parse::<f64>(),
            ) {
                let rtl_left = mirror_x(ltr_left, width, view_width);
                let rtl_right = rtl_left + width;
                
                positions.push(RtlPosition {
                    left: rtl_left,
                    right: rtl_right,
                    width,
                });
            }
        }
    }
    
    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_x() {
        let view_width = 1280.0;
        
        // LTR tile at x=0, width=426 should mirror to x=854
        assert_eq!(mirror_x(0.0, 426.0, view_width), 854.0);
        
        // LTR tile at x=0, width=640 should mirror to x=640
        assert_eq!(mirror_x(0.0, 640.0, view_width), 640.0);
        
        // LTR tile at x=0, width=853 should mirror to x=427
        assert_eq!(mirror_x(0.0, 853.0, view_width), 427.0);
    }

    #[test]
    fn test_parse_tiles() {
        let snapshot = r"
view_width=1280
view_height=720
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(0.33333333333333337) active_tile=0
  tile[0]: w=426 h=720 window_id=1
";
        
        let tiles = parse_tiles(snapshot);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].width, 426.0);
        assert_eq!(tiles[0].height, 720.0);
        assert_eq!(tiles[0].window_id, 1);
    }

    #[test]
    fn test_calculate_rtl_positions() {
        let ltr_snapshot = r"
view_width=1280
view_height=720
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(0.33333333333333337) active_tile=0
  tile[0]: w=426 h=720 window_id=1
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].left, 854.0);
        assert_eq!(positions[0].right, 1280.0);
        assert_eq!(positions[0].width, 426.0);
    }
}
