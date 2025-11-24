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
