// Golden tests for 180_tiles_focus
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn focus_window_down_in_column_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn focus_window_down_in_column_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn focus_window_up_in_column_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn focus_window_down_in_column_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_window_down_in_column_1_ops());
    assert_golden!(layout.snapshot(), "focus_window_down_in_column_1");
}

#[test]
fn focus_window_down_in_column_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_window_down_in_column_2_ops());
    assert_golden!(layout.snapshot(), "focus_window_down_in_column_2");
}

#[test]
fn focus_window_up_in_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_window_up_in_column_ops());
    assert_golden!(layout.snapshot(), "focus_window_up_in_column");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_window_down_in_column_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_window_down_in_column_1_ops());
    assert_golden_rtl!(layout, "focus_window_down_in_column_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_window_down_in_column_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_window_down_in_column_2_ops());
    assert_golden_rtl!(layout, "focus_window_down_in_column_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_window_up_in_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_window_up_in_column_ops());
    assert_golden_rtl!(layout, "focus_window_up_in_column");
}
