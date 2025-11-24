// Golden tests for 240_scale_factors
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn scale_factor_1_5_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn scale_factor_2_0_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn scale_factor_with_gaps_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn scale_factor_1_5() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, scale_factor_1_5_ops());
    assert_golden!(layout.snapshot(), "scale_factor_1_5");
}

#[test]
fn scale_factor_2_0() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, scale_factor_2_0_ops());
    assert_golden!(layout.snapshot(), "scale_factor_2_0");
}

#[test]
fn scale_factor_with_gaps() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, scale_factor_with_gaps_ops());
    assert_golden!(layout.snapshot(), "scale_factor_with_gaps");
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
fn scale_factor_1_5_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, scale_factor_1_5_ops());
    assert_golden_rtl!(layout, "scale_factor_1_5");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn scale_factor_2_0_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, scale_factor_2_0_ops());
    assert_golden_rtl!(layout, "scale_factor_2_0");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn scale_factor_with_gaps_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, scale_factor_with_gaps_ops());
    assert_golden_rtl!(layout, "scale_factor_with_gaps");
}
