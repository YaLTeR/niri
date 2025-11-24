// Golden tests for 230_column_display
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn set_column_display_tabbed_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn toggle_column_tabbed_display_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn set_column_display_tabbed() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, set_column_display_tabbed_ops());
    assert_golden!(layout.snapshot(), "set_column_display_tabbed");
}

#[test]
fn toggle_column_tabbed_display() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, toggle_column_tabbed_display_ops());
    assert_golden!(layout.snapshot(), "toggle_column_tabbed_display");
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
fn set_column_display_tabbed_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, set_column_display_tabbed_ops());
    assert_golden_rtl!(layout, "set_column_display_tabbed");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn toggle_column_tabbed_display_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, toggle_column_tabbed_display_ops());
    assert_golden_rtl!(layout, "toggle_column_tabbed_display");
}
