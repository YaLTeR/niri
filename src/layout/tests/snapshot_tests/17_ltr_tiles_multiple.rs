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

fn set_up_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_options(), ops)
}

// ============================================================================
// TILE TESTS - Multiple Tiles in Column
// ============================================================================

#[test]
fn three_tiles_in_column() {
    let mut layout = set_up_empty();

    // Create a column with 3 stacked windows
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
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
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");
}

#[test]
fn four_tiles_in_column_equal_height() {
    let mut layout = set_up_empty();

    // Create a column with 4 stacked windows (should auto-distribute height)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
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
    view_offset=Static(-300.0)
    active_column=3
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    column[3]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=4
    ");
}

#[test]
fn tiles_with_mixed_heights() {
    let mut layout = set_up_empty();

    // Create column with 3 tiles, set different heights
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetWindowHeight { id: Some(2), change: SizeChange::SetFixed(200) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetWindowHeight { id: Some(3), change: SizeChange::SetFixed(300) },
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
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=200 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=300 window_id=3
    ");
}

#[test]
fn tiles_with_preset_heights() {
    let mut layout = set_up_empty();

    // Create column with tiles using preset heights
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetWindowHeight { id: Some(2) }, // 1/3
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetWindowHeight { id: Some(3) }, // 1/3
        Op::SwitchPresetWindowHeight { id: Some(3) }, // 1/2
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
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=240 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=360 window_id=3
    ");
}
