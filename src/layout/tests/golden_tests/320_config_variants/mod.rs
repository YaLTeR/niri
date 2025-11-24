// Golden tests for 320_config_variants
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn empty_workspace_above_first_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn gaps_struts_and_centering_combined_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_switch_rightmost_column_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_switch_rightmost_column_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_switch_rightmost_column_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_switch_rightmost_column_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_widths_with_gaps_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_widths_with_gaps_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_widths_with_gaps_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn empty_workspace_above_first() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, empty_workspace_above_first_ops());
    assert_golden!(layout.snapshot(), "empty_workspace_above_first");
}

#[test]
fn gaps_struts_and_centering_combined() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, gaps_struts_and_centering_combined_ops());
    assert_golden!(layout.snapshot(), "gaps_struts_and_centering_combined");
}

#[test]
fn preset_width_switch_rightmost_column_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_1_ops());
    assert_golden!(layout.snapshot(), "preset_width_switch_rightmost_column_1");
}

#[test]
fn preset_width_switch_rightmost_column_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_2_ops());
    assert_golden!(layout.snapshot(), "preset_width_switch_rightmost_column_2");
}

#[test]
fn preset_width_switch_rightmost_column_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_3_ops());
    assert_golden!(layout.snapshot(), "preset_width_switch_rightmost_column_3");
}

#[test]
fn preset_width_switch_rightmost_column_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_4_ops());
    assert_golden!(layout.snapshot(), "preset_width_switch_rightmost_column_4");
}

#[test]
fn preset_widths_with_gaps_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_widths_with_gaps_1_ops());
    assert_golden!(layout.snapshot(), "preset_widths_with_gaps_1");
}

#[test]
fn preset_widths_with_gaps_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_widths_with_gaps_2_ops());
    assert_golden!(layout.snapshot(), "preset_widths_with_gaps_2");
}

#[test]
fn preset_widths_with_gaps_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_widths_with_gaps_3_ops());
    assert_golden!(layout.snapshot(), "preset_widths_with_gaps_3");
}

// ============================================================================
// RTL Tests
// ============================================================================

fn make_options_rtl() -> Options {
    let mut options = make_options();
    options.layout.right_to_left = true;
    options
}

fn set_up_empty_rtl() -> Layout<TestWindow> {
    let ops = [Op::AddOutput(1)];
    check_ops_with_options(make_options_rtl(), ops)
}

#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn empty_workspace_above_first_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, empty_workspace_above_first_ops());
    assert_golden_rtl!(layout, "empty_workspace_above_first");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn gaps_struts_and_centering_combined_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, gaps_struts_and_centering_combined_ops());
    assert_golden_rtl!(layout, "gaps_struts_and_centering_combined");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_switch_rightmost_column_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_1_ops());
    assert_golden_rtl!(layout, "preset_width_switch_rightmost_column_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_switch_rightmost_column_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_2_ops());
    assert_golden_rtl!(layout, "preset_width_switch_rightmost_column_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_switch_rightmost_column_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_3_ops());
    assert_golden_rtl!(layout, "preset_width_switch_rightmost_column_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_switch_rightmost_column_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_switch_rightmost_column_4_ops());
    assert_golden_rtl!(layout, "preset_width_switch_rightmost_column_4");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_widths_with_gaps_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_widths_with_gaps_1_ops());
    assert_golden_rtl!(layout, "preset_widths_with_gaps_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_widths_with_gaps_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_widths_with_gaps_2_ops());
    assert_golden_rtl!(layout, "preset_widths_with_gaps_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_widths_with_gaps_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_widths_with_gaps_3_ops());
    assert_golden_rtl!(layout, "preset_widths_with_gaps_3");
}
