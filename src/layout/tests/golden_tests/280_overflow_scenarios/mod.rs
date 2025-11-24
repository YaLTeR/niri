// Golden tests for 280_overflow_scenarios
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn many_columns_overflow_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn many_columns_overflow() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, many_columns_overflow_ops());
    assert_golden!(layout.snapshot(), "many_columns_overflow");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn many_columns_overflow_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, many_columns_overflow_ops());
    assert_golden_rtl!(layout, "many_columns_overflow");
}
