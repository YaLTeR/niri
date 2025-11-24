// Golden tests for 350_anim_resize
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn anim_resize_adjust_proportion_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_resize_adjust_proportion_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_resize_non_preset_fixed_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_resize_non_preset_fixed_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn anim_resize_adjust_proportion_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_resize_adjust_proportion_1_ops());
    assert_golden!(layout.snapshot(), "anim_resize_adjust_proportion_1");
}

#[test]
fn anim_resize_adjust_proportion_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_resize_adjust_proportion_2_ops());
    assert_golden!(layout.snapshot(), "anim_resize_adjust_proportion_2");
}

#[test]
fn anim_resize_non_preset_fixed_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_resize_non_preset_fixed_1_ops());
    assert_golden!(layout.snapshot(), "anim_resize_non_preset_fixed_1");
}

#[test]
fn anim_resize_non_preset_fixed_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_resize_non_preset_fixed_2_ops());
    assert_golden!(layout.snapshot(), "anim_resize_non_preset_fixed_2");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_resize_adjust_proportion_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_resize_adjust_proportion_1_ops());
    assert_golden_rtl!(layout, "anim_resize_adjust_proportion_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_resize_adjust_proportion_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_resize_adjust_proportion_2_ops());
    assert_golden_rtl!(layout, "anim_resize_adjust_proportion_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_resize_non_preset_fixed_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_resize_non_preset_fixed_1_ops());
    assert_golden_rtl!(layout, "anim_resize_non_preset_fixed_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_resize_non_preset_fixed_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_resize_non_preset_fixed_2_ops());
    assert_golden_rtl!(layout, "anim_resize_non_preset_fixed_2");
}
