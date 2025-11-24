// Golden tests for 090_focus_edge_cases
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn focus_at_boundaries_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn focus_at_boundaries_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn focus_at_boundaries_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_at_boundaries_1_ops());
    assert_golden!(layout.snapshot(), "focus_at_boundaries_1");
}

#[test]
fn focus_at_boundaries_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, focus_at_boundaries_2_ops());
    assert_golden!(layout.snapshot(), "focus_at_boundaries_2");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_at_boundaries_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_at_boundaries_1_ops());
    assert_golden_rtl!(layout, "focus_at_boundaries_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn focus_at_boundaries_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, focus_at_boundaries_2_ops());
    assert_golden_rtl!(layout, "focus_at_boundaries_2");
}
