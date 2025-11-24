use insta::assert_snapshot;

use super::*;

// ============================================================================
// ADD WINDOW NEXT TO - Spawn windows between existing columns
// ============================================================================

#[test]
fn spawn_window_between_two_columns() {
    let options = make_options();
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Two 1/3 columns
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
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
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    ");

    // Spawn window 3 next to window 1 (inserts between 1 and 2)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(3),
            next_to_id: 1,
        },
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
    view_offset=Static(-100.0)
    active_column=2
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    ");
}

#[test]
fn spawn_window_between_three_columns() {
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

    // Spawn window 4 next to window 2 (inserts between 2 and 3)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 2,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
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
    active_column=3
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=4
    column[3]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");
}

#[test]
fn spawn_window_between_with_mixed_sizes() {
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

    // Spawn window 4 next to window 2 (between 1/2 and 2/3)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 2,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
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
    active_column=3
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Proportion(0.33333333333333326) active_tile=0
      tile[0]: w=426 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=4
    column[3]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=3
    ");
}

#[test]
fn spawn_window_between_with_gaps() {
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

    // Spawn window 4 next to window 1 (between 1 and 2)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 1,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
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
    gaps=16
    view_offset=Static(-248.0)
    active_column=3
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=4
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=2
    column[3]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=688 window_id=3
    ");
}

#[test]
fn spawn_window_between_with_overflow() {
    let mut options = make_options();
    options.layout.default_column_width = Some(niri_config::PresetSize::Proportion(2.0 / 5.0));
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // Setup: Three 2/5 columns (creates overflow)
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

    // Spawn window 4 next to window 2 (inserts in middle, increases overflow)
    let ops = [
        Op::AddWindowNextTo {
            params: TestWindowParams::new(4),
            next_to_id: 2,
        },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
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
    active_column=3
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=2
    column[2]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=4
    column[3]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=720 window_id=3
    ");
}

