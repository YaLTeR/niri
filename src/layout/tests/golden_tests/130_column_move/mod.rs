// Golden tests for 130_column_move
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn move_column_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn move_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, move_column_ops());
    assert_golden!(layout.snapshot(), "move_column");
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
fn move_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, move_column_ops());
    assert_golden_rtl!(layout, "move_column");
}
