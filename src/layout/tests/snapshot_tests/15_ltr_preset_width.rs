use insta::assert_snapshot;

use super::*;

fn make_options() -> Options {
    let mut options = Options {
        layout: niri_config::Layout {
            gaps: 0.0,
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

#[test]
fn preset_width_index_cycles_correctly() {
    let mut layout = set_up_empty();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
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
    active_column=0
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    ");

    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
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
    active_column=0
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=1
    ");

    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
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
    active_column=0
    column[0]: width=Proportion(0.6666666666666665) active_tile=0
      tile[0]: w=853 h=720 window_id=1
    ");

    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(1),
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
    active_column=0
    column[0]: width=Proportion(0.33333333333333326) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    ");
}

#[test]
fn preset_width_index_cycles_backward() {
    let mut layout = set_up_empty();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    let ops = [
        Op::SwitchPresetColumnWidthBack,
        Op::Communicate(1),
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
    active_column=0
    column[0]: width=Proportion(0.6666666666666665) active_tile=0
      tile[0]: w=853 h=720 window_id=1
    ");

    let ops = [
        Op::SwitchPresetColumnWidthBack,
        Op::Communicate(1),
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
    active_column=0
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=1
    ");
}

#[test]
fn preset_cycling_two_fifths_to_four_fifths() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add two 2/5 tiles
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    ");

    // 2) Cycle second column to 3/5
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.4) active_tile=0
      tile[0]: w=512 h=720 window_id=2
    ");

    // 3) Cycle second column to 4/5
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.6) active_tile=0
      tile[0]: w=768 h=720 window_id=2
    ");

    // 4) Cycle back to 2/5
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.8) active_tile=0
      tile[0]: w=1024 h=720 window_id=2
    ");
}

#[test]
fn preset_two_fifths_gaps_struts_combined() {
    let mut options = make_options();
    options.layout.gaps = 12.0;
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(30.0),
        right: niri_config::FloatOrInt(30.0),
        top: niri_config::FloatOrInt(15.0),
        bottom: niri_config::FloatOrInt(15.0),
    };
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add three 2/5 tiles with both gaps and struts
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
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=666 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=666 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=666 window_id=3
    ");

    // 2) Cycle middle column to 3/5 - should push third column more OOB
    let ops = [
        Op::FocusColumnLeft,
        Op::SwitchPresetColumnWidth,
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=666 window_id=1
    column[1]: width=Proportion(0.4) active_tile=0
      tile[0]: w=471 h=666 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=666 window_id=3
    ");
}

#[test]
fn preset_cycling_middle_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles - creates overflow
    // (1/5 OOB [1/5 inbounds first tile, 2/5 second tile, 2/5 third tile])
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
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");

    // Focus middle column and cycle to 3/5
    // (1/5 OOB [1/5 inbounds first tile, 3/5 middle tile, 1/5 inbounds third tile] 1/5 OOB)
    let ops = [
        Op::FocusColumnLeft,
        Op::SwitchPresetColumnWidth,
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.4) active_tile=0
      tile[0]: w=512 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");

    // Cycle middle column to 4/5
    // (1/5 OOB [1/5 inbounds first tile, 4/5 middle tile] 2/5 OOB third tile)
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.6) active_tile=0
      tile[0]: w=768 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");

    // Cycle middle column back to 2/5
    // (1/5 OOB [1/5 inbounds first tile, 2/5 middle tile, 2/5 third tile])
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.8) active_tile=0
      tile[0]: w=1024 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");
}

#[test]
fn preset_cycling_rightmost_column_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Add three 2/5 tiles - creates overflow
    // (1/5 OOB [1/5 inbounds first tile, 2/5 second tile, 2/5 third tile])
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
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");

    // Cycle rightmost column to 3/5
    // (2/5 OOB first tile [2/5 second tile, 3/5 third tile])
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Proportion(0.4) active_tile=0
      tile[0]: w=512 h=720 window_id=3
    ");

    // Cycle rightmost column to 4/5
    // (2/5 OOB first tile, 1/5 OOB second tile, [1/5 inbounds second tile, 4/5 third tile])
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Proportion(0.6) active_tile=0
      tile[0]: w=768 h=720 window_id=3
    ");

    // Cycle rightmost column back to 2/5
    // (2/5 OOB first tile, 1/5 OOB second tile, [1/5 inbounds second tile, 2/5 third tile, 2/5 empty])
    let ops = [
        Op::SwitchPresetColumnWidth,
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
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Proportion(0.8) active_tile=0
      tile[0]: w=1024 h=720 window_id=3
    ");
}

// ============================================================================
// PRESET CYCLING - Advanced preset width cycling scenarios
// ============================================================================

