// Golden tests for 120_resize_incremental
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn adjust_fixed_width_incrementally_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn resize_adjust_incremental_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn resize_adjust_incremental_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn adjust_fixed_width_incrementally() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, adjust_fixed_width_incrementally_ops());
    assert_golden!(layout.snapshot(), "adjust_fixed_width_incrementally");
}

#[test]
fn resize_adjust_incremental_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, resize_adjust_incremental_1_ops());
    assert_golden!(layout.snapshot(), "resize_adjust_incremental_1");
}

#[test]
fn resize_adjust_incremental_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, resize_adjust_incremental_2_ops());
    assert_golden!(layout.snapshot(), "resize_adjust_incremental_2");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn adjust_fixed_width_incrementally_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, adjust_fixed_width_incrementally_ops());
    assert_golden_rtl!(layout, "adjust_fixed_width_incrementally");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn resize_adjust_incremental_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, resize_adjust_incremental_1_ops());
    assert_golden_rtl!(layout, "resize_adjust_incremental_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn resize_adjust_incremental_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, resize_adjust_incremental_2_ops());
    assert_golden_rtl!(layout, "resize_adjust_incremental_2");
}
