// Golden tests for 070_window_closing
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn closing_first_of_three_fifths_preset_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_first_of_three_fifths_preset_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_first_of_three_thirds_preset_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_first_of_three_thirds_preset_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_last_of_three_fifths_preset_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_last_of_three_thirds_preset_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_leftmost_column_with_overflow_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_leftmost_column_with_overflow_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_middle_column_with_overflow_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_middle_column_with_overflow_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_middle_of_three_fifths_preset_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_middle_of_three_thirds_preset_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_mixed_preset_sizes_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_mixed_preset_sizes_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_rightmost_column_with_overflow_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_rightmost_column_with_overflow_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_with_gaps_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_with_gaps_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_with_gaps_and_struts_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn closing_with_gaps_and_struts_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn window_closing_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn window_closing_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn closing_first_of_three_fifths_preset_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_first_of_three_fifths_preset_1_ops());
    assert_golden!(layout.snapshot(), "closing_first_of_three_fifths_preset_1");
}

#[test]
fn closing_first_of_three_fifths_preset_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_first_of_three_fifths_preset_2_ops());
    assert_golden!(layout.snapshot(), "closing_first_of_three_fifths_preset_2");
}

#[test]
fn closing_first_of_three_thirds_preset_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_first_of_three_thirds_preset_1_ops());
    assert_golden!(layout.snapshot(), "closing_first_of_three_thirds_preset_1");
}

#[test]
fn closing_first_of_three_thirds_preset_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_first_of_three_thirds_preset_2_ops());
    assert_golden!(layout.snapshot(), "closing_first_of_three_thirds_preset_2");
}

#[test]
fn closing_last_of_three_fifths_preset() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_last_of_three_fifths_preset_ops());
    assert_golden!(layout.snapshot(), "closing_last_of_three_fifths_preset");
}

#[test]
fn closing_last_of_three_thirds_preset() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_last_of_three_thirds_preset_ops());
    assert_golden!(layout.snapshot(), "closing_last_of_three_thirds_preset");
}

#[test]
fn closing_leftmost_column_with_overflow_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_leftmost_column_with_overflow_1_ops());
    assert_golden!(layout.snapshot(), "closing_leftmost_column_with_overflow_1");
}

#[test]
fn closing_leftmost_column_with_overflow_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_leftmost_column_with_overflow_2_ops());
    assert_golden!(layout.snapshot(), "closing_leftmost_column_with_overflow_2");
}

#[test]
fn closing_middle_column_with_overflow_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_middle_column_with_overflow_1_ops());
    assert_golden!(layout.snapshot(), "closing_middle_column_with_overflow_1");
}

#[test]
fn closing_middle_column_with_overflow_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_middle_column_with_overflow_2_ops());
    assert_golden!(layout.snapshot(), "closing_middle_column_with_overflow_2");
}

#[test]
fn closing_middle_of_three_fifths_preset() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_middle_of_three_fifths_preset_ops());
    assert_golden!(layout.snapshot(), "closing_middle_of_three_fifths_preset");
}

#[test]
fn closing_middle_of_three_thirds_preset() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_middle_of_three_thirds_preset_ops());
    assert_golden!(layout.snapshot(), "closing_middle_of_three_thirds_preset");
}

#[test]
fn closing_mixed_preset_sizes_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_mixed_preset_sizes_1_ops());
    assert_golden!(layout.snapshot(), "closing_mixed_preset_sizes_1");
}

#[test]
fn closing_mixed_preset_sizes_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_mixed_preset_sizes_2_ops());
    assert_golden!(layout.snapshot(), "closing_mixed_preset_sizes_2");
}

#[test]
fn closing_rightmost_column_with_overflow_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_rightmost_column_with_overflow_1_ops());
    assert_golden!(layout.snapshot(), "closing_rightmost_column_with_overflow_1");
}

#[test]
fn closing_rightmost_column_with_overflow_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_rightmost_column_with_overflow_2_ops());
    assert_golden!(layout.snapshot(), "closing_rightmost_column_with_overflow_2");
}

#[test]
fn closing_with_gaps_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_with_gaps_1_ops());
    assert_golden!(layout.snapshot(), "closing_with_gaps_1");
}

