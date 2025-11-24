use insta::assert_snapshot;

use super::*;

// RTL mirroring calculator
//
// The output is 1280x720 pixels. In LTR mode, columns start at x=0 and grow rightward.
// In RTL mode, columns should start at x=1280 and grow leftward.
//
// For a tile at LTR position x with width w:
// - LTR: left edge = x, right edge = x + w
// - RTL: right edge = 1280 - x, left edge = 1280 - x - w
//
// This means:
// - A tile at x=0, w=426 (1/3) in LTR should be at x=854, w=426 in RTL (right-aligned)
// - A tile at x=0, w=640 (1/2) in LTR should be at x=640, w=640 in RTL (right-aligned)
// - A tile at x=0, w=853 (2/3) in LTR should be at x=427, w=853 in RTL (right-aligned)

const OUTPUT_WIDTH: f64 = 1280.0;

fn mirror_x(ltr_x: f64, width: f64) -> f64 {
    OUTPUT_WIDTH - ltr_x - width
}

fn make_options() -> Options {
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
            // Enable RTL mode
            right_to_left: true,
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

fn set_up_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_options(), ops)
}

// ============================================================================
// BASIC SPAWNING - Single Column (RTL Mirrored)
// ============================================================================

#[test]
fn spawn_single_column_one_third_rtl() {
    let mut layout = set_up_empty();

    // Spawn a 1/3 tile, should be right-aligned
    // LTR: x=0, w=426 -> RTL: x=854, w=426
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Expected: Column should be right-aligned at x=854
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");
    
    // Verify actual position using format_column_edges
    let edges = format_column_edges(&layout);
    assert_snapshot!(edges, @"left: 854 right:1280 width: 426");
}

#[test]
fn spawn_single_column_one_half_rtl() {
    let mut layout = set_up_empty();

    // Spawn a 1/2 tile, should be right-aligned
    // LTR: x=0, w=640 -> RTL: x=640, w=640
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=1
    ");
    
    let edges = format_column_edges(&layout);
    assert_snapshot!(edges, @"left: 640 right:1280 width: 640");
}

#[test]
fn spawn_single_column_two_thirds_rtl() {
    let mut layout = set_up_empty();

    // Spawn a 2/3 tile, should be right-aligned
    // LTR: x=0, w=853 -> RTL: x=427, w=853
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(200.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.6666666666666667), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 853.0, h: 720.0 }, window_id=1
    ");
    
    let edges = format_column_edges(&layout);
    assert_snapshot!(edges, @"left: 427 right:1280 width: 853");
}

#[test]
fn spawn_single_column_fixed_width_rtl() {
    let mut layout = set_up_empty();

    // Spawn with fixed width, should be right-aligned
    // LTR: x=0, w=400 -> RTL: x=880, w=400
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(400)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(400.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 400.0, h: 720.0 }, window_id=1
    ");
    
    let edges = format_column_edges(&layout);
    assert_snapshot!(edges, @"left: 880 right:1280 width: 400");
}

#[test]
fn column_x_positions_single_column_rtl() {
    let mut layout = set_up_empty();

    // Single column at 1/3 width, should be right-aligned
    // LTR: x=0, w=426 -> RTL: x=854, w=426
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Column 0 should be at x=854 (right-aligned)
    // View offset = 0, so column renders at x=854
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");
    
    let edges = format_column_edges(&layout);
    assert_snapshot!(edges, @"left: 854 right:1280 width: 426");
}
