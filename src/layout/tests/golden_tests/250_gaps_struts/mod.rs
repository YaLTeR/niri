// Golden tests for 250_gaps_struts
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn very_large_gaps_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn very_large_struts_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn zero_gaps_explicit_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn very_large_gaps() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, very_large_gaps_ops());
    assert_golden!(layout.snapshot(), "very_large_gaps");
}

#[test]
fn very_large_struts() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, very_large_struts_ops());
    assert_golden!(layout.snapshot(), "very_large_struts");
}

#[test]
fn zero_gaps_explicit() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, zero_gaps_explicit_ops());
    assert_golden!(layout.snapshot(), "zero_gaps_explicit");
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
fn very_large_gaps_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, very_large_gaps_ops());
    assert_golden_rtl!(layout, "very_large_gaps");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn very_large_struts_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, very_large_struts_ops());
    assert_golden_rtl!(layout, "very_large_struts");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn zero_gaps_explicit_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, zero_gaps_explicit_ops());
    assert_golden_rtl!(layout, "zero_gaps_explicit");
}
