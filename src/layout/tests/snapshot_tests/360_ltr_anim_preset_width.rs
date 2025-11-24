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
// ANIMATION TESTS - Preset Width with Leading Edge Pinned
// ============================================================================

#[test]
fn preset_width_anim_left_edge_pinned() {
    let mut layout = set_up_anim_empty();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 426
    ");

    let ops = [Op::SwitchPresetColumnWidth, Op::Communicate(1)];
    check_ops_on_layout(&mut layout, ops);

    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 533
    ");

    Op::AdvanceAnimations { msec_delta: 500 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 640
    ");
}

#[test]
fn preset_width_anim_with_multiple_columns() {
    let mut layout = set_up_anim_empty();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::FocusColumnLeft,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    let ops = [Op::SwitchPresetColumnWidth, Op::Communicate(1), Op::Communicate(2)];
    check_ops_on_layout(&mut layout, ops);

    Op::AdvanceAnimations { msec_delta: 1000 }.apply(&mut layout);
    assert_snapshot!(format_column_positions(&layout), @r"
    win1: x=   0 w= 640
    win2: x= 640 w= 426
    ");
}
