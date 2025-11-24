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

// ============================================================================
// CONFIG TESTS - Extreme Configurations
// ============================================================================

#[test]
fn very_large_gaps_extreme() {
    let mut options = make_options();
    options.layout.gaps = 200.0;
    
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
    view_offset=Static(-200.0)
    active_column=0
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=160 h=320 window_id=1
    ");
}

#[test]
fn very_large_struts_extreme() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(400.0),
        right: niri_config::FloatOrInt(400.0),
        top: niri_config::FloatOrInt(200.0),
        bottom: niri_config::FloatOrInt(200.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-400.0)
    active_column=0
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=320 window_id=1
    ");
}
