// Golden tests for 160_alternative_presets
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn preset_two_fifths_gaps_struts_combined_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_gaps_struts_combined_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_tiles_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_tiles_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_tiles_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_tiles_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_tiles_5_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_with_gaps_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_with_struts_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn preset_two_fifths_gaps_struts_combined_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_1_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_gaps_struts_combined_1");
}

#[test]
fn preset_two_fifths_gaps_struts_combined_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_2_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_gaps_struts_combined_2");
}

#[test]
fn preset_two_fifths_tiles_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_1_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_tiles_1");
}

#[test]
fn preset_two_fifths_tiles_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_2_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_tiles_2");
}

#[test]
fn preset_two_fifths_tiles_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_3_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_tiles_3");
}

#[test]
fn preset_two_fifths_tiles_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_4_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_tiles_4");
}

#[test]
fn preset_two_fifths_tiles_5() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_5_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_tiles_5");
}

#[test]
fn preset_two_fifths_with_gaps() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_with_gaps_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_with_gaps");
}

#[test]
fn preset_two_fifths_with_struts() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_with_struts_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_with_struts");
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
fn preset_two_fifths_gaps_struts_combined_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_1_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_gaps_struts_combined_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_gaps_struts_combined_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_2_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_gaps_struts_combined_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_tiles_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_1_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_tiles_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_tiles_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_2_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_tiles_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_tiles_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_3_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_tiles_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_tiles_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_4_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_tiles_4");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_tiles_5_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_tiles_5_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_tiles_5");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_with_gaps_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_with_gaps_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_with_gaps");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_with_struts_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_with_struts_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_with_struts");
}
