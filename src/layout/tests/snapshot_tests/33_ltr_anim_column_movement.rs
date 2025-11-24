use insta::assert_snapshot;

use super::*;

fn make_anim_options() -> Options {
    use niri_config::animations::{Curve, EasingParams, Kind};
    
    const LINEAR: Kind = Kind::Easing(EasingParams {
        duration_ms: 1000,
        curve: Curve::Linear,
    });

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
    options.animations.window_resize.anim.kind = LINEAR;
    options.animations.window_movement.0.kind = LINEAR;
    options.animations.horizontal_view_movement.0.kind = LINEAR;

    options
}

fn format_column_positions(layout: &Layout<TestWindow>) -> String {
    use std::fmt::Write as _;
    
    let mut buf = String::new();
    let ws = layout.active_workspace().unwrap();
    let mut tiles: Vec<_> = ws.tiles_with_render_positions().collect();

    tiles.sort_by_key(|(tile, _, _)| tile.window().id());
    for (tile, pos, _visible) in tiles {
        let Size { w, .. } = tile.animated_tile_size();
        let Point { x, .. } = pos;
        writeln!(&mut buf, "win{}: x={x:>4.0} w={w:>4.0}", tile.window().id()).unwrap();
    }
    buf
}

fn set_up_anim_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_anim_options(), ops)
}

// ============================================================================
// ANIMATION TESTS - Column Movement
// ============================================================================

#[test]
fn anim_move_column_right() {
    let mut layout = set_up_anim_empty();

    // Setup: Three 1/3 columns
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
    
    // Initial state
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 426
    win2: x= 426 w= 426
    win3: x= 852 w= 426
    ");

    // Move first column right (swap with second)
    let ops = [Op::MoveColumnRight];
    check_ops_on_layout(&mut layout, ops);
    
    // At start of animation
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 426
    win2: x= 426 w= 426
    win3: x= 852 w= 426
    ");

    // Halfway through animation (500ms)
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 213 w= 426
    win2: x= 213 w= 426
    win3: x= 852 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 426 w= 426
    win2: x=   0 w= 426
    win3: x= 852 w= 426
    ");
}

#[test]
fn anim_move_column_left() {
    let mut layout = set_up_anim_empty();

    // Setup: Three 1/3 columns, focus on third
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

    // Initial state
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=-652 w= 426
    win2: x=-226 w= 426
    win3: x= 200 w= 426
    ");

    // Move third column left (swap with second)
    let ops = [Op::MoveColumnLeft];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=-539 w= 426
    win2: x= 100 w= 426
    win3: x= 100 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=-426 w= 426
    win2: x= 426 w= 426
    win3: x=   0 w= 426
    ");
}

#[test]
fn anim_move_column_to_first() {
    let mut layout = set_up_anim_empty();

    // Setup: Three columns, focus on third
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

    // Move third column to first position
    let ops = [Op::MoveColumnToFirst];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=-113 w= 426
    win2: x= 313 w= 426
    win3: x= 100 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 426 w= 426
    win2: x= 852 w= 426
    win3: x=   0 w= 426
    ");
}

#[test]
fn anim_move_column_to_last() {
    let mut layout = set_up_anim_empty();

    // Setup: Three columns, focus on first
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Move first column to last position
    let ops = [Op::MoveColumnToLast];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 426 w= 426
    win2: x= 213 w= 426
    win3: x= 639 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 852 w= 426
    win2: x=   0 w= 426
    win3: x= 426 w= 426
    ");
}
