// Golden tests for multiple column spawning
//
// This module tests spawning multiple columns incrementally.
// Each spawn operation is executed only once, with snapshots taken after each step.

use super::*;

// ============================================================================
// Incremental Operations
// ============================================================================

/// Spawn a single 1/3 width column
fn spawn_one_third_column_ops(id: usize) -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(id) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ]
}

/// Spawn a single 1/2 width column
fn spawn_one_half_column_ops(id: usize) -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(id) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn spawn_one_third_columns() {
    let mut layout = set_up_empty();

    // 1 column: [1/3 tile, 2/3 empty]
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(1));
    assert_golden!(layout.snapshot(), "spawn_one_third_one_tile");

    // 2 columns: [1/3 tile, 1/3 tile, 1/3 empty]
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(2));
    assert_golden!(layout.snapshot(), "spawn_one_third_two_tiles");

    // 3 columns: [1/3 tile, 1/3 tile, 1/3 tile] (workspace full)
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(3));
    assert_golden!(layout.snapshot(), "spawn_one_third_three_tiles");

    // 4 columns: first tile now out of bounds on the left
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(4));
    assert_golden!(layout.snapshot(), "spawn_one_third_four_tiles");
}

#[test]
fn spawn_one_half_columns() {
    let mut layout = set_up_empty();

    // 1 column: [1/2 tile, 1/2 empty]
    check_ops_on_layout(&mut layout, spawn_one_half_column_ops(1));
    assert_golden!(layout.snapshot(), "spawn_one_half_one_tile");

    // 2 columns: [1/2 tile, 1/2 tile] (workspace full)
    check_ops_on_layout(&mut layout, spawn_one_half_column_ops(2));
    assert_golden!(layout.snapshot(), "spawn_one_half_two_tiles");

    // 3 columns: first tile now out of bounds on the left
    check_ops_on_layout(&mut layout, spawn_one_half_column_ops(3));
    assert_golden!(layout.snapshot(), "spawn_one_half_three_tiles");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
fn spawn_one_third_columns_rtl() {
    let mut layout = set_up_empty_rtl();

    // 1 column: [2/3 empty, 1/3 tile]
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(1));
    assert_golden_rtl!(layout, "spawn_one_third_one_tile");

    // 2 columns: [1/3 empty, 1/3 tile, 1/3 tile]
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(2));
    assert_golden_rtl!(layout, "spawn_one_third_two_tiles");

    // 3 columns: [1/3 tile, 1/3 tile, 1/3 tile] (workspace full)
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(3));
    assert_golden_rtl!(layout, "spawn_one_third_three_tiles");

    // 4 columns: first tile now out of bounds on the right
    check_ops_on_layout(&mut layout, spawn_one_third_column_ops(4));
    assert_golden_rtl!(layout, "spawn_one_third_four_tiles");
}

#[test]
fn spawn_one_half_columns_rtl() {
    let mut layout = set_up_empty_rtl();

    // 1 column: [1/2 empty, 1/2 tile]
    check_ops_on_layout(&mut layout, spawn_one_half_column_ops(1));
    assert_golden_rtl!(layout, "spawn_one_half_one_tile");

    // 2 columns: [1/2 tile, 1/2 tile] (workspace full)
    check_ops_on_layout(&mut layout, spawn_one_half_column_ops(2));
    assert_golden_rtl!(layout, "spawn_one_half_two_tiles");

    // 3 columns: first tile now out of bounds on the right
    check_ops_on_layout(&mut layout, spawn_one_half_column_ops(3));
    assert_golden_rtl!(layout, "spawn_one_half_three_tiles");
}
