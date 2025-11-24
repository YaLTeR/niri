// Golden tests for 360_anim_preset_width
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn preset_width_anim_left_edge_pinned_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_anim_left_edge_pinned_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_anim_left_edge_pinned_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_anim_with_multiple_columns_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn preset_width_anim_left_edge_pinned_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_anim_left_edge_pinned_1_ops());
    assert_golden!(layout.snapshot(), "preset_width_anim_left_edge_pinned_1");
}

#[test]
fn preset_width_anim_left_edge_pinned_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_anim_left_edge_pinned_2_ops());
    assert_golden!(layout.snapshot(), "preset_width_anim_left_edge_pinned_2");
}

#[test]
fn preset_width_anim_left_edge_pinned_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_anim_left_edge_pinned_3_ops());
    assert_golden!(layout.snapshot(), "preset_width_anim_left_edge_pinned_3");
}

#[test]
fn preset_width_anim_with_multiple_columns() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_anim_with_multiple_columns_ops());
    assert_golden!(layout.snapshot(), "preset_width_anim_with_multiple_columns");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_anim_left_edge_pinned_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_anim_left_edge_pinned_1_ops());
    assert_golden_rtl!(layout, "preset_width_anim_left_edge_pinned_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_anim_left_edge_pinned_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_anim_left_edge_pinned_2_ops());
    assert_golden_rtl!(layout, "preset_width_anim_left_edge_pinned_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_anim_left_edge_pinned_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_anim_left_edge_pinned_3_ops());
    assert_golden_rtl!(layout, "preset_width_anim_left_edge_pinned_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_anim_with_multiple_columns_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_anim_with_multiple_columns_ops());
    assert_golden_rtl!(layout, "preset_width_anim_with_multiple_columns");
}
