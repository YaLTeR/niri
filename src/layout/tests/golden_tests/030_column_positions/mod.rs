// Golden tests for 030_column_positions
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn column_x_positions_mixed_widths_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn column_x_positions_three_columns_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn column_x_positions_two_columns_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn mixed_column_widths_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn column_x_positions_mixed_widths() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, column_x_positions_mixed_widths_ops());
    assert_golden!(layout.snapshot(), "column_x_positions_mixed_widths");
}

#[test]
fn column_x_positions_three_columns() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, column_x_positions_three_columns_ops());
    assert_golden!(layout.snapshot(), "column_x_positions_three_columns");
}

#[test]
fn column_x_positions_two_columns() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, column_x_positions_two_columns_ops());
    assert_golden!(layout.snapshot(), "column_x_positions_two_columns");
}

#[test]
fn mixed_column_widths() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, mixed_column_widths_ops());
    assert_golden!(layout.snapshot(), "mixed_column_widths");
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
fn column_x_positions_mixed_widths_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, column_x_positions_mixed_widths_ops());
    assert_golden_rtl!(layout, "column_x_positions_mixed_widths");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn column_x_positions_three_columns_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, column_x_positions_three_columns_ops());
    assert_golden_rtl!(layout, "column_x_positions_three_columns");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn column_x_positions_two_columns_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, column_x_positions_two_columns_ops());
    assert_golden_rtl!(layout, "column_x_positions_two_columns");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn mixed_column_widths_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, mixed_column_widths_ops());
    assert_golden_rtl!(layout, "mixed_column_widths");
}
