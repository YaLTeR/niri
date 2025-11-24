use insta::assert_snapshot;

use super::*;

// ============================================================================
// CONFIG VARIANTS - Various configuration combinations
// ============================================================================

#[test]
fn empty_workspace_above_first() {
    let mut options = make_options();
    options.layout.empty_workspace_above_first = true;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows - should have empty workspace above
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
      tile[0]: w=640 h=720 window_id=1
    column[1]: width=Proportion(0.5) active_tile=0
      tile[0]: w=640 h=720 window_id=2
    ");
}

#[test]
fn gaps_struts_and_centering_combined() {
    let mut options = make_options();
    options.layout.gaps = 16.0;
    options.layout.struts = niri_config::Struts {
        left: niri_config::FloatOrInt(20.0),
        right: niri_config::FloatOrInt(20.0),
        top: niri_config::FloatOrInt(10.0),
        bottom: niri_config::FloatOrInt(10.0),
    };
    options.layout.center_focused_column = CenterFocusedColumn::OnOverflow;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows with gaps, struts, and centering
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-384.0)
    active_column=3
    column[0]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=392 h=668 window_id=1
    column[1]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=392 h=668 window_id=2
    column[2]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=392 h=668 window_id=3
    column[3]: width=Proportion(0.33333333333333337) active_tile=0
      tile[0]: w=392 h=668 window_id=4
    ");
}

#[test]
fn preset_widths_with_gaps() {
    let mut options = make_options();
    options.layout.gaps = 20.0;
    
    let ops = [Op::AddOutput(1)];
    let mut layout = check_ops_with_options(options, ops);

    // 1) Spawn windows and cycle through preset widths with gaps
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-140.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=1
    column[1]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=2
    ");

    // 2) Switch preset width to 1/2
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-140.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=1
    column[1]: width=Proportion(0.33333333333333326) active_tile=0
      tile[0]: w=399 h=680 window_id=2
    ");

    // 3) Switch preset width to 2/3
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(2),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(layout.snapshot(), @r"
    view_offset=Static(-140.0)
    active_column=1
    column[0]: width=Fixed(100.0) active_tile=0
      tile[0]: w=100 h=680 window_id=1
    column[1]: width=Proportion(0.5) active_tile=0
      tile[0]: w=610 h=680 window_id=2
    ");
}

// ============================================================================
// Tests for alternative preset sizes: 2/5, 3/5, 4/5
// ============================================================================

#[test]
fn preset_width_switch_rightmost_column() {
    let mut layout = set_up_empty();

    // Start with 3 columns at 1/3 width each, focus on rightmost
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);

    // Initial state: all three columns visible
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");

    // 1st MOD+R: rightmost 1/3 → 1/2 (640px), camera shifts left
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");

    // 2nd MOD+R: rightmost 1/2 → 2/3 (853px), camera shifts further
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");

    // 3rd MOD+R: rightmost 2/3 → 1/3 (back to 426px), camera shifts back but left column stays OOB
    let ops = [
        Op::SwitchPresetColumnWidth,
        Op::Communicate(3),
    ];
    check_ops_on_layout(&mut layout, ops);
    assert_snapshot!(format_column_edges(&layout), @r"
    left:   0 right: 100 width: 100
    left: 100 right: 200 width: 100
    left: 200 right: 300 width: 100
    ");
}

// ============================================================================
// Tests for user settings: gaps, struts, center_focused_column, etc.
// ============================================================================

