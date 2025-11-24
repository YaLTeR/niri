// Golden tests for 340_anim_view_offset
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn anim_view_offset_focus_left_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_view_offset_focus_right_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_view_offset_focus_right_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn anim_view_offset_focus_left() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_view_offset_focus_left_ops());
    assert_golden!(layout.snapshot(), "anim_view_offset_focus_left");
}

#[test]
fn anim_view_offset_focus_right_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_view_offset_focus_right_1_ops());
    assert_golden!(layout.snapshot(), "anim_view_offset_focus_right_1");
}

#[test]
fn anim_view_offset_focus_right_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_view_offset_focus_right_2_ops());
    assert_golden!(layout.snapshot(), "anim_view_offset_focus_right_2");
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
fn anim_view_offset_focus_left_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_view_offset_focus_left_ops());
    assert_golden_rtl!(layout, "anim_view_offset_focus_left");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_view_offset_focus_right_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_view_offset_focus_right_1_ops());
    assert_golden_rtl!(layout, "anim_view_offset_focus_right_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_view_offset_focus_right_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_view_offset_focus_right_2_ops());
    assert_golden_rtl!(layout, "anim_view_offset_focus_right_2");
}
