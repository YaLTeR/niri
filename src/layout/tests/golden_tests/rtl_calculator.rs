//! LTR to RTL transformation calculator
//!
//! This module provides utilities to calculate expected RTL geometry from LTR snapshots.
//! RTL is a mathematical mirror transformation of LTR, not a separate specification.

/// Complete snapshot metadata needed for RTL calculation
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotMetadata {
    pub view_width: f64,
    pub view_height: f64,
    pub scale: f64,
    pub working_area_x: f64,
    pub working_area_y: f64,
    pub working_area_width: f64,
    pub working_area_height: f64,
    pub parent_area_x: f64,
    pub parent_area_y: f64,
    pub parent_area_width: f64,
    pub parent_area_height: f64,
    pub gaps: f64,
}

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

/// Parse a single f64 value from snapshot by key.
fn parse_f64(snapshot: &str, key: &str) -> Option<f64> {
    for line in snapshot.lines() {
        if let Some(value) = line.strip_prefix(key) {
            return value.trim().parse::<f64>().ok();
        }
    }
    None
}

/// Parse complete snapshot metadata.
/// Returns None if any required field is missing.
pub fn parse_snapshot_metadata(snapshot: &str) -> Option<SnapshotMetadata> {
    Some(SnapshotMetadata {
        view_width: parse_f64(snapshot, "view_width=")?,
        view_height: parse_f64(snapshot, "view_height=")?,
        scale: parse_f64(snapshot, "scale=")?,
        working_area_x: parse_f64(snapshot, "working_area_x=")?,
        working_area_y: parse_f64(snapshot, "working_area_y=")?,
        working_area_width: parse_f64(snapshot, "working_area_width=")?,
        working_area_height: parse_f64(snapshot, "working_area_height=")?,
        parent_area_x: parse_f64(snapshot, "parent_area_x=")?,
        parent_area_y: parse_f64(snapshot, "parent_area_y=")?,
        parent_area_width: parse_f64(snapshot, "parent_area_width=")?,
        parent_area_height: parse_f64(snapshot, "parent_area_height=")?,
        gaps: parse_f64(snapshot, "gaps=")?,
    })
}

