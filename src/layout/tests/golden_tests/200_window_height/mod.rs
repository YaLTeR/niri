// Golden tests for 200_window_height
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn preset_window_heights_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn window_height_resize_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn preset_window_heights() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_window_heights_ops());
    assert_golden!(layout.snapshot(), "preset_window_heights");
}

#[test]
fn window_height_resize() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, window_height_resize_ops());
    assert_golden!(layout.snapshot(), "window_height_resize");
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
fn preset_window_heights_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_window_heights_ops());
    assert_golden_rtl!(layout, "preset_window_heights");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn window_height_resize_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, window_height_resize_ops());
    assert_golden_rtl!(layout, "window_height_resize");
}
