// Golden tests for 300_extreme_config
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn very_large_gaps_extreme_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn very_large_struts_extreme_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn very_large_gaps_extreme() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, very_large_gaps_extreme_ops());
    assert_golden!(layout.snapshot(), "very_large_gaps_extreme");
}

#[test]
fn very_large_struts_extreme() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, very_large_struts_extreme_ops());
    assert_golden!(layout.snapshot(), "very_large_struts_extreme");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn very_large_gaps_extreme_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, very_large_gaps_extreme_ops());
    assert_golden_rtl!(layout, "very_large_gaps_extreme");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn very_large_struts_extreme_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, very_large_struts_extreme_ops());
    assert_golden_rtl!(layout, "very_large_struts_extreme");
}