/// Calculate expected RTL positions from LTR snapshot.
/// 
/// For single-column layouts, assumes LTR tiles start at working_area.x (left-aligned).
/// RTL tiles should be right-aligned within the working area.
/// 
/// All metadata is parsed from the snapshot for deterministic calculation.
pub fn calculate_rtl_positions(ltr_snapshot: &str) -> Vec<RtlPosition> {
    let tiles = parse_tiles(ltr_snapshot);
    let metadata = parse_snapshot_metadata(ltr_snapshot)
        .expect("Failed to parse snapshot metadata - ensure snapshot includes all required fields");
    
    tiles.iter().map(|tile| {
        // In LTR, single column starts at working_area.x (typically 0)
        // In RTL, it should be right-aligned within working area
        let ltr_x = metadata.working_area_x;
        
        // Mirror within the working area
        let working_right = metadata.working_area_x + metadata.working_area_width;
        let rtl_left = working_right - tile.width;
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
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=0
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
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=0
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(0.33333333333333337) active_tile=0
  tile[0]: w=426 h=720 window_id=1
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 1);
        // RTL: right-aligned in working area (0 + 1280 - 426 = 854)
        assert_eq!(positions[0].left, 854.0);
        assert_eq!(positions[0].right, 1280.0);
        assert_eq!(positions[0].width, 426.0);
    }

    #[test]
    fn test_rtl_with_struts() {
        // Working area is smaller due to struts
        let ltr_snapshot = r"
view_width=1280
view_height=720
scale=1
working_area_x=50
working_area_y=30
working_area_width=1180
working_area_height=660
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=0
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(0.5) active_tile=0
  tile[0]: w=590 h=660 window_id=1
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 1);
        // RTL: right-aligned in working area (50 + 1180 - 590 = 640)
        assert_eq!(positions[0].left, 640.0);
        assert_eq!(positions[0].right, 1230.0);
        assert_eq!(positions[0].width, 590.0);
    }

    #[test]
    fn test_rtl_with_gaps() {
        let ltr_snapshot = r"
view_width=1280
view_height=720
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=16
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(0.5) active_tile=0
  tile[0]: w=632 h=720 window_id=1
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 1);
        // RTL: right-aligned in working area (0 + 1280 - 632 = 648)
        assert_eq!(positions[0].left, 648.0);
        assert_eq!(positions[0].right, 1280.0);
        assert_eq!(positions[0].width, 632.0);
    }

    #[test]
    fn test_rtl_fixed_width() {
        let ltr_snapshot = r"
view_width=1280
view_height=720
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=0
view_offset=Static(0.0)
active_column=0
column[0]: width=Fixed(400.0) active_tile=0
  tile[0]: w=400 h=720 window_id=1
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 1);
        // RTL: right-aligned (0 + 1280 - 400 = 880)
        assert_eq!(positions[0].left, 880.0);
        assert_eq!(positions[0].right, 1280.0);
        assert_eq!(positions[0].width, 400.0);
    }

    #[test]
    fn test_rtl_full_width() {
        let ltr_snapshot = r"
view_width=1280
view_height=720
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=0
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(1.0) active_tile=0
  tile[0]: w=1280 h=720 window_id=1
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 1);
        // RTL: full width, same position as LTR
        assert_eq!(positions[0].left, 0.0);
        assert_eq!(positions[0].right, 1280.0);
        assert_eq!(positions[0].width, 1280.0);
    }

    #[test]
    fn test_rtl_hidpi_scale() {
        let ltr_snapshot = r"
view_width=1920
view_height=1080
scale=2
working_area_x=0
working_area_y=0
working_area_width=1920
working_area_height=1080
parent_area_x=0
parent_area_y=0
parent_area_width=1920
parent_area_height=1080
gaps=0
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(0.5) active_tile=0
  tile[0]: w=960 h=1080 window_id=1
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 1);
        // RTL: right-aligned (0 + 1920 - 960 = 960)
        assert_eq!(positions[0].left, 960.0);
        assert_eq!(positions[0].right, 1920.0);
        assert_eq!(positions[0].width, 960.0);
    }

    #[test]
    fn test_parse_metadata_complete() {
        let snapshot = r"
view_width=1280
view_height=720
scale=1.5
working_area_x=10
working_area_y=20
working_area_width=1260
working_area_height=680
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=8
view_offset=Static(0.0)
active_column=0
";
        
        let metadata = parse_snapshot_metadata(snapshot).unwrap();
        assert_eq!(metadata.view_width, 1280.0);
        assert_eq!(metadata.view_height, 720.0);
        assert_eq!(metadata.scale, 1.5);
        assert_eq!(metadata.working_area_x, 10.0);
        assert_eq!(metadata.working_area_y, 20.0);
        assert_eq!(metadata.working_area_width, 1260.0);
        assert_eq!(metadata.working_area_height, 680.0);
        assert_eq!(metadata.parent_area_x, 0.0);
        assert_eq!(metadata.parent_area_y, 0.0);
        assert_eq!(metadata.parent_area_width, 1280.0);
        assert_eq!(metadata.parent_area_height, 720.0);
        assert_eq!(metadata.gaps, 8.0);
    }

    #[test]
    fn test_parse_metadata_missing_field() {
        let snapshot = r"
view_width=1280
view_height=720
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
";
        // Missing gaps field
        assert!(parse_snapshot_metadata(snapshot).is_none());
    }

    #[test]
    fn test_multiple_tiles() {
        let ltr_snapshot = r"
view_width=1280
view_height=720
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=0
view_offset=Static(0.0)
active_column=0
column[0]: width=Proportion(0.5) active_tile=0
  tile[0]: w=640 h=360 window_id=1
  tile[1]: w=640 h=360 window_id=2
";
        
        let positions = calculate_rtl_positions(ltr_snapshot);
        assert_eq!(positions.len(), 2);
        // Both tiles in same column, same x position
        assert_eq!(positions[0].left, 640.0);
        assert_eq!(positions[0].right, 1280.0);
        assert_eq!(positions[1].left, 640.0);
        assert_eq!(positions[1].right, 1280.0);
    }

    #[test]
    fn test_mirror_x_symmetry() {
        let view_width = 1280.0;
        
        // Centered tile should stay centered
        let centered_width = 640.0;
        let centered_x = 320.0;
        let rtl_x = mirror_x(centered_x, centered_width, view_width);
        assert_eq!(rtl_x, 320.0); // Symmetric
        
        // Left tile mirrors to right
        let left_x = 0.0;
        let width = 400.0;
        let rtl_x = mirror_x(left_x, width, view_width);
        assert_eq!(rtl_x, 880.0);
        
        // Right tile mirrors to left
        let right_x = 880.0;
        let rtl_x = mirror_x(right_x, width, view_width);
        assert_eq!(rtl_x, 0.0);
    }
}

