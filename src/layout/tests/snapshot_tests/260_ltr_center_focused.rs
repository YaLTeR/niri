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

#[test]
fn center_focused_column_always() {    
    let mut options = make_options();
    options.layout.center_focused_column = niri_config::CenterFocusedColumn::Always;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

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
    view_offset=Static(-427.0)
    view_pos=-1.0
    active_column=1
    active_column_x=426.0
    active_tile_viewport_x=427.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: x=0.0 y=0.0 w=426 h=720 window_id=1
    column[1] [ACTIVE]: x=426.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=426.0 y=0.0 w=426 h=720 window_id=2
    ");
}

#[test]
fn center_focused_column_on_overflow() {
    let mut options = make_options();
    options.layout.center_focused_column = niri_config::CenterFocusedColumn::OnOverflow;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

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
}

#[test]
fn always_center_single_column() {
    let mut options = make_options();
    options.layout.always_center_single_column = true;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

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
    view_offset=Static(-427.0)
    view_pos=-427.0
    active_column=0
    active_column_x=0.0
    active_tile_viewport_x=427.0
    active_tile_viewport_y=0.0
    column[0] [ACTIVE]: x=0.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=0.0 y=0.0 w=426 h=720 window_id=1
    ");
}
