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
// FOCUS MOVEMENT - Left/Right Navigation
// ============================================================================

#[test]
fn focus_movement() {
    let mut layout = set_up_empty();

    // 1) Spawn 4 windows, each 1/3 width. This will overflow the view.
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
    view_pos=978.0
    active_column=3
    active_column_x=1278.0
    active_tile_viewport_x=300.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=0.0 y=0.0 w=426 h=720 window_id=1
    column[1]: x=426.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=426.0 y=0.0 w=426 h=720 window_id=2
    column[2]: x=852.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=852.0 y=0.0 w=426 h=720 window_id=3
    column[3] [ACTIVE]: x=1278.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=1278.0 y=0.0 w=426 h=720 window_id=4
    ");

    // 2) Move focus left to Window 3.
    // View offset shouldn't change because Window 3 is already visible.
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
    view_offset=Static(-0.0)
    view_pos=852.0
    active_column=2
    active_column_x=852.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=0.0 y=0.0 w=426 h=720 window_id=1
    column[1]: x=426.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=426.0 y=0.0 w=426 h=720 window_id=2
    column[2] [ACTIVE]: x=852.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=852.0 y=0.0 w=426 h=720 window_id=3
    column[3]: x=1278.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=1278.0 y=0.0 w=426 h=720 window_id=4
    ");

    // 3) Move focus left to Window 1 (which is out of view).
    // View offset should shift to 0 to show Window 1.
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
    view_offset=Static(-0.0)
    view_pos=0.0
    active_column=0
    active_column_x=0.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0] [ACTIVE]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=426 h=720 window_id=1
    column[1]: x=426.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=426.0 y=0.0 w=426 h=720 window_id=2
    column[2]: x=852.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=852.0 y=0.0 w=426 h=720 window_id=3
    column[3]: x=1278.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=1278.0 y=0.0 w=426 h=720 window_id=4
    ");

    // 4) Move focus right back to 4.
    // View offset should shift back to show Window 4.
    let ops = [
        Op::FocusColumnRight,
        Op::FocusColumnRight,
        Op::FocusColumnRight,
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
    view_offset=Static(-854.0)
    view_pos=424.0
    active_column=3
    active_column_x=1278.0
    active_tile_viewport_x=854.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=0.0 y=0.0 w=426 h=720 window_id=1
    column[1]: x=426.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=426.0 y=0.0 w=426 h=720 window_id=2
    column[2]: x=852.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=852.0 y=0.0 w=426 h=720 window_id=3
    column[3] [ACTIVE]: x=1278.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=1278.0 y=0.0 w=426 h=720 window_id=4
    ");
}

#[test]
fn focus_with_mixed_widths() {
    let mut layout = set_up_empty();

    // Create columns with different width types
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

    // Focus left to the proportional column
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
    view_offset=Static(-0.0)
    view_pos=200.0
    active_column=1
    active_column_x=200.0
    active_tile_viewport_x=0.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Fixed(200.0) active_tile=0
      tile[0]: x=0.0 y=0.0 w=200 h=720 window_id=1
    column[1] [ACTIVE]: x=200.0 width=Proportion(0.5) active_tile=0
      tile[0] [ACTIVE]: x=200.0 y=0.0 w=640 h=720 window_id=2
    column[2]: x=840.0 width=Fixed(300.0) active_tile=0
      tile[0]: x=840.0 y=0.0 w=300 h=720 window_id=3
    ");
}
