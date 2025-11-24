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
fn move_column_to_first() {
    let mut layout = set_up_empty();

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

    let ops = [
        Op::MoveColumnToFirst,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-0.0)
    active_column=0
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=3
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    column[2]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    ");
}

#[test]
fn move_column_to_last() {
    let mut layout = set_up_empty();

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

    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::MoveColumnToLast,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-852.0)
    active_column=2
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=3
    column[2]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    ");
}
