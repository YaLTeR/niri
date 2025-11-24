// Golden tests for basic single column spawning
//
// This module contains both LTR and RTL tests to avoid duplication.
// Each test is run twice: once in LTR mode (with golden snapshots),
// and once in RTL mode (with calculated expectations from LTR).

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

// @niri_config("default-1-3", "default-1-3-rtl")
fn spawn_single_column_one_third_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

// @niri_config("default-1-2", "default-1-2-rtl")
fn spawn_single_column_one_half_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

// @niri_config("default-2-3", "default-2-3-rtl")
fn spawn_single_column_two_thirds_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(200.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

// @niri_config("default-fixed-width", "default-fixed-width-rtl")
fn spawn_single_column_fixed_width_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(400)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

// @niri_config("default-1-3", "default-1-3-rtl")
fn column_x_positions_single_column_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn spawn_single_column_one_third() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_single_column_one_third_ops());
    assert_golden!(layout.snapshot(), "spawn_single_column_one_third");
}

#[test]
fn spawn_single_column_one_half() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_single_column_one_half_ops());
    assert_golden!(layout.snapshot(), "spawn_single_column_one_half");
}

#[test]
fn spawn_single_column_two_thirds() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_single_column_two_thirds_ops());
    assert_golden!(layout.snapshot(), "spawn_single_column_two_thirds");
}

#[test]
fn spawn_single_column_fixed_width() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_single_column_fixed_width_ops());
    assert_golden!(layout.snapshot(), "spawn_single_column_fixed_width");
}

#[test]
fn column_x_positions_single_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, column_x_positions_single_column_ops());
    assert_golden!(layout.snapshot(), "column_x_positions_single_column");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
fn spawn_single_column_one_third_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_single_column_one_third_ops());
    assert_golden_rtl!(layout, "spawn_single_column_one_third");
}

#[test]
fn spawn_single_column_one_half_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_single_column_one_half_ops());
    assert_golden_rtl!(layout, "spawn_single_column_one_half");
}

#[test]
fn spawn_single_column_two_thirds_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_single_column_two_thirds_ops());
    assert_golden_rtl!(layout, "spawn_single_column_two_thirds");
}

#[test]
fn spawn_single_column_fixed_width_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_single_column_fixed_width_ops());
    assert_golden_rtl!(layout, "spawn_single_column_fixed_width");
}

#[test]
fn column_x_positions_single_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, column_x_positions_single_column_ops());
    assert_golden_rtl!(layout, "column_x_positions_single_column");
}
