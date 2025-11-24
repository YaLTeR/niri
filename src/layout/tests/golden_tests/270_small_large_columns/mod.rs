// Golden tests for 270_small_large_columns
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn single_very_wide_column_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn very_small_column_with_proportion_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn very_small_view_width_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn single_very_wide_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, single_very_wide_column_ops());
    assert_golden!(layout.snapshot(), "single_very_wide_column");
}

#[test]
fn very_small_column_with_proportion() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, very_small_column_with_proportion_ops());
    assert_golden!(layout.snapshot(), "very_small_column_with_proportion");
}

#[test]
fn very_small_view_width() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, very_small_view_width_ops());
    assert_golden!(layout.snapshot(), "very_small_view_width");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn single_very_wide_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, single_very_wide_column_ops());
    assert_golden_rtl!(layout, "single_very_wide_column");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn very_small_column_with_proportion_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, very_small_column_with_proportion_ops());
    assert_golden_rtl!(layout, "very_small_column_with_proportion");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn very_small_view_width_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, very_small_view_width_ops());
    assert_golden_rtl!(layout, "very_small_view_width");
}
