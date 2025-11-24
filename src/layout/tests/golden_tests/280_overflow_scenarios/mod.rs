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
fn many_columns_overflow_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, many_columns_overflow_ops());
    assert_golden_rtl!(layout, "many_columns_overflow");
}
