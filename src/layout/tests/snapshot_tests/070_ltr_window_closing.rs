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

fn set_up_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_options(), ops)
}

// ============================================================================
// WINDOW CLOSING - Focus Changes
// ============================================================================

#[test]
fn window_closing() {
    let mut layout = set_up_empty();

    // 1) Spawn 3 windows (full view)
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
    view_offset=Static(-200.0)
    view_pos=652.0
    active_column=2
    active_column_x=852.0
    active_tile_viewport_x=200.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=0.0 y=0.0 w=426 h=720 window_id=1
    column[1]: x=426.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=426.0 y=0.0 w=426 h=720 window_id=2
    column[2] [ACTIVE]: x=852.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=852.0 y=0.0 w=426 h=720 window_id=3
    ");

    // 2) Close window 2 (middle).
    let ops = [
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-200.0)
    view_pos=226.0
    active_column=1
    active_column_x=426.0
    active_tile_viewport_x=200.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=0.0 y=0.0 w=426 h=720 window_id=1
    column[1] [ACTIVE]: x=426.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=426.0 y=0.0 w=426 h=720 window_id=3
    ");
}

#[test]
fn closing_rightmost_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles - creates overflow
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
    view_offset=Static(-200.0)
    view_pos=0.0
    active_column=2
    active_column_x=200.0
    active_tile_viewport_x=200.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=100.0 y=0.0 w=100 h=720 window_id=2
    column[2] [ACTIVE]: x=200.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=200.0 y=0.0 w=100 h=720 window_id=3
    ");

    // Close rightmost column (3) - focus should move to column 2
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-100.0)
    view_pos=0.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=100.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2
    ");
}

#[test]
fn closing_middle_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles
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

    // Focus middle column
    let ops = [
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-100.0)
    view_pos=0.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=100.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2
    column[2]: x=200.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=200.0 y=0.0 w=100 h=720 window_id=3
    ");

    // Close middle column (2) - focus should move to column 3
    let ops = [
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-100.0)
    view_pos=0.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=100.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=3
    ");
}

#[test]
fn closing_leftmost_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles
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

    // Focus leftmost column
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_pos=0.0
    active_column=0
    active_column_x=0.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0] [ACTIVE]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=100.0 y=0.0 w=100 h=720 window_id=2
    column[2]: x=200.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=200.0 y=0.0 w=100 h=720 window_id=3
    ");

    // Close leftmost column (1) - focus should move to column 2
    let ops = [
        Op::CloseWindow(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_pos=0.0
    active_column=0
    active_column_x=0.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0] [ACTIVE]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=100 h=720 window_id=2
    column[1]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=100.0 y=0.0 w=100 h=720 window_id=3
    ");
}

#[test]
fn closing_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles with gaps
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
    view_offset=Static(-248.0)
    view_pos=-16.0
    active_column=2
    active_column_x=232.0
    active_tile_viewport_x=248.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=688 window_id=1
    column[1]: x=116.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=116.0 y=0.0 w=100 h=688 window_id=2
    column[2] [ACTIVE]: x=232.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=232.0 y=0.0 w=100 h=688 window_id=3
    ");

    // Close middle column - gaps should remain consistent
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-132.0)
    view_pos=-16.0
    active_column=1
    active_column_x=116.0
    active_tile_viewport_x=132.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=688 window_id=1
    column[1] [ACTIVE]: x=116.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=116.0 y=0.0 w=100 h=688 window_id=3
    ");
}

#[test]
fn closing_mixed_preset_sizes() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three columns with different sizes
    // Column 1: 2/5, Column 2: 3/5, Column 3: 2/5
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // Make column 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-200.0)
    view_pos=412.0
    active_column=2
    active_column_x=612.0
    active_tile_viewport_x=200.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1]: x=100.0 width=Proportion(0.4) active_tile=0
      tile[0]: x=100.0 y=0.0 w=512 h=720 window_id=2
    column[2] [ACTIVE]: x=612.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=612.0 y=0.0 w=100 h=720 window_id=3
    ");

    // Close the large middle column (3/5) - camera should adjust
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-0.0)
    view_pos=100.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=3
    ");
}

