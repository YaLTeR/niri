use insta::assert_snapshot;

use super::*;

// ============================================================================
// ALTERNATIVE PRESET SIZES - 2/5, 3/5, 4/5 preset column widths
// ============================================================================

#[test]
fn preset_two_fifths_tiles() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add first 2/5 tile [2/5 tile, 3/5 empty]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
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
    view_pos=0.0
    active_column=0
    active_column_x=0.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0] [ACTIVE]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=100 h=720 window_id=1
    ");

    // 2) Add second 2/5 tile [2/5 first tile, 2/5 second tile, 1/5 empty]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(2) },
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

    // 3) Add third 2/5 tile - causes overflow, 1/5 of first tile goes OOB
    //    [1/5 OOB | 1/5 inbounds first tile, 2/5 second tile, 2/5 new tile]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(3) },
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

    // 4) Focus left to second column - camera should adjust
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

    // 5) Focus left to first column - should be fully visible
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
}

#[test]
fn preset_two_fifths_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add three 2/5 tiles with gaps
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
}

#[test]
fn preset_two_fifths_with_struts() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(50.0),
        right: niri_config::FloatOrInt(50.0),
        top: niri_config::FloatOrInt(20.0),
        bottom: niri_config::FloatOrInt(20.0),
    };
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    options.layout.preset_column_widths = vec![
        niri_config::PresetSize::Proportion(2.0 / 5.0),
        niri_config::PresetSize::Proportion(3.0 / 5.0),
        niri_config::PresetSize::Proportion(4.0 / 5.0),
    ];
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Add three 2/5 tiles with struts
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
    working_area_x=50
    working_area_y=20
    working_area_width=1180
    working_area_height=680
    parent_area_x=0
    parent_area_y=0
    parent_area_width=1280
    parent_area_height=720
    gaps=0
    view_offset=Static(-250.0)
    view_pos=-50.0
    active_column=2
    active_column_x=200.0
    active_tile_viewport_x=250.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=680 window_id=1
    column[1]: x=100.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=100.0 y=0.0 w=100 h=680 window_id=2
    column[2] [ACTIVE]: x=200.0 width=Fixed(100.0) active_tile=0
      tile[0] [ACTIVE]: x=200.0 y=0.0 w=100 h=680 window_id=3
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
    view_pos=-42.0
    active_column=1
    active_column_x=112.0
    active_tile_viewport_x=154.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=100 h=666 window_id=1
    column[1] [ACTIVE]: x=112.0 width=Proportion(0.4) active_tile=0
      tile[0] [ACTIVE]: x=112.0 y=0.0 w=471 h=666 window_id=2
    column[2]: x=595.0 width=Fixed(100.0) active_tile=0
      tile[0]: x=595.0 y=0.0 w=100 h=666 window_id=3
    ");
}

