// Golden tests for 190_tiles_movement
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn move_window_down_in_column_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn move_window_down_in_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, move_window_down_in_column_ops());
    assert_golden!(layout.snapshot(), "move_window_down_in_column");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn move_window_down_in_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, move_window_down_in_column_ops());
    assert_golden_rtl!(layout, "move_window_down_in_column");
}