#[test]
fn closing_with_gaps_and_struts() {
    let mut options = make_options();
    options.layout.gaps = 12.0;
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(30.0),
        right: niri_config::FloatOrInt(30.0),
        top: niri_config::FloatOrInt(15.0),
        bottom: niri_config::FloatOrInt(15.0),
    };
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three columns with gaps and struts
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
    view_width=1280
    view_height=720
    scale=1
    working_area_x=30
    working_area_y=15
    working_area_width=1220
    working_area_height=690
    parent_area_x=0
    parent_area_y=0
    parent_area_width=1280
    parent_area_height=720
    gaps=12
    view_offset=Static(-266.0)
    view_pos=-42.0
    active_column=2
    active_column_x=224.0
    active_tile_viewport_x=266.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=666 window_id=1
    column[1]: x=112.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=112.0 y=0.0 w=100 h=666 window_id=2
    column[2] [ACTIVE]: x=224.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=224.0 y=0.0 w=100 h=666 window_id=3
    ");

    // Close rightmost column - gaps and struts should remain correct
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_width=1280
    view_height=720
    scale=1
    working_area_x=30
    working_area_y=15
    working_area_width=1220
    working_area_height=690
    parent_area_x=0
    parent_area_y=0
    parent_area_width=1280
    parent_area_height=720
    gaps=12
    view_offset=Static(-154.0)
    view_pos=-42.0
    active_column=1
    active_column_x=112.0
    active_tile_viewport_x=154.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=666 window_id=1
    column[1] [ACTIVE]: x=112.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=112.0 y=0.0 w=100 h=666 window_id=2
    ");
}

// ============================================================================
// Tests for closing with 1/3, 1/2, 2/3 preset sizes
// ============================================================================

#[test]
fn closing_first_of_three_thirds_preset() {
    let options = make_options();
    // Use default 1/3, 1/2, 2/3 presets
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-200.0)
    view_pos=326.0
    active_column=2
    active_column_x=526.0
    active_tile_viewport_x=200.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1]: x=100.0 width=Proportion(0.33333333333333326) active_tile=0
      tile[0]: x=100.0 y=0.0 w=426 h=720 window_id=2
    column[2] [ACTIVE]: x=526.0 width=Proportion(0.5) active_tile=0
      tile[0] [ACTIVE]: x=526.0 y=0.0 w=640 h=720 window_id=3
    ");

    // Focus and close first column (1/3)
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CloseWindow(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-0.0)
    view_pos=0.0
    active_column=0
    active_column_x=0.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0] [ACTIVE]: x=0.0 width=Proportion(0.33333333333333326) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=426 h=720 window_id=2
    column[1]: x=426.0 width=Proportion(0.5) active_tile=0
      tile[0]: x=426.0 y=0.0 w=640 h=720 window_id=3
    ");
}

#[test]
fn closing_middle_of_three_thirds_preset() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus and close middle column (1/2)
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-0.0)
    view_pos=100.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Proportion(0.5) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=640 h=720 window_id=3
    ");
}

#[test]
fn closing_last_of_three_thirds_preset() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Close last column (2/3) - already focused
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-100.0)
    view_pos=0.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=100.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Proportion(0.33333333333333326) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=426 h=720 window_id=2
    ");
}

// ============================================================================
// Tests for closing with 2/5, 3/5, 4/5 preset sizes
// ============================================================================

#[test]
fn closing_first_of_three_fifths_preset() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 2/5, 3/5, 4/5 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 3/5
        Op::SwitchPresetColumnWidth, // 3 -> 4/5
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-200.0)
    view_pos=412.0
    active_column=2
    active_column_x=612.0
    active_tile_viewport_x=200.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1]: x=100.0 width=Proportion(0.4) active_tile=0
      tile[0]: x=100.0 y=0.0 w=512 h=720 window_id=2
    column[2] [ACTIVE]: x=612.0 width=Proportion(0.6) active_tile=0
      tile[0] [ACTIVE]: x=612.0 y=0.0 w=768 h=720 window_id=3
    ");

    // Focus and close first column (2/5)
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::CloseWindow(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-0.0)
    view_pos=0.0
    active_column=0
    active_column_x=0.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0] [ACTIVE]: x=0.0 width=Proportion(0.4) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=512 h=720 window_id=2
    column[1]: x=512.0 width=Proportion(0.6) active_tile=0
      tile[0]: x=512.0 y=0.0 w=768 h=720 window_id=3
    ");
}

#[test]
fn closing_middle_of_three_fifths_preset() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 2/5, 3/5, 4/5 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 3/5
        Op::SwitchPresetColumnWidth, // 3 -> 4/5
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus and close middle column (3/5)
    let ops = [
        Op::FocusColumnLeft,
        Op::CloseWindow(2),
        Op::Communicate(1),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-0.0)
    view_pos=100.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Proportion(0.6) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=768 h=720 window_id=3
    ");
}

#[test]
fn closing_last_of_three_fifths_preset() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 2/5, 3/5, 4/5 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 3/5
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 3/5
        Op::SwitchPresetColumnWidth, // 3 -> 4/5
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Close last column (4/5) - already focused
    let ops = [
        Op::CloseWindow(3),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
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
    view_offset=Static(-100.0)
    view_pos=0.0
    active_column=1
    active_column_x=100.0
    active_tile_viewport_x=100.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
    column[1] [ACTIVE]: x=100.0 width=Proportion(0.4) active_tile=0
      tile[0] [ACTIVE]: x=100.0 y=0.0 w=512 h=720 window_id=2
    ");
}
