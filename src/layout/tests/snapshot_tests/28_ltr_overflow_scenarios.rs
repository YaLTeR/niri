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
// CONFIG TESTS - Overflow Scenarios
// ============================================================================

#[test]
fn many_columns_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Fixed(200));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Create 10 columns (way more than fits)
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::AddWindow { params: TestWindowParams::new(5) },
        Op::AddWindow { params: TestWindowParams::new(6) },
        Op::AddWindow { params: TestWindowParams::new(7) },
        Op::AddWindow { params: TestWindowParams::new(8) },
        Op::AddWindow { params: TestWindowParams::new(9) },
        Op::AddWindow { params: TestWindowParams::new(10) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::Communicate(5),
        Op::Communicate(6),
        Op::Communicate(7),
        Op::Communicate(8),
        Op::Communicate(9),
        Op::Communicate(10),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Should show the last column
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
    view_offset=Static(-900.0)
    active_column=9
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    column[3]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=4
    column[4]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=5
    column[5]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=6
    column[6]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=7
    column[7]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=8
    column[8]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=9
    column[9]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=10
    ");
}
