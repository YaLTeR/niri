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
// BASIC SPAWNING - Single Column
// ============================================================================

#[test]
fn spawn_single_column_one_third() {
    let mut layout = set_up_empty();

    // Spawn a 1/3 tile result [1/3 tile, 2/3 empty]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    assert_golden!(layout.snapshot(), "spawn_single_column_one_third");
}

#[test]
fn spawn_single_column_one_half() {
    let mut layout = set_up_empty();

    // Spawn a 1/2 tile [1/2 tile, 1/2 empty]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    assert_golden!(layout.snapshot(), "spawn_single_column_one_half");
}

#[test]
fn spawn_single_column_two_thirds() {
    let mut layout = set_up_empty();

    // Spawn a 2/3 tile [2/3 tile, 1/3 empty]
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(200.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    assert_golden!(layout.snapshot(), "spawn_single_column_two_thirds");
}

#[test]
fn spawn_single_column_fixed_width() {
    let mut layout = set_up_empty();

    // Spawn with fixed width
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(400)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    assert_golden!(layout.snapshot(), "spawn_single_column_fixed_width");
}

#[test]
fn column_x_positions_single_column() {
    let mut layout = set_up_empty();

    // Single column at 1/3 width
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    
    // Column 0 should be at x=0
    // View offset = 0, so column renders at x=0
    assert_golden!(layout.snapshot(), "column_x_positions_single_column");
}
