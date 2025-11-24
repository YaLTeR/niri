// Golden tests for 080_column_resize
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn column_resize_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn column_resize_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn column_resize_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, column_resize_1_ops());
    assert_golden!(layout.snapshot(), "column_resize_1");
}

#[test]
fn column_resize_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, column_resize_2_ops());
    assert_golden!(layout.snapshot(), "column_resize_2");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn column_resize_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, column_resize_1_ops());
    assert_golden_rtl!(layout, "column_resize_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn column_resize_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, column_resize_2_ops());
    assert_golden_rtl!(layout, "column_resize_2");
}
