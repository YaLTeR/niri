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
            preset_column_widths: vec![
                niri_config::PresetSize::Proportion(1.0 / 3.0),
                niri_config::PresetSize::Proportion(1.0 / 2.0),
                niri_config::PresetSize::Proportion(2.0 / 3.0),
            ],
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

// ============================================================================
// COLUMN RESIZE - Basic Operations
// ============================================================================

#[test]
fn column_resize() {
    let mut layout = set_up_empty();

    // 1) Spawn 2 windows, 50% each.
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
    view_offset=Static(-100.0)
    view_pos=540.0
    active_column=1
    active_column_x=640.0
    active_tile_viewport_x=100.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.5) active_tile=0
      tile[0]: x=0.0 y=0.0 w=640 h=720 window_id=1
    column[1] [ACTIVE]: x=640.0 width=Proportion(0.5) active_tile=0
      tile[0] [ACTIVE]: x=640.0 y=0.0 w=640 h=720 window_id=2
    ");

    // 2) Resize active column (2) to 1/3.
    let ops = [
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
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
    view_offset=Static(-100.0)
    view_pos=540.0
    active_column=1
    active_column_x=640.0
    active_tile_viewport_x=100.0
    active_tile_viewport_y=0.0
    column[0]: x=0.0 width=Proportion(0.5) active_tile=0
      tile[0]: x=0.0 y=0.0 w=640 h=720 window_id=1
    column[1] [ACTIVE]: x=640.0 width=Proportion(0.33333333333333337) active_tile=0
      tile[0] [ACTIVE]: x=640.0 y=0.0 w=426 h=720 window_id=2
    ");
}
