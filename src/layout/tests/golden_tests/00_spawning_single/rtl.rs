// RTL tests for basic single column spawning
//
// These tests verify that RTL behavior is the mathematical mirror of LTR.
// They derive expected values from LTR golden snapshots rather than having
// separate RTL snapshots.

use super::*;

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
fn spawn_single_column_one_third_rtl() {
    let mut layout = set_up_empty_rtl();

    // Run same operations as LTR test
    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    assert_golden_rtl!(layout, "spawn_single_column_one_third");
}

#[test]
fn spawn_single_column_one_half_rtl() {
    let mut layout = set_up_empty_rtl();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(50.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    assert_golden_rtl!(layout, "spawn_single_column_one_half");
}

#[test]
fn spawn_single_column_two_thirds_rtl() {
    let mut layout = set_up_empty_rtl();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(200.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    assert_golden_rtl!(layout, "spawn_single_column_two_thirds");
}

#[test]
fn spawn_single_column_fixed_width_rtl() {
    let mut layout = set_up_empty_rtl();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetFixed(400)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    assert_golden_rtl!(layout, "spawn_single_column_fixed_width");
}

#[test]
fn column_x_positions_single_column_rtl() {
    let mut layout = set_up_empty_rtl();

    let ops = [
        Op::AddWindow { params: TestWindowParams::new(1) },
        Op::SetColumnWidth(SizeChange::SetProportion(100.0 / 3.0)),
        Op::Communicate(1),
        Op::CompleteAnimations,
    ];
    check_ops_on_layout(&mut layout, ops);

    assert_golden_rtl!(layout, "column_x_positions_single_column");
}
