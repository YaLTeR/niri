use insta::assert_snapshot;

use super::*;

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
// EDGE TESTS - Leading Edge Pinning (LTR = Left Edge)
// ============================================================================

#[test]
fn leading_edge_pinned_on_spawn() {
    let mut layout = set_up_empty();

    // Spawn first window - left edge should be at 0
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");

    // Spawn second window - first window's left edge still at 0
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    // View offset should be negative, meaning first column's left edge is still at 0
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(426.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn leading_edge_pinned_on_resize() {
    let mut layout = set_up_empty();

    // Spawn window at 1/3 width
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Resize to 1/2 - left edge should stay at 0
    let ops = [
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

    // Resize to 2/3 - left edge STILL at 0
    let ops = [
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
}

#[test]
fn leading_edge_pinned_with_multiple_columns() {
    let mut layout = set_up_empty();

    // Create 3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus first column
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // First column's left edge should be at 0
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");

    // Resize first column - its left edge should stay at 0
    let ops = [
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
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn leading_edge_pinned_middle_column() {
    let mut layout = set_up_empty();

    // Create 3 columns, focus middle
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::FocusColumnLeft,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Resize middle column - its left edge should stay pinned
    let ops = [
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    // Middle column left edge was at 426, should still be at 426
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(426.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

// ============================================================================
// EDGE TESTS - Column X Position Calculations
// ============================================================================

#[test]
fn column_x_positions_single_column() {
    let mut layout = set_up_empty();

    // Single column at 1/3 width
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Column 0 should be at x=0
    // View offset = 0, so column renders at x=0
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");
}

#[test]
fn column_x_positions_two_columns() {
    let mut layout = set_up_empty();

    // Two columns at 1/3 width each
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Column 0 at x=0, Column 1 at x=426
    // View offset = -426, so column 1 renders at x=0 on screen
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(426.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn column_x_positions_three_columns() {
    let mut layout = set_up_empty();

    // Three columns at 1/3 width each
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Column 0 at x=0, Column 1 at x=426, Column 2 at x=852
    // View offset adjusts so column 2 is visible
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(852.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn column_x_positions_mixed_widths() {
    let mut layout = set_up_empty();

    // Mixed widths: 200px, 50%, 300px
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(200)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetFixed(300)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Column 0 at x=0 (200px)
    // Column 1 at x=200 (640px)
    // Column 2 at x=840 (300px)
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(840.0)
    Active Column: 2
    Column 0: width=Fixed(200.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 200.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 640.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(300.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 300.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn column_x_positions_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Three columns with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // With gaps, columns are spaced further apart
    // Gap before first column: 16
    // Column 0: 16 to 435 (width 419)
    // Gap: 16
    // Column 1: 451 to 870 (width 419)
    // Gap: 16
    // Column 2: 886 to 1305 (width 419)
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(826.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 405.0, h: 688.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 405.0, h: 688.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 405.0, h: 688.0 }, window_id=3
    ");
}

// ============================================================================
// EDGE TESTS - View Offset Clamping
// ============================================================================

#[test]
fn view_offset_clamped_at_zero() {
    let mut layout = set_up_empty();

    // Single column - view offset should be 0
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");
}

#[test]
fn view_offset_clamped_with_small_columns() {
    let mut layout = set_up_empty();

    // Two small columns (total < view width)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(200)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetFixed(200)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Total width = 400px < 1280px view
    // View offset should not go negative beyond what's needed
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(200.0)
    Active Column: 1
    Column 0: width=Fixed(200.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 200.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(200.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 200.0, h: 720.0 }, window_id=2
    ");
}

#[test]
fn view_offset_with_overflow() {
    let mut layout = set_up_empty();

    // Four 1/3 columns (creates overflow)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // View offset should show the active (last) column
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(1278.0)
    Active Column: 3
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");
}

// ============================================================================
// EDGE TESTS - Preset Width Index Tracking
// ============================================================================

#[test]
fn preset_width_index_cycles_correctly() {
    let mut layout = set_up_empty();

    // Start at default (1/3)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");

    // Cycle to 1/2
    let ops = [
        Op::SwitchPresetColumnWidth,
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

    // Cycle to 2/3
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.6666666666666665), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 853.0, h: 720.0 }, window_id=1
    ");

    // Cycle back to 1/3
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333326), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    ");
}

#[test]
fn preset_width_index_cycles_backward() {
    let mut layout = set_up_empty();

    // Start at 1/3
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Cycle backward to 2/3
    let ops = [
        Op::SwitchPresetColumnWidthBack,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Proportion(0.6666666666666665), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 853.0, h: 720.0 }, window_id=1
    ");

    // Cycle backward to 1/2
    let ops = [
        Op::SwitchPresetColumnWidthBack,
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
}
