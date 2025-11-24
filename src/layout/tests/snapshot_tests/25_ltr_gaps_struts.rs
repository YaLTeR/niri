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

// ============================================================================
// CONFIG TESTS - Zero Gaps Explicit
// ============================================================================

#[test]
fn zero_gaps_explicit() {
    let mut options = make_options();
    options.layout.gaps = 0.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // With zero gaps, columns should be adjacent
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
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    column[2]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=3
    ");
}

// ============================================================================
// CONFIG TESTS - Very Large Gaps and Struts
// ============================================================================

#[test]
fn very_large_gaps() {
    let mut options = make_options();
    options.layout.gaps = 100.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // With 100px gaps, columns are far apart
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
    gaps=100
    view_offset=Static(-300.0)
    active_column=1
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=293 h=520 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=293 h=520 window_id=2
    ");
}

#[test]
fn very_large_struts() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(200.0),
        right: niri_config::FloatOrInt(200.0),
        top: niri_config::FloatOrInt(100.0),
        bottom: niri_config::FloatOrInt(100.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // With large struts, working area is much smaller
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_width=1280
    view_height=720
    scale=1
    working_area_x=200
    working_area_y=100
    working_area_width=880
    working_area_height=520
    parent_area_x=0
    parent_area_y=0
    parent_area_width=1280
    parent_area_height=720
    gaps=0
    view_offset=Static(-200.0)
    active_column=0
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=440 h=520 window_id=1
    ");
}
