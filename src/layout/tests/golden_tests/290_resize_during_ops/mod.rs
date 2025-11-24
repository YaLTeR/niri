// Golden tests for 290_resize_during_ops
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn close_column_while_moving_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn close_column_while_resizing_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn close_column_while_moving() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, close_column_while_moving_ops());
    assert_golden!(layout.snapshot(), "close_column_while_moving");
}

#[test]
fn close_column_while_resizing() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, close_column_while_resizing_ops());
    assert_golden!(layout.snapshot(), "close_column_while_resizing");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn close_column_while_moving_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, close_column_while_moving_ops());
    assert_golden_rtl!(layout, "close_column_while_moving");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn close_column_while_resizing_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, close_column_while_resizing_ops());
    assert_golden_rtl!(layout, "close_column_while_resizing");
}
