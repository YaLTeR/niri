// Golden tests for 220_maximize_fullscreen
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn fullscreen_window_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_first_of_three_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_first_of_three_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_last_of_three_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_last_of_three_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_middle_of_three_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_middle_of_three_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_with_gaps_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_with_gaps_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_with_mixed_preset_sizes_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_with_mixed_preset_sizes_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_with_struts_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximize_column_with_struts_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximized_fullscreen_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn maximized_fullscreen_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn fullscreen_window() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, fullscreen_window_ops());
    assert_golden!(layout.snapshot(), "fullscreen_window");
}

#[test]
fn maximize_column_first_of_three_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_first_of_three_1_ops());
    assert_golden!(layout.snapshot(), "maximize_column_first_of_three_1");
}

#[test]
fn maximize_column_first_of_three_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_first_of_three_2_ops());
    assert_golden!(layout.snapshot(), "maximize_column_first_of_three_2");
}

#[test]
fn maximize_column_last_of_three_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_last_of_three_1_ops());
    assert_golden!(layout.snapshot(), "maximize_column_last_of_three_1");
}

#[test]
fn maximize_column_last_of_three_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_last_of_three_2_ops());
    assert_golden!(layout.snapshot(), "maximize_column_last_of_three_2");
}

#[test]
fn maximize_column_middle_of_three_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_middle_of_three_1_ops());
    assert_golden!(layout.snapshot(), "maximize_column_middle_of_three_1");
}

#[test]
fn maximize_column_middle_of_three_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_middle_of_three_2_ops());
    assert_golden!(layout.snapshot(), "maximize_column_middle_of_three_2");
}

#[test]
fn maximize_column_with_gaps_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_with_gaps_1_ops());
    assert_golden!(layout.snapshot(), "maximize_column_with_gaps_1");
}

#[test]
fn maximize_column_with_gaps_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_with_gaps_2_ops());
    assert_golden!(layout.snapshot(), "maximize_column_with_gaps_2");
}

#[test]
fn maximize_column_with_mixed_preset_sizes_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_with_mixed_preset_sizes_1_ops());
    assert_golden!(layout.snapshot(), "maximize_column_with_mixed_preset_sizes_1");
}

#[test]
fn maximize_column_with_mixed_preset_sizes_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_with_mixed_preset_sizes_2_ops());
    assert_golden!(layout.snapshot(), "maximize_column_with_mixed_preset_sizes_2");
}

#[test]
fn maximize_column_with_struts_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_with_struts_1_ops());
    assert_golden!(layout.snapshot(), "maximize_column_with_struts_1");
}

#[test]
fn maximize_column_with_struts_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximize_column_with_struts_2_ops());
    assert_golden!(layout.snapshot(), "maximize_column_with_struts_2");
}

#[test]
fn maximized_fullscreen_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximized_fullscreen_1_ops());
    assert_golden!(layout.snapshot(), "maximized_fullscreen_1");
}

#[test]
fn maximized_fullscreen_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, maximized_fullscreen_2_ops());
    assert_golden!(layout.snapshot(), "maximized_fullscreen_2");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn fullscreen_window_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, fullscreen_window_ops());
    assert_golden_rtl!(layout, "fullscreen_window");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_first_of_three_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_first_of_three_1_ops());
    assert_golden_rtl!(layout, "maximize_column_first_of_three_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_first_of_three_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_first_of_three_2_ops());
    assert_golden_rtl!(layout, "maximize_column_first_of_three_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_last_of_three_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_last_of_three_1_ops());
    assert_golden_rtl!(layout, "maximize_column_last_of_three_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_last_of_three_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_last_of_three_2_ops());
    assert_golden_rtl!(layout, "maximize_column_last_of_three_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_middle_of_three_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_middle_of_three_1_ops());
    assert_golden_rtl!(layout, "maximize_column_middle_of_three_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_middle_of_three_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_middle_of_three_2_ops());
    assert_golden_rtl!(layout, "maximize_column_middle_of_three_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_with_gaps_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_with_gaps_1_ops());
    assert_golden_rtl!(layout, "maximize_column_with_gaps_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_with_gaps_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_with_gaps_2_ops());
    assert_golden_rtl!(layout, "maximize_column_with_gaps_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_with_mixed_preset_sizes_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_with_mixed_preset_sizes_1_ops());
    assert_golden_rtl!(layout, "maximize_column_with_mixed_preset_sizes_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_with_mixed_preset_sizes_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_with_mixed_preset_sizes_2_ops());
    assert_golden_rtl!(layout, "maximize_column_with_mixed_preset_sizes_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_with_struts_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_with_struts_1_ops());
    assert_golden_rtl!(layout, "maximize_column_with_struts_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximize_column_with_struts_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximize_column_with_struts_2_ops());
    assert_golden_rtl!(layout, "maximize_column_with_struts_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximized_fullscreen_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximized_fullscreen_1_ops());
    assert_golden_rtl!(layout, "maximized_fullscreen_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn maximized_fullscreen_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, maximized_fullscreen_2_ops());
    assert_golden_rtl!(layout, "maximized_fullscreen_2");
}
