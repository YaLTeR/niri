// Golden tests for 150_preset_width
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn preset_cycling_middle_column_with_overflow_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_middle_column_with_overflow_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_middle_column_with_overflow_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_middle_column_with_overflow_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_rightmost_column_with_overflow_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_rightmost_column_with_overflow_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_rightmost_column_with_overflow_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_rightmost_column_with_overflow_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_two_fifths_to_four_fifths_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_two_fifths_to_four_fifths_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_two_fifths_to_four_fifths_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_cycling_two_fifths_to_four_fifths_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_gaps_struts_combined_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_two_fifths_gaps_struts_combined_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_index_cycles_backward_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_index_cycles_backward_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_index_cycles_correctly_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_index_cycles_correctly_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_index_cycles_correctly_3_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn preset_width_index_cycles_correctly_4_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn preset_cycling_middle_column_with_overflow_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_1_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_middle_column_with_overflow_1");
}

#[test]
fn preset_cycling_middle_column_with_overflow_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_2_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_middle_column_with_overflow_2");
}

#[test]
fn preset_cycling_middle_column_with_overflow_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_3_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_middle_column_with_overflow_3");
}

#[test]
fn preset_cycling_middle_column_with_overflow_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_4_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_middle_column_with_overflow_4");
}

#[test]
fn preset_cycling_rightmost_column_with_overflow_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_1_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_rightmost_column_with_overflow_1");
}

#[test]
fn preset_cycling_rightmost_column_with_overflow_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_2_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_rightmost_column_with_overflow_2");
}

#[test]
fn preset_cycling_rightmost_column_with_overflow_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_3_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_rightmost_column_with_overflow_3");
}

#[test]
fn preset_cycling_rightmost_column_with_overflow_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_4_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_rightmost_column_with_overflow_4");
}

#[test]
fn preset_cycling_two_fifths_to_four_fifths_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_1_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_two_fifths_to_four_fifths_1");
}

#[test]
fn preset_cycling_two_fifths_to_four_fifths_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_2_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_two_fifths_to_four_fifths_2");
}

#[test]
fn preset_cycling_two_fifths_to_four_fifths_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_3_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_two_fifths_to_four_fifths_3");
}

#[test]
fn preset_cycling_two_fifths_to_four_fifths_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_4_ops());
    assert_golden!(layout.snapshot(), "preset_cycling_two_fifths_to_four_fifths_4");
}

#[test]
fn preset_two_fifths_gaps_struts_combined_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_1_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_gaps_struts_combined_1");
}

#[test]
fn preset_two_fifths_gaps_struts_combined_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_2_ops());
    assert_golden!(layout.snapshot(), "preset_two_fifths_gaps_struts_combined_2");
}

#[test]
fn preset_width_index_cycles_backward_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_backward_1_ops());
    assert_golden!(layout.snapshot(), "preset_width_index_cycles_backward_1");
}

#[test]
fn preset_width_index_cycles_backward_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_backward_2_ops());
    assert_golden!(layout.snapshot(), "preset_width_index_cycles_backward_2");
}

#[test]
fn preset_width_index_cycles_correctly_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_1_ops());
    assert_golden!(layout.snapshot(), "preset_width_index_cycles_correctly_1");
}

#[test]
fn preset_width_index_cycles_correctly_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_2_ops());
    assert_golden!(layout.snapshot(), "preset_width_index_cycles_correctly_2");
}

#[test]
fn preset_width_index_cycles_correctly_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_3_ops());
    assert_golden!(layout.snapshot(), "preset_width_index_cycles_correctly_3");
}

#[test]
fn preset_width_index_cycles_correctly_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_4_ops());
    assert_golden!(layout.snapshot(), "preset_width_index_cycles_correctly_4");
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
fn preset_cycling_middle_column_with_overflow_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_1_ops());
    assert_golden_rtl!(layout, "preset_cycling_middle_column_with_overflow_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_middle_column_with_overflow_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_2_ops());
    assert_golden_rtl!(layout, "preset_cycling_middle_column_with_overflow_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_middle_column_with_overflow_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_3_ops());
    assert_golden_rtl!(layout, "preset_cycling_middle_column_with_overflow_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_middle_column_with_overflow_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_middle_column_with_overflow_4_ops());
    assert_golden_rtl!(layout, "preset_cycling_middle_column_with_overflow_4");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_rightmost_column_with_overflow_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_1_ops());
    assert_golden_rtl!(layout, "preset_cycling_rightmost_column_with_overflow_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_rightmost_column_with_overflow_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_2_ops());
    assert_golden_rtl!(layout, "preset_cycling_rightmost_column_with_overflow_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_rightmost_column_with_overflow_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_3_ops());
    assert_golden_rtl!(layout, "preset_cycling_rightmost_column_with_overflow_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_rightmost_column_with_overflow_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_rightmost_column_with_overflow_4_ops());
    assert_golden_rtl!(layout, "preset_cycling_rightmost_column_with_overflow_4");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_two_fifths_to_four_fifths_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_1_ops());
    assert_golden_rtl!(layout, "preset_cycling_two_fifths_to_four_fifths_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_two_fifths_to_four_fifths_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_2_ops());
    assert_golden_rtl!(layout, "preset_cycling_two_fifths_to_four_fifths_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_two_fifths_to_four_fifths_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_3_ops());
    assert_golden_rtl!(layout, "preset_cycling_two_fifths_to_four_fifths_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_cycling_two_fifths_to_four_fifths_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_cycling_two_fifths_to_four_fifths_4_ops());
    assert_golden_rtl!(layout, "preset_cycling_two_fifths_to_four_fifths_4");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_gaps_struts_combined_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_1_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_gaps_struts_combined_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_two_fifths_gaps_struts_combined_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_two_fifths_gaps_struts_combined_2_ops());
    assert_golden_rtl!(layout, "preset_two_fifths_gaps_struts_combined_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_index_cycles_backward_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_backward_1_ops());
    assert_golden_rtl!(layout, "preset_width_index_cycles_backward_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_index_cycles_backward_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_backward_2_ops());
    assert_golden_rtl!(layout, "preset_width_index_cycles_backward_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_index_cycles_correctly_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_1_ops());
    assert_golden_rtl!(layout, "preset_width_index_cycles_correctly_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_index_cycles_correctly_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_2_ops());
    assert_golden_rtl!(layout, "preset_width_index_cycles_correctly_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_index_cycles_correctly_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_3_ops());
    assert_golden_rtl!(layout, "preset_width_index_cycles_correctly_3");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn preset_width_index_cycles_correctly_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, preset_width_index_cycles_correctly_4_ops());
    assert_golden_rtl!(layout, "preset_width_index_cycles_correctly_4");
}
