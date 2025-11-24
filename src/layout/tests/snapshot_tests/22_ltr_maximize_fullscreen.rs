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

fn set_up_empty() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_options(), ops)
}

#[test]
fn maximized_fullscreen() {
    let mut layout = set_up_empty();

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

    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=0
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=1280 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    ");

    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=0
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    ");
}

#[test]
fn fullscreen_window() {
    let mut layout = set_up_empty();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    let ops = [
        Op::FullscreenWindow(1),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=0
    column[0]: width=Proportion(0.5) active_tile=0
      tile[0]: w=1280 h=720 window_id=1
    ");
}

#[test]
fn maximize_column_first_of_three() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns
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

    // Focus first column and maximize it
    let ops = [
        Op::FocusColumnLeft,
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=0
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=1280 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");

    // Maximize again - should restore original width
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=0
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");
}

#[test]
fn maximize_column_middle_of_three() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns
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

    // Focus middle column and maximize it
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=1280 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");

    // Maximize again - should restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");
}

#[test]
fn maximize_column_last_of_three() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns
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

    // Maximize last column (already focused)
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=1280 h=720 window_id=3
    ");

    // Maximize again - should restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
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
fn maximize_column_with_mixed_preset_sizes() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: 1/3, 1/2, 2/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SwitchPresetColumnWidth, // 2 -> 1/2
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SwitchPresetColumnWidth, // 3 -> 1/2
        Op::SwitchPresetColumnWidth, // 3 -> 2/3
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    // Maximize middle column (1/2)
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333326) active_tile=0
      tile[0]: w=1280 h=720 window_id=2
    column[2]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=3
    ");

    // Restore - should go back to 1/2
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(0.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333326) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    column[2]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=3
    ");
}

#[test]
fn maximize_column_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns with gaps
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

    // Maximize middle column with gaps
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-16.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=1248 h=688 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=3
    ");

    // Restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-16.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=3
    ");
}

#[test]
fn maximize_column_with_struts() {
    let mut options = make_options();
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(50.0),
        right: niri_config::FloatOrInt(50.0),
        top: niri_config::FloatOrInt(20.0),
        bottom: niri_config::FloatOrInt(20.0),
    };
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 1/3 columns with struts
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

    // Maximize middle column - should respect struts
    let ops = [
        Op::FocusColumnLeft,
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-50.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=1180 h=680 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=3
    ");

    // Restore
    let ops = [
        Op::MaximizeColumn,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-50.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=3
    ");
}
