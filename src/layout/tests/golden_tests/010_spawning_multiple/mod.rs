//! Golden tests for 010_spawning_multiple
//!
//! Test ops are manually maintained - do not auto-generate.
use super::*;

fn spawn_one_half_tiles_1_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_half_tiles_2_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_half_tiles_3_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_third_tiles_1_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_third_tiles_2_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_third_tiles_3_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::CompleteAnimations,
    ]
}

fn spawn_one_third_tiles_4_ops() -> Vec<Op> {
    vec![
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(2) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(3) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::AddWindow { params: TestWindowParams::new(4) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::Communicate(2),
        Op::Communicate(3),
        Op::Communicate(4),
        Op::CompleteAnimations,
    ]
}
#[test]
fn spawn_one_half_tiles_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_half_tiles_1_ops());
    assert_golden!(layout.snapshot(), "spawn_one_half_tiles_1");
}
#[test]
fn spawn_one_half_tiles_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_half_tiles_2_ops());
    assert_golden!(layout.snapshot(), "spawn_one_half_tiles_2");
}
#[test]
fn spawn_one_half_tiles_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_half_tiles_3_ops());
    assert_golden!(layout.snapshot(), "spawn_one_half_tiles_3");
}
#[test]
fn spawn_one_third_tiles_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_1_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_tiles_1");
}
#[test]
fn spawn_one_third_tiles_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_2_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_tiles_2");
}
#[test]
fn spawn_one_third_tiles_3() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_3_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_tiles_3");
}
#[test]
fn spawn_one_third_tiles_4() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_4_ops());
    assert_golden!(layout.snapshot(), "spawn_one_third_tiles_4");
}
#[test]
fn spawn_one_half_tiles_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_half_tiles_1_ops());
    assert_golden_rtl!(layout, "spawn_one_half_tiles_1");
}
#[test]
fn spawn_one_half_tiles_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_half_tiles_2_ops());
    assert_golden_rtl!(layout, "spawn_one_half_tiles_2");
}
#[test]
fn spawn_one_half_tiles_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_half_tiles_3_ops());
    assert_golden_rtl!(layout, "spawn_one_half_tiles_3");
}
#[test]
fn spawn_one_third_tiles_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_1_ops());
    assert_golden_rtl!(layout, "spawn_one_third_tiles_1");
}
#[test]
fn spawn_one_third_tiles_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_2_ops());
    assert_golden_rtl!(layout, "spawn_one_third_tiles_2");
}
#[test]
fn spawn_one_third_tiles_3_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_3_ops());
    assert_golden_rtl!(layout, "spawn_one_third_tiles_3");
}
#[test]
fn spawn_one_third_tiles_4_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, spawn_one_third_tiles_4_ops());
    assert_golden_rtl!(layout, "spawn_one_third_tiles_4");
}
