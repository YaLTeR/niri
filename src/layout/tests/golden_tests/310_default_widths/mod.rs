// Golden tests for 310_default_widths
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn default_column_width_fixed_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn different_default_column_width_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn default_column_width_fixed() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, default_column_width_fixed_ops());
    assert_golden!(layout.snapshot(), "default_column_width_fixed");
}

#[test]
fn different_default_column_width() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, different_default_column_width_ops());
    assert_golden!(layout.snapshot(), "different_default_column_width");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn default_column_width_fixed_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, default_column_width_fixed_ops());
    assert_golden_rtl!(layout, "default_column_width_fixed");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn different_default_column_width_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, different_default_column_width_ops());
    assert_golden_rtl!(layout, "different_default_column_width");
}
