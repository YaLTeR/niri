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



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn move_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, move_column_ops());
    assert_golden_rtl!(layout, "move_column");
}
