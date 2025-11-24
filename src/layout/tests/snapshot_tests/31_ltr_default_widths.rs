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
fn different_default_column_width() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(0.25));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
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
    active_column=0
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    ");
}

#[test]
fn default_column_width_fixed() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Fixed(400));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
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
    active_column=0
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    ");
}
