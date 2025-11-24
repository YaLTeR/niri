// Golden tests for 330_anim_column_movement
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn anim_move_column_left_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_left_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_left_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_right_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_right_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_right_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_right_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_to_first_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_to_first_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_to_last_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn anim_move_column_to_last_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn anim_move_column_left_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_left_1_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_left_1");
}

#[test]
fn anim_move_column_left_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_left_2_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_left_2");
}

#[test]
fn anim_move_column_left_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_left_3_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_left_3");
}

#[test]
fn anim_move_column_right_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_right_1_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_right_1");
}

#[test]
fn anim_move_column_right_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_right_2_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_right_2");
}

#[test]
fn anim_move_column_right_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_right_3_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_right_3");
}

#[test]
fn anim_move_column_right_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_right_4_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_right_4");
}

#[test]
fn anim_move_column_to_first_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_to_first_1_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_to_first_1");
}

#[test]
fn anim_move_column_to_first_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_to_first_2_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_to_first_2");
}

#[test]
fn anim_move_column_to_last_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_to_last_1_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_to_last_1");
}

#[test]
fn anim_move_column_to_last_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, anim_move_column_to_last_2_ops());
    assert_golden!(layout.snapshot(), "anim_move_column_to_last_2");
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
fn anim_move_column_left_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_left_1_ops());
    assert_golden_rtl!(layout, "anim_move_column_left_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_left_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_left_2_ops());
    assert_golden_rtl!(layout, "anim_move_column_left_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_left_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_left_3_ops());
    assert_golden_rtl!(layout, "anim_move_column_left_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_right_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_right_1_ops());
    assert_golden_rtl!(layout, "anim_move_column_right_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_right_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_right_2_ops());
    assert_golden_rtl!(layout, "anim_move_column_right_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_right_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_right_3_ops());
    assert_golden_rtl!(layout, "anim_move_column_right_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_right_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_right_4_ops());
    assert_golden_rtl!(layout, "anim_move_column_right_4");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_to_first_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_to_first_1_ops());
    assert_golden_rtl!(layout, "anim_move_column_to_first_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_to_first_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_to_first_2_ops());
    assert_golden_rtl!(layout, "anim_move_column_to_first_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_to_last_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_to_last_1_ops());
    assert_golden_rtl!(layout, "anim_move_column_to_last_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn anim_move_column_to_last_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, anim_move_column_to_last_2_ops());
    assert_golden_rtl!(layout, "anim_move_column_to_last_2");
}
