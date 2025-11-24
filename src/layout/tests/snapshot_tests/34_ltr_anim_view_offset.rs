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

fn set_up_anim_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_anim_options(), ops)
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
    view_offset=Static(-426.0)
    active_column=1
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    column[2]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=3
    column[3]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=4
    ");

    // Complete animation
    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
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
    view_offset=Static(-426.0)
    active_column=1
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    column[2]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=3
    column[3]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=4
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
    active_column=2
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    column[2]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=3
    column[3]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=4
    ");
}
