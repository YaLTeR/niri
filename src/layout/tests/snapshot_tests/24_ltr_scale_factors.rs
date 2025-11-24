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
// CONFIG TESTS - Different Scale Factors
// ============================================================================

#[test]
fn scale_factor_1_5() {
    let options = make_options();
    
    let ops = [
        Op::AddScaledOutput {
            id: 1,
            scale: 1.5,
            layout_config: None,
        },
    ];
    let mut layout = check_ops_with_options(options, ops);

    // Spawn windows with 1.5x scale
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
    view_offset=Static(-100.0)
    active_column=1
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=284 h=480 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=284 h=480 window_id=2
    ");
}

#[test]
fn scale_factor_2_0() {
    let options = make_options();
    
    let ops = [
        Op::AddScaledOutput {
            id: 1,
            scale: 2.0,
            layout_config: None,
        },
    ];
    let mut layout = check_ops_with_options(options, ops);

    // Spawn windows with 2.0x scale
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-100.0)
    active_column=1
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=320 h=360 window_id=1
    column[1]: width=Proportion(0.5) active_tile=0
      tile[0]: w=320 h=360 window_id=2
    ");
}

#[test]
fn scale_factor_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    
    let ops = [
        Op::AddScaledOutput {
            id: 1,
            scale: 1.5,
            layout_config: None,
        },
    ];
    let mut layout = check_ops_with_options(options, ops);

    // Gaps should be scaled appropriately
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
    view_offset=Static(-132.0)
    active_column=1
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=263 h=448 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=263 h=448 window_id=2
    ");
}
