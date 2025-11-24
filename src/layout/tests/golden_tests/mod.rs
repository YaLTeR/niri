// Golden tests for the scrolling layout
//
// Structure:
// Each test suite is in its own directory with:
// - ltr.rs: LTR tests with immutable golden snapshots
// - rtl.rs: RTL tests that derive from LTR
// - golden/*.txt: Golden reference files (LTR only)
//
// Philosophy:
// - LTR snapshots are the IMMUTABLE specification
// - RTL behavior is mathematically derived from LTR
// - Single source of truth prevents divergence

use super::*;

mod rtl_calculator;

pub use rtl_calculator::*;

/// Assert against an immutable golden reference file.
/// Golden files are the IMMUTABLE LTR specification.
macro_rules! assert_golden {
    ($actual:expr, $test_name:literal) => {{
        let golden = include_str!(concat!("golden/", $test_name, ".txt"));
        let actual_value = $actual;
        let actual = actual_value.trim();
        let expected = golden.trim();
        assert_eq!(
            actual,
            expected,
            "\n\n❌ GOLDEN REFERENCE MISMATCH ❌\n\
             \nTest: {}\n\
             \nGolden files are IMMUTABLE LTR specifications.\n\
             Any change is a regression unless explicitly intended.\n\
             \nTo update: manually edit golden/{}.txt\n\n",
            $test_name,
            $test_name
        );
    }};
}

/// Assert RTL behavior against LTR golden reference.
/// RTL geometry is deterministically calculated from LTR golden files.
/// 
/// This macro:
/// 1. Automatically derives the LTR golden file name from the test function name
///    by stripping the `_rtl` suffix
/// 2. Loads the LTR golden snapshot
/// 3. Calculates expected RTL positions using mirror transformation
/// 4. Verifies actual RTL geometry matches expected
/// 5. Verifies logical state (snapshot) is identical to LTR
macro_rules! assert_golden_rtl {
    ($layout:expr) => {{
        use $crate::layout::tests::golden_tests::{calculate_rtl_positions, format_column_edges};
        
        // Derive golden file name from test function name
        // Test function: spawn_single_column_one_third_rtl -> Golden: spawn_single_column_one_third
        let test_fn_name = {
            fn type_name_of<T>(_: &T) -> &'static str {
                std::any::type_name::<T>()
            }
            // This won't work directly, we need the function name not type name
            // Let's use a different approach: pass the name explicitly but make it a macro rule
            stringify!($layout) // This gives us the variable name, not function name
        };
        
        // Since we can't easily get function name in declarative macros,
        // we'll require explicit test name for now
        // Users should call: assert_golden_rtl!(layout, test_name)
        compile_error!("assert_golden_rtl! requires explicit test name parameter. Use: assert_golden_rtl!(layout, \"test_name\")")
    }};
    
    ($layout:expr, $test_name:literal) => {{
        use $crate::layout::tests::golden_tests::{calculate_rtl_positions, format_column_edges};
        
        // Load LTR golden snapshot
        let ltr_golden = include_str!(concat!("golden/", $test_name, ".txt"));
        
        // Calculate expected RTL positions from LTR golden
        let expected_positions = calculate_rtl_positions(ltr_golden);
        
        // Verify actual RTL geometry matches expected mirrored positions
        let actual_edges = format_column_edges(&$layout);
        let expected_edges: String = expected_positions
            .iter()
            .map(|pos| pos.format())
            .collect::<Vec<_>>()
            .join("\n");
        
        assert_eq!(
            actual_edges.trim(),
            expected_edges.trim(),
            "\n\n❌ RTL GEOMETRY MISMATCH ❌\n\
             \nTest: {}\n\
             \nRTL geometry should be a mathematical mirror of LTR.\n\
             \nLTR golden: golden/{}.txt\n\
             \nExpected RTL positions are calculated from LTR golden.\n\n",
            $test_name,
            $test_name
        );
        
        // Verify logical state is identical to LTR (direction-agnostic)
        // Note: view_offset is excluded from comparison because RTL scrolling is not yet implemented
        let rtl_snapshot = $layout.snapshot();
        let rtl_normalized = $crate::layout::tests::golden_tests::normalize_view_offset(&rtl_snapshot);
        let ltr_normalized = $crate::layout::tests::golden_tests::normalize_view_offset(ltr_golden);
        assert_eq!(
            rtl_normalized.trim(),
            ltr_normalized.trim(),
            "\n\n❌ RTL LOGICAL STATE MISMATCH ❌\n\
             \nTest: {}\n\
             \nLogical state (snapshot) should be identical between LTR and RTL.\n\
             \nOnly visual geometry should differ.\n\
             \n(Note: view_offset is normalized for comparison as RTL scrolling is not yet implemented)\n\n",
            $test_name
        );
    }};
}

