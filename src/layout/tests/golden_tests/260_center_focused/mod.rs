// Golden tests for 260_center_focused
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn always_center_single_column_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn center_focused_column_always_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn center_focused_column_on_overflow_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn always_center_single_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, always_center_single_column_ops());
    assert_golden!(layout.snapshot(), "always_center_single_column");
}

#[test]
fn center_focused_column_always() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, center_focused_column_always_ops());
    assert_golden!(layout.snapshot(), "center_focused_column_always");
}

#[test]
fn center_focused_column_on_overflow() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, center_focused_column_on_overflow_ops());
    assert_golden!(layout.snapshot(), "center_focused_column_on_overflow");
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
fn always_center_single_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, always_center_single_column_ops());
    assert_golden_rtl!(layout, "always_center_single_column");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn center_focused_column_always_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, center_focused_column_always_ops());
    assert_golden_rtl!(layout, "center_focused_column_always");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn center_focused_column_on_overflow_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, center_focused_column_on_overflow_ops());
    assert_golden_rtl!(layout, "center_focused_column_on_overflow");
}
