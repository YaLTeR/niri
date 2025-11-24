// Golden tests for 060_focus_movement
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn focus_movement_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn focus_movement_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn focus_movement_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn focus_movement_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn focus_with_mixed_widths_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn focus_movement_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_movement_1_ops());
    assert_golden!(layout.snapshot(), "focus_movement_1");
}

#[test]
fn focus_movement_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_movement_2_ops());
    assert_golden!(layout.snapshot(), "focus_movement_2");
}

#[test]
fn focus_movement_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_movement_3_ops());
    assert_golden!(layout.snapshot(), "focus_movement_3");
}

#[test]
fn focus_movement_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_movement_4_ops());
    assert_golden!(layout.snapshot(), "focus_movement_4");
}

#[test]
fn focus_with_mixed_widths() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_with_mixed_widths_ops());
    assert_golden!(layout.snapshot(), "focus_with_mixed_widths");
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
fn focus_movement_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_movement_1_ops());
    assert_golden_rtl!(layout, "focus_movement_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_movement_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_movement_2_ops());
    assert_golden_rtl!(layout, "focus_movement_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_movement_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_movement_3_ops());
    assert_golden_rtl!(layout, "focus_movement_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_movement_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_movement_4_ops());
    assert_golden_rtl!(layout, "focus_movement_4");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_with_mixed_widths_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_with_mixed_widths_ops());
    assert_golden_rtl!(layout, "focus_with_mixed_widths");
}
