// Golden tests for multiple column spawning
//
// This module contains both LTR and RTL tests to avoid duplication.
// Each test is run twice: once in LTR mode (with golden snapshots),
// and once in RTL mode (with calculated expectations from LTR).

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn spawn_one_third_one_tile_ops() -> Vec<Op> {
    vec![
        // spawn a 1/3 tile result [1/3 tile, 2/3 empty]
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_third_two_tiles_ops() -> Vec<Op> {
    vec![
        // spawn first 1/3 tile
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
        // spawn a second 1/3 tile [1/3 first tile, 1/3 new tile, 1/3 empty]
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_third_three_tiles_ops() -> Vec<Op> {
    vec![
        // spawn first 1/3 tile
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
        // spawn second 1/3 tile
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
        // spawn a third 1/3 tile [1/3 first tile, 1/3 second tile, 1/3 new tile] (workspace full)
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_third_four_tiles_ops() -> Vec<Op> {
    vec![
        // spawn first 1/3 tile
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
        // spawn second 1/3 tile
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
        // spawn third 1/3 tile
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
        // spawn a fourth 1/3 tile (workspace full), the first tile is now out of bounds on the left side) did the camera move?
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_half_one_tile_ops() -> Vec<Op> {
    vec![
        // spawn a 1/2 tile [1/2 tile, 1/2 empty]
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_half_two_tiles_ops() -> Vec<Op> {
    vec![
        // spawn first 1/2 tile
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
        // spawn a second tile [1/2 first tile, 1/2 new tile] (workspace full)
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_half_three_tiles_ops() -> Vec<Op> {
    vec![
        // spawn first 1/2 tile
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
        // spawn second 1/2 tile
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
        // spawn a third tile (workspace still full) the first tile is now out of bounds on the left side) did the camera move?
        Op::AddWindow { params: TestWindowParams::new(3) },
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
fn spawn_one_third_one_tile() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_one_tile_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_one_tile");
}

#[test]
fn spawn_one_third_two_tiles() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_two_tiles_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_two_tiles");
}

#[test]
fn spawn_one_third_three_tiles() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_three_tiles_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_three_tiles");
}

#[test]
fn spawn_one_third_four_tiles() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_four_tiles_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_four_tiles");
}

#[test]
fn spawn_one_half_one_tile() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_half_one_tile_ops());
    assert_golden!(layout.snapshot(), "spawn_one_half_one_tile");
}

#[test]
fn spawn_one_half_two_tiles() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_half_two_tiles_ops());
    assert_golden!(layout.snapshot(), "spawn_one_half_two_tiles");
}

#[test]
fn spawn_one_half_three_tiles() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_half_three_tiles_ops());
    assert_golden!(layout.snapshot(), "spawn_one_half_three_tiles");
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
fn spawn_one_third_one_tile_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_one_tile_ops());
    assert_golden_rtl!(layout, "spawn_one_third_one_tile");
}

#[test]
fn spawn_one_third_two_tiles_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_two_tiles_ops());
    assert_golden_rtl!(layout, "spawn_one_third_two_tiles");
}

#[test]
fn spawn_one_third_three_tiles_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_three_tiles_ops());
    assert_golden_rtl!(layout, "spawn_one_third_three_tiles");
}

#[test]
fn spawn_one_third_four_tiles_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_four_tiles_ops());
    assert_golden_rtl!(layout, "spawn_one_third_four_tiles");
}

#[test]
fn spawn_one_half_one_tile_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_half_one_tile_ops());
    assert_golden_rtl!(layout, "spawn_one_half_one_tile");
}

#[test]
fn spawn_one_half_two_tiles_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_half_two_tiles_ops());
    assert_golden_rtl!(layout, "spawn_one_half_two_tiles");
}

#[test]
fn spawn_one_half_three_tiles_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_half_three_tiles_ops());
    assert_golden_rtl!(layout, "spawn_one_half_three_tiles");
}