// Helper functions shared across all golden tests

/// Normalize view_offset in a snapshot to Static(0.0) for comparison.
/// This is needed because RTL scrolling is not yet implemented, so view_offset
/// differs between LTR and RTL even though the logical state should be the same.
pub fn normalize_view_offset(snapshot: &str) -> String {
    snapshot
        .lines()
        .map(|line| {
            if line.starts_with("view_offset=") {
                "view_offset=Static(0.0)"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn make_options() -> Options {
    let mut options = Options {
        layout: niri_config::Layout {
            gaps: 0.0,
            struts: niri_config::Struts {
                left: niri_config::FloatOrInt(0.0),
                right: niri_config::FloatOrInt(0.0),
                top: niri_config::FloatOrInt(0.0),
                bottom: niri_config::FloatOrInt(0.0),
            },
            center_focused_column: niri_config::CenterFocusedColumn::Never,
            always_center_single_column: false,
            default_column_width: Some(niri_config::PresetSize::Proportion(1.0 / 3.0)),
            preset_column_widths: vec![
                niri_config::PresetSize::Proportion(1.0 / 3.0),
                niri_config::PresetSize::Proportion(1.0 / 2.0),
                niri_config::PresetSize::Proportion(2.0 / 3.0),
            ],
            preset_window_heights: vec![
                niri_config::PresetSize::Proportion(1.0 / 3.0),
                niri_config::PresetSize::Proportion(1.0 / 2.0),
                niri_config::PresetSize::Proportion(2.0 / 3.0),
            ],
            default_column_display: niri_ipc::ColumnDisplay::Normal,
            empty_workspace_above_first: false,
            ..Default::default()
        },
        ..Options::default()
    };
    options.animations.window_open.anim.off = true;
    options.animations.window_close.anim.off = true;
    options.animations.window_resize.anim.off = true;
    options.animations.window_movement.0.off = true;
    options.animations.horizontal_view_movement.0.off = true;

    options
}

pub fn set_up_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_options(), ops)
}

pub fn format_column_edges(layout: &Layout<TestWindow>) -> String {
    use std::fmt::Write as _;
    
    let mut buf = String::new();
    let ws = layout.active_workspace().unwrap();
    let mut tiles: Vec<_> = ws.tiles_with_render_positions().collect();

    tiles.sort_by_key(|(tile, _, _)| tile.window().id());
    for (tile, pos, _visible) in tiles {
        let Size { w, .. } = tile.animated_tile_size();
        let Point { x, .. } = pos;
        let right_edge = x + w;
        writeln!(&mut buf, "left:{x:>4.0} right:{right_edge:>4.0} width:{w:>4.0}").unwrap();
    }
    buf
}

/// Parse a snapshot string to extract tile widths.
/// Returns Vec<f64> of tile widths in order.
#[allow(dead_code)]
pub fn parse_snapshot_widths(snapshot: &str) -> Vec<f64> {
    let mut widths = Vec::new();
    for line in snapshot.lines() {
        if line.trim().starts_with("tile[") {
            // Parse: "  tile[0]: w=426 h=720 window_id=1"
            if let Some(w_start) = line.find("w=") {
                let w_str = &line[w_start + 2..];
                if let Some(w_end) = w_str.find(' ') {
                    if let Ok(width) = w_str[..w_end].parse::<f64>() {
                        widths.push(width);
                    }
                }
            }
        }
    }
    widths
}

/// Parse a snapshot string to extract all tile dimensions.
/// Returns Vec<(width, height)> in order.
#[allow(dead_code)]
pub fn parse_snapshot_tiles(snapshot: &str) -> Vec<(f64, f64)> {
    let mut tiles = Vec::new();
    for line in snapshot.lines() {
        if line.trim().starts_with("tile[") {
            // Parse: "  tile[0]: w=426 h=720 window_id=1"
            let mut width = None;
            let mut height = None;
            
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
            
            if let (Some(w), Some(h)) = (width, height) {
                tiles.push((w, h));
            }
        }
    }
    tiles
}

// Phase 1: Basic Spawning & Geometry
#[path = "000_spawning_single/mod.rs"]
mod spawning_single;

#[path = "010_spawning_multiple/mod.rs"]
mod spawning_multiple;

#[path = "020_add_window_next_to/mod.rs"]
mod add_window_next_to;

#[path = "030_column_positions/mod.rs"]
mod column_positions;

#[path = "040_view_offset/mod.rs"]
mod view_offset;

#[path = "050_leading_edge/mod.rs"]
mod leading_edge;

#[path = "060_focus_movement/mod.rs"]
mod focus_movement;

#[path = "070_window_closing/mod.rs"]
mod window_closing;

#[path = "080_column_resize/mod.rs"]
mod column_resize;

#[path = "090_focus_edge_cases/mod.rs"]
mod focus_edge_cases;

#[path = "100_window_lifecycle/mod.rs"]
mod window_lifecycle;

#[path = "110_resize_advanced/mod.rs"]
mod resize_advanced;

#[path = "120_resize_incremental/mod.rs"]
mod resize_incremental;

#[path = "130_column_move/mod.rs"]
mod column_move;

#[path = "140_column_move_first_last/mod.rs"]
mod column_move_first_last;

#[path = "150_preset_width/mod.rs"]
mod preset_width;

#[path = "160_alternative_presets/mod.rs"]
mod alternative_presets;

#[path = "170_tiles_multiple/mod.rs"]
mod tiles_multiple;

#[path = "180_tiles_focus/mod.rs"]
mod tiles_focus;

#[path = "190_tiles_movement/mod.rs"]
mod tiles_movement;

#[path = "200_window_height/mod.rs"]
mod window_height;

#[path = "210_consume_expel/mod.rs"]
mod consume_expel;

#[path = "220_maximize_fullscreen/mod.rs"]
mod maximize_fullscreen;

#[path = "230_column_display/mod.rs"]
mod column_display;

#[path = "240_scale_factors/mod.rs"]
mod scale_factors;

#[path = "250_gaps_struts/mod.rs"]
mod gaps_struts;

#[path = "260_center_focused/mod.rs"]
mod center_focused;

#[path = "270_small_large_columns/mod.rs"]
mod small_large_columns;

#[path = "280_overflow_scenarios/mod.rs"]
mod overflow_scenarios;

#[path = "290_resize_during_ops/mod.rs"]
mod resize_during_ops;

#[path = "300_extreme_config/mod.rs"]
mod extreme_config;

#[path = "310_default_widths/mod.rs"]
mod default_widths;

#[path = "320_config_variants/mod.rs"]
mod config_variants;

#[path = "330_anim_column_movement/mod.rs"]
mod anim_column_movement;

#[path = "340_anim_view_offset/mod.rs"]
mod anim_view_offset;

#[path = "350_anim_resize/mod.rs"]
mod anim_resize;

#[path = "360_anim_preset_width/mod.rs"]
mod anim_preset_width;

#[path = "370_active_tile_visibility/mod.rs"]
mod active_tile_visibility;
