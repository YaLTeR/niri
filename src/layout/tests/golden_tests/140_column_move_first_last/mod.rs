// Golden tests for 140_column_move_first_last
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn move_column_to_first_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn move_column_to_last_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn move_column_to_first() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, move_column_to_first_ops());
    assert_golden!(layout.snapshot(), "move_column_to_first");
}

#[test]
fn move_column_to_last() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, move_column_to_last_ops());
    assert_golden!(layout.snapshot(), "move_column_to_last");
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
fn move_column_to_first_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, move_column_to_first_ops());
    assert_golden_rtl!(layout, "move_column_to_first");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn move_column_to_last_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, move_column_to_last_ops());
    assert_golden_rtl!(layout, "move_column_to_last");
}
