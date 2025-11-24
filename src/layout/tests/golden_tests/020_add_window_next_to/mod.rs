// Golden tests for 020_add_window_next_to
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn spawn_window_between_three_columns_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn spawn_window_between_two_columns_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn spawn_window_between_two_columns_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn spawn_window_between_with_gaps_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn spawn_window_between_with_mixed_sizes_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn spawn_window_between_with_overflow_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn spawn_window_between_with_overflow_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn spawn_window_between_three_columns() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_window_between_three_columns_ops());
    assert_golden!(layout.snapshot(), "spawn_window_between_three_columns");
}

#[test]
fn spawn_window_between_two_columns_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_window_between_two_columns_1_ops());
    assert_golden!(layout.snapshot(), "spawn_window_between_two_columns_1");
}

#[test]
fn spawn_window_between_two_columns_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_window_between_two_columns_2_ops());
    assert_golden!(layout.snapshot(), "spawn_window_between_two_columns_2");
}

#[test]
fn spawn_window_between_with_gaps() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_window_between_with_gaps_ops());
    assert_golden!(layout.snapshot(), "spawn_window_between_with_gaps");
}

#[test]
fn spawn_window_between_with_mixed_sizes() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_window_between_with_mixed_sizes_ops());
    assert_golden!(layout.snapshot(), "spawn_window_between_with_mixed_sizes");
}

#[test]
fn spawn_window_between_with_overflow_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_window_between_with_overflow_1_ops());
    assert_golden!(layout.snapshot(), "spawn_window_between_with_overflow_1");
}

#[test]
fn spawn_window_between_with_overflow_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_window_between_with_overflow_2_ops());
    assert_golden!(layout.snapshot(), "spawn_window_between_with_overflow_2");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn spawn_window_between_three_columns_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_window_between_three_columns_ops());
    assert_golden_rtl!(layout, "spawn_window_between_three_columns");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn spawn_window_between_two_columns_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_window_between_two_columns_1_ops());
    assert_golden_rtl!(layout, "spawn_window_between_two_columns_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn spawn_window_between_two_columns_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_window_between_two_columns_2_ops());
    assert_golden_rtl!(layout, "spawn_window_between_two_columns_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn spawn_window_between_with_gaps_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_window_between_with_gaps_ops());
    assert_golden_rtl!(layout, "spawn_window_between_with_gaps");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn spawn_window_between_with_mixed_sizes_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_window_between_with_mixed_sizes_ops());
    assert_golden_rtl!(layout, "spawn_window_between_with_mixed_sizes");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn spawn_window_between_with_overflow_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_window_between_with_overflow_1_ops());
    assert_golden_rtl!(layout, "spawn_window_between_with_overflow_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn spawn_window_between_with_overflow_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_window_between_with_overflow_2_ops());
    assert_golden_rtl!(layout, "spawn_window_between_with_overflow_2");
}
