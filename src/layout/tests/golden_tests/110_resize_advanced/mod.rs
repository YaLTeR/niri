// Golden tests for 110_resize_advanced
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn resize_while_scrolled_overflow_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn resize_with_multiple_tiles_open_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn resize_while_scrolled_overflow() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, resize_while_scrolled_overflow_ops());
    assert_golden!(layout.snapshot(), "resize_while_scrolled_overflow");
}

#[test]
fn resize_with_multiple_tiles_open() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, resize_with_multiple_tiles_open_ops());
    assert_golden!(layout.snapshot(), "resize_with_multiple_tiles_open");
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
fn resize_while_scrolled_overflow_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, resize_while_scrolled_overflow_ops());
    assert_golden_rtl!(layout, "resize_while_scrolled_overflow");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn resize_with_multiple_tiles_open_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, resize_with_multiple_tiles_open_ops());
    assert_golden_rtl!(layout, "resize_with_multiple_tiles_open");
}
