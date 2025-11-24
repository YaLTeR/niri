// Golden tests for 040_view_offset
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn view_offset_clamped_at_zero_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn view_offset_clamped_with_small_columns_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn view_offset_with_overflow_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn view_offset_clamped_at_zero() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, view_offset_clamped_at_zero_ops());
    assert_golden!(layout.snapshot(), "view_offset_clamped_at_zero");
}

#[test]
fn view_offset_clamped_with_small_columns() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, view_offset_clamped_with_small_columns_ops());
    assert_golden!(layout.snapshot(), "view_offset_clamped_with_small_columns");
}

#[test]
fn view_offset_with_overflow() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, view_offset_with_overflow_ops());
    assert_golden!(layout.snapshot(), "view_offset_with_overflow");
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
fn view_offset_clamped_at_zero_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, view_offset_clamped_at_zero_ops());
    assert_golden_rtl!(layout, "view_offset_clamped_at_zero");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn view_offset_clamped_with_small_columns_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, view_offset_clamped_with_small_columns_ops());
    assert_golden_rtl!(layout, "view_offset_clamped_with_small_columns");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn view_offset_with_overflow_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, view_offset_with_overflow_ops());
    assert_golden_rtl!(layout, "view_offset_with_overflow");
}
