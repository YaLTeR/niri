use insta::assert_snapshot;

use super::*;

// ============================================================================
// ANIMATION TESTS - Column Movement
// ============================================================================

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
    win1: x=   2 w= 426
    win2: x= 428 w= 426
    win3: x= 854 w= 426
    ");

    // Move first column right (swap with second)
    let ops = [Op::MoveColumnRight];
    check_ops_on_layout(&mut layout, ops);
    
    // At start of animation
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   2 w= 426
    win2: x= 428 w= 426
    win3: x= 854 w= 426
    ");

    // Halfway through animation (500ms)
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 215 w= 426
    win2: x= 215 w= 426
    win3: x= 854 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 428 w= 426
    win2: x=   2 w= 426
    win3: x= 854 w= 426
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
    win1: x=   2 w= 426
    win2: x= 428 w= 426
    win3: x= 854 w= 426
    ");

    // Move third column left (swap with second)
    let ops = [Op::MoveColumnLeft];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   2 w= 426
    win2: x= 641 w= 426
    win3: x= 641 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   2 w= 426
    win2: x= 854 w= 426
    win3: x= 428 w= 426
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
    win1: x= 215 w= 426
    win2: x= 641 w= 426
    win3: x= 428 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 428 w= 426
    win2: x= 854 w= 426
    win3: x=   2 w= 426
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
    win1: x= 427 w= 426
    win2: x= 214 w= 426
    win3: x= 640 w= 426
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x= 854 w= 426
    win2: x=   2 w= 426
    win3: x= 428 w= 426
    ");
}

// ============================================================================
// ANIMATION TESTS - View Offset During Focus Changes
// ============================================================================

#[test]
fn anim_view_offset_focus_right() {
    let mut layout = set_up_anim_empty();

    // Setup: Four 1/3 columns (creates overflow)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Focus right - should animate view offset
    let ops = [Op::FocusColumnRight];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(0.0)
    Active Column: 1
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");
}

#[test]
fn anim_view_offset_focus_left() {
    let mut layout = set_up_anim_empty();

    // Setup: Four columns, focus on last
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

    // Focus left - should animate view offset
    let ops = [Op::FocusColumnLeft];
    check_ops_on_layout(&mut layout, ops);

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 1000 }.apply(&mut layout);
    assert_snapshot!(layout.snapshot(), @r"
    View Offset: Static(424.0)
    Active Column: 2
    Column 0: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=1
    Column 1: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=2
    Column 2: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=3
    Column 3: width=Proportion(0.33333333333333337), active_tile=0
      Tile 0: size=Size<smithay::utils::geometry::Logical> { w: 426.0, h: 720.0 }, window_id=4
    ");
}

#[test]
fn anim_resize_non_preset_fixed() {
    let mut layout = set_up_anim_empty();

    // Setup: Single column
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Resize to fixed 800px
    let ops = [
        Op::SetColumnWidth(SizeChange::SetFixed(800)),
        Op::Communicate(1),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 613
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 800
    ");
}

#[test]
fn anim_resize_adjust_proportion() {
    let mut layout = set_up_anim_empty();

    // Setup: Single column at 50%
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Adjust by +10%
    let ops = [
        Op::SetColumnWidth(SizeChange::AdjustProportion(10.0)),
        Op::Communicate(1),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Halfway through animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 704
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 768
    ");
}
