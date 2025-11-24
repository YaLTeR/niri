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

// ============================================================================
// CONFIG TESTS - Different Scale Factors
// ============================================================================

#[test]
fn scale_factor_1_5() {
    let options = make_options();
    
    let ops = [
        Op::AddScaledOutput {
            id: 1,
            scale: 1.5,
            layout_config: None,
        },
    ];
    let mut layout = check_ops_with_options(options, ops);

    // Spawn windows with 1.5x scale
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
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 284.0, h: 480.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 284.0, h: 480.0 }, window_id=2
    ");
}

#[test]
fn scale_factor_2_0() {
    let options = make_options();
    
    let ops = [
        Op::AddScaledOutput {
            id: 1,
            scale: 2.0,
            layout_config: None,
        },
    ];
    let mut layout = check_ops_with_options(options, ops);

    // Spawn windows with 2.0x scale
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-100.0)
    Active Column: 1
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 320.0, h: 360.0 }, window_id=1
    Column 1: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 320.0, h: 360.0 }, window_id=2
    ");
}

#[test]
fn scale_factor_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    
    let ops = [
        Op::AddScaledOutput {
            id: 1,
            scale: 1.5,
            layout_config: None,
        },
    ];
    let mut layout = check_ops_with_options(options, ops);

    // Gaps should be scaled appropriately
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
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-132.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 263.3333333333333, h: 448.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 263.3333333333333, h: 448.0 }, window_id=2
    ");
}

// ============================================================================
// CONFIG TESTS - Very Small View Sizes
// ============================================================================

#[test]
fn very_small_view_width() {
    let options = make_options();
    
    // This would require modifying the output size, which isn't directly exposed
    // Instead, test with very small column widths
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Create very narrow columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(50)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetFixed(50)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetFixed(50)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(50.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 50.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(50.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 50.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(50.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 50.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn very_small_column_with_proportion() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(0.05));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 5% width columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    ");
}

// ============================================================================
// CONFIG TESTS - Zero Gaps Explicit
// ============================================================================

#[test]
fn zero_gaps_explicit() {
    let mut options = make_options();
    options.layout.gaps = 0.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // With zero gaps, columns should be adjacent
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
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

// ============================================================================
// CONFIG TESTS - Column Removal During Operations
// ============================================================================

#[test]
fn close_column_while_resizing() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

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

    // Start resizing middle column, then close it
    let ops = [
        Op::FocusColumnLeft,
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-0.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

#[test]
fn close_column_while_moving() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

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

    // Move first column, then close second
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::MoveColumnRight,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-426.0)
    Active Column: 0
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    ");
}

// ============================================================================
// CONFIG TESTS - Extreme Configurations
// ============================================================================

#[test]
fn very_large_gaps() {
    let mut options = make_options();
    options.layout.gaps = 100.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // With 100px gaps, columns are far apart
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
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-300.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 293.0, h: 520.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 293.0, h: 520.0 }, window_id=2
    ");
}

#[test]
fn very_large_struts() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(200.0),
        right: niri_config::FloatOrInt(200.0),
        top: niri_config::FloatOrInt(100.0),
        bottom: niri_config::FloatOrInt(100.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // With large struts, working area is much smaller
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-200.0)
    Active Column: 0
    Column 0: width=Proportion(0.5), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 440.0, h: 520.0 }, window_id=1
    ");
}

#[test]
fn many_columns_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Fixed(200));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Create 10 columns (way more than fits)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::AddWindow { params: TestWindowParams::new(5) },
        Op::AddWindow { params: TestWindowParams::new(6) },
        Op::AddWindow { params: TestWindowParams::new(7) },
        Op::AddWindow { params: TestWindowParams::new(8) },
        Op::AddWindow { params: TestWindowParams::new(9) },
        Op::AddWindow { params: TestWindowParams::new(10) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::Communicate(5),
        Op::Communicate(6),
        Op::Communicate(7),
        Op::Communicate(8),
        Op::Communicate(9),
        Op::Communicate(10),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Should show the last column
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(-900.0)
    Active Column: 9
    Column 0: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=1
    Column 1: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=2
    Column 2: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=3
    Column 3: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=4
    Column 4: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=5
    Column 5: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=6
    Column 6: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=7
    Column 7: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=8
    Column 8: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=9
    Column 9: width=Fixed(100.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 100.0, h: 720.0 }, window_id=10
    ");
}

#[test]
fn single_very_wide_column() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Create a column wider than the view
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(2000)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 0
    Column 0: width=Fixed(2000.0), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 2000.0, h: 720.0 }, window_id=1
    ");
}
