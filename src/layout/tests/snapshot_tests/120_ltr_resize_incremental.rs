use insta::assert_snapshot;

use super::*;

fn make_options() -> Options {
    let mut options = Options {
        layout: niri_config::Layout {
            gaps: 0.0,
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

#[test]
fn resize_adjust_incremental() {
    let mut layout = set_up_empty();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    let ops = [
        Op::SetColumnWidth(SizeChange::AdjustProportion(10.0)),
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
    column[0] [ACTIVE]: x=0.0 width=Proportion(0.6) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=768 h=720 window_id=1
    ");

    let ops = [
        Op::SetColumnWidth(SizeChange::AdjustProportion(-5.0)),
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
    column[0] [ACTIVE]: x=0.0 width=Proportion(0.5499999999999999) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=703 h=720 window_id=1
    ");
}

#[test]
fn adjust_fixed_width_incrementally() {
    let mut layout = set_up_empty();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(400)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    let ops = [
        Op::SetColumnWidth(SizeChange::AdjustFixed(100)),
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
    column[0] [ACTIVE]: x=0.0 width=Fixed(500.0) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=500 h=720 window_id=1
    ");
}
