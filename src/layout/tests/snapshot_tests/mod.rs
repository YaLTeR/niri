// Snapshot tests for the scrolling layout, organized by RTL implementation complexity
//
// This module contains 33 numbered test files, each focusing on a specific aspect
// of the scrolling layout. The files are ordered by RTL implementation complexity,
// starting with basic spawning and geometry, progressing through focus and navigation,
// column operations, tiles and stacking, advanced features, edge cases, and finally
// animations.
//
// Each file contains LTR tests that will later be mirrored with RTL variants.

use super::*;

// Helper functions shared across all test files
pub fn make_options() -> Options {
    let mut options = Options {
        layout: niri_config::Layout {
            // Explicitly set all layout options to known values for comprehensive testing
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
    // Disable animations for these tests to make snapshots deterministic and immediate
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

// Phase 1: Basic Spawning & Geometry (Files 00-05)
#[path = "00_ltr_spawning_single.rs"]
mod ltr_spawning_single;
#[path = "00_rtl.rs"]
mod rtl;
#[path = "01_ltr_spawning_multiple.rs"]
mod ltr_spawning_multiple;
#[path = "02_ltr_add_window_next_to.rs"]
mod ltr_add_window_next_to;
#[path = "03_ltr_column_positions.rs"]
mod ltr_column_positions;
#[path = "04_ltr_view_offset.rs"]
mod ltr_view_offset;
#[path = "05_ltr_leading_edge.rs"]
mod ltr_leading_edge;

// Phase 2: Focus & Navigation (Files 06-10)
#[path = "06_ltr_focus_movement.rs"]
mod ltr_focus_movement;
#[path = "07_ltr_window_closing.rs"]
mod ltr_window_closing;
#[path = "08_ltr_column_resize.rs"]
mod ltr_column_resize;
#[path = "09_ltr_focus_edge_cases.rs"]
mod ltr_focus_edge_cases;
#[path = "10_ltr_window_lifecycle.rs"]
mod ltr_window_lifecycle;

// Phase 3: Column Operations (Files 11-16)
#[path = "11_ltr_resize_advanced.rs"]
mod ltr_resize_advanced;
#[path = "12_ltr_resize_incremental.rs"]
mod ltr_resize_incremental;
#[path = "13_ltr_column_move.rs"]
mod ltr_column_move;
#[path = "14_ltr_column_move_first_last.rs"]
mod ltr_column_move_first_last;
#[path = "15_ltr_preset_width.rs"]
mod ltr_preset_width;
#[path = "16_ltr_alternative_presets.rs"]
mod ltr_alternative_presets;

// Phase 4: Tiles & Stacking (Files 17-21)
#[path = "17_ltr_tiles_multiple.rs"]
mod ltr_tiles_multiple;
#[path = "18_ltr_tiles_focus.rs"]
mod ltr_tiles_focus;
#[path = "19_ltr_tiles_movement.rs"]
mod ltr_tiles_movement;
#[path = "20_ltr_window_height.rs"]
mod ltr_window_height;
#[path = "21_ltr_consume_expel.rs"]
mod ltr_consume_expel;

// Phase 5: Advanced Features (Files 22-26)
#[path = "22_ltr_maximize_fullscreen.rs"]
mod ltr_maximize_fullscreen;
#[path = "23_ltr_column_display.rs"]
mod ltr_column_display;
#[path = "24_ltr_scale_factors.rs"]
mod ltr_scale_factors;
#[path = "25_ltr_gaps_struts.rs"]
mod ltr_gaps_struts;
#[path = "26_ltr_center_focused.rs"]
mod ltr_center_focused;

// Phase 6: Edge Cases & Config (Files 27-32)
#[path = "27_ltr_small_large_columns.rs"]
mod ltr_small_large_columns;
#[path = "28_ltr_overflow_scenarios.rs"]
mod ltr_overflow_scenarios;
#[path = "29_ltr_resize_during_ops.rs"]
mod ltr_resize_during_ops;
#[path = "30_ltr_extreme_config.rs"]
mod ltr_extreme_config;
#[path = "31_ltr_default_widths.rs"]
mod ltr_default_widths;
#[path = "32_ltr_config_variants.rs"]
mod ltr_config_variants;

// Phase 7: Animations (Files 33-36)
#[path = "33_ltr_anim_column_movement.rs"]
mod ltr_anim_column_movement;
#[path = "34_ltr_anim_view_offset.rs"]
mod ltr_anim_view_offset;
#[path = "35_ltr_anim_resize.rs"]
mod ltr_anim_resize;
#[path = "36_ltr_anim_preset_width.rs"]
mod ltr_anim_preset_width;