#[test]
fn closing_with_gaps_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_with_gaps_2_ops());
    assert_golden!(layout.snapshot(), "closing_with_gaps_2");
}

#[test]
fn closing_with_gaps_and_struts_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_with_gaps_and_struts_1_ops());
    assert_golden!(layout.snapshot(), "closing_with_gaps_and_struts_1");
}

#[test]
fn closing_with_gaps_and_struts_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, closing_with_gaps_and_struts_2_ops());
    assert_golden!(layout.snapshot(), "closing_with_gaps_and_struts_2");
}

#[test]
fn window_closing_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, window_closing_1_ops());
    assert_golden!(layout.snapshot(), "window_closing_1");
}

#[test]
fn window_closing_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, window_closing_2_ops());
    assert_golden!(layout.snapshot(), "window_closing_2");
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
fn closing_first_of_three_fifths_preset_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_first_of_three_fifths_preset_1_ops());
    assert_golden_rtl!(layout, "closing_first_of_three_fifths_preset_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_first_of_three_fifths_preset_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_first_of_three_fifths_preset_2_ops());
    assert_golden_rtl!(layout, "closing_first_of_three_fifths_preset_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_first_of_three_thirds_preset_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_first_of_three_thirds_preset_1_ops());
    assert_golden_rtl!(layout, "closing_first_of_three_thirds_preset_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_first_of_three_thirds_preset_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_first_of_three_thirds_preset_2_ops());
    assert_golden_rtl!(layout, "closing_first_of_three_thirds_preset_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_last_of_three_fifths_preset_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_last_of_three_fifths_preset_ops());
    assert_golden_rtl!(layout, "closing_last_of_three_fifths_preset");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_last_of_three_thirds_preset_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_last_of_three_thirds_preset_ops());
    assert_golden_rtl!(layout, "closing_last_of_three_thirds_preset");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_leftmost_column_with_overflow_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_leftmost_column_with_overflow_1_ops());
    assert_golden_rtl!(layout, "closing_leftmost_column_with_overflow_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_leftmost_column_with_overflow_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_leftmost_column_with_overflow_2_ops());
    assert_golden_rtl!(layout, "closing_leftmost_column_with_overflow_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_middle_column_with_overflow_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_middle_column_with_overflow_1_ops());
    assert_golden_rtl!(layout, "closing_middle_column_with_overflow_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_middle_column_with_overflow_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_middle_column_with_overflow_2_ops());
    assert_golden_rtl!(layout, "closing_middle_column_with_overflow_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_middle_of_three_fifths_preset_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_middle_of_three_fifths_preset_ops());
    assert_golden_rtl!(layout, "closing_middle_of_three_fifths_preset");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_middle_of_three_thirds_preset_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_middle_of_three_thirds_preset_ops());
    assert_golden_rtl!(layout, "closing_middle_of_three_thirds_preset");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_mixed_preset_sizes_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_mixed_preset_sizes_1_ops());
    assert_golden_rtl!(layout, "closing_mixed_preset_sizes_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_mixed_preset_sizes_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_mixed_preset_sizes_2_ops());
    assert_golden_rtl!(layout, "closing_mixed_preset_sizes_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_rightmost_column_with_overflow_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_rightmost_column_with_overflow_1_ops());
    assert_golden_rtl!(layout, "closing_rightmost_column_with_overflow_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_rightmost_column_with_overflow_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_rightmost_column_with_overflow_2_ops());
    assert_golden_rtl!(layout, "closing_rightmost_column_with_overflow_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_with_gaps_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_with_gaps_1_ops());
    assert_golden_rtl!(layout, "closing_with_gaps_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_with_gaps_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_with_gaps_2_ops());
    assert_golden_rtl!(layout, "closing_with_gaps_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_with_gaps_and_struts_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_with_gaps_and_struts_1_ops());
    assert_golden_rtl!(layout, "closing_with_gaps_and_struts_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn closing_with_gaps_and_struts_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, closing_with_gaps_and_struts_2_ops());
    assert_golden_rtl!(layout, "closing_with_gaps_and_struts_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn window_closing_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, window_closing_1_ops());
    assert_golden_rtl!(layout, "window_closing_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn window_closing_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, window_closing_2_ops());
    assert_golden_rtl!(layout, "window_closing_2");
}
