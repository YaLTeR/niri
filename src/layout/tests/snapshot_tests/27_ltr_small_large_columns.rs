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
// CONFIG TESTS - Very Small View Sizes
// ============================================================================

#[test]
fn very_small_view_width() {
    let options = make_options();
    
    // This would require modifying the output size, which isn't directly exposed
    // Instead, test with very small column widths
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Create very narrow columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(50)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetFixed(50)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetFixed(50)),
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
    column[0]: width=Fixed(50.0) active_tile=0
      tile[0]: w=50 h=720 window_id=1
    column[1]: width=Fixed(50.0) active_tile=0
      tile[0]: w=50 h=720 window_id=2
    column[2]: width=Fixed(50.0) active_tile=0
      tile[0]: w=50 h=720 window_id=3
    ");
}

#[test]
fn very_small_column_with_proportion() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(0.05));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 5% width columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::AddWindow { params: TestWindowParams::new(3) },
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
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");
}

#[test]
fn single_very_wide_column() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Create a column wider than the view
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(2000)),
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
    column[0]: width=Fixed(2000.0) active_tile=0
      tile[0]: w=2000 h=720 window_id=1
    ");
}
