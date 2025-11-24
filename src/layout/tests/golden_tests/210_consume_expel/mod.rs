// Golden tests for 210_consume_expel
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn consume_into_column_with_tiles_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn expel_from_column_with_tiles_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn consume_into_column_with_tiles() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, consume_into_column_with_tiles_ops());
    assert_golden!(layout.snapshot(), "consume_into_column_with_tiles");
}

#[test]
fn expel_from_column_with_tiles() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, expel_from_column_with_tiles_ops());
    assert_golden!(layout.snapshot(), "expel_from_column_with_tiles");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn consume_into_column_with_tiles_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, consume_into_column_with_tiles_ops());
    assert_golden_rtl!(layout, "consume_into_column_with_tiles");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn expel_from_column_with_tiles_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, expel_from_column_with_tiles_ops());
    assert_golden_rtl!(layout, "expel_from_column_with_tiles");
}
