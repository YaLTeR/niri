// Golden tests for 050_leading_edge
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn leading_edge_pinned_middle_column_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn leading_edge_pinned_on_resize_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn leading_edge_pinned_on_resize_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn leading_edge_pinned_on_spawn_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn leading_edge_pinned_on_spawn_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn leading_edge_pinned_with_multiple_columns_1_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn leading_edge_pinned_with_multiple_columns_2_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn leading_edge_pinned_middle_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, leading_edge_pinned_middle_column_ops());
    assert_golden!(layout.snapshot(), "leading_edge_pinned_middle_column");
}

#[test]
fn leading_edge_pinned_on_resize_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_resize_1_ops());
    assert_golden!(layout.snapshot(), "leading_edge_pinned_on_resize_1");
}

#[test]
fn leading_edge_pinned_on_resize_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_resize_2_ops());
    assert_golden!(layout.snapshot(), "leading_edge_pinned_on_resize_2");
}

#[test]
fn leading_edge_pinned_on_spawn_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_spawn_1_ops());
    assert_golden!(layout.snapshot(), "leading_edge_pinned_on_spawn_1");
}

#[test]
fn leading_edge_pinned_on_spawn_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_spawn_2_ops());
    assert_golden!(layout.snapshot(), "leading_edge_pinned_on_spawn_2");
}

#[test]
fn leading_edge_pinned_with_multiple_columns_1() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, leading_edge_pinned_with_multiple_columns_1_ops());
    assert_golden!(layout.snapshot(), "leading_edge_pinned_with_multiple_columns_1");
}

#[test]
fn leading_edge_pinned_with_multiple_columns_2() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, leading_edge_pinned_with_multiple_columns_2_ops());
    assert_golden!(layout.snapshot(), "leading_edge_pinned_with_multiple_columns_2");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn leading_edge_pinned_middle_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, leading_edge_pinned_middle_column_ops());
    assert_golden_rtl!(layout, "leading_edge_pinned_middle_column");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn leading_edge_pinned_on_resize_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_resize_1_ops());
    assert_golden_rtl!(layout, "leading_edge_pinned_on_resize_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn leading_edge_pinned_on_resize_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_resize_2_ops());
    assert_golden_rtl!(layout, "leading_edge_pinned_on_resize_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn leading_edge_pinned_on_spawn_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_spawn_1_ops());
    assert_golden_rtl!(layout, "leading_edge_pinned_on_spawn_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn leading_edge_pinned_on_spawn_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, leading_edge_pinned_on_spawn_2_ops());
    assert_golden_rtl!(layout, "leading_edge_pinned_on_spawn_2");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn leading_edge_pinned_with_multiple_columns_1_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, leading_edge_pinned_with_multiple_columns_1_ops());
    assert_golden_rtl!(layout, "leading_edge_pinned_with_multiple_columns_1");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn leading_edge_pinned_with_multiple_columns_2_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, leading_edge_pinned_with_multiple_columns_2_ops());
    assert_golden_rtl!(layout, "leading_edge_pinned_with_multiple_columns_2");
}
