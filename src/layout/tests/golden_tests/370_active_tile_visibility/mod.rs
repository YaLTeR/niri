// Golden tests for 370_active_tile_visibility
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn active_tile_viewport_y_position_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn active_tile_visibility_with_large_columns_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn active_tile_visible_after_scroll_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn active_tile_viewport_y_position() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, active_tile_viewport_y_position_ops());
    assert_golden!(layout.snapshot(), "active_tile_viewport_y_position");
}

#[test]
fn active_tile_visibility_with_large_columns() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, active_tile_visibility_with_large_columns_ops());
    assert_golden!(layout.snapshot(), "active_tile_visibility_with_large_columns");
}

#[test]
fn active_tile_visible_after_scroll() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, active_tile_visible_after_scroll_ops());
    assert_golden!(layout.snapshot(), "active_tile_visible_after_scroll");
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
fn active_tile_viewport_y_position_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, active_tile_viewport_y_position_ops());
    assert_golden_rtl!(layout, "active_tile_viewport_y_position");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn active_tile_visibility_with_large_columns_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, active_tile_visibility_with_large_columns_ops());
    assert_golden_rtl!(layout, "active_tile_visibility_with_large_columns");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn active_tile_visible_after_scroll_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, active_tile_visible_after_scroll_ops());
    assert_golden_rtl!(layout, "active_tile_visible_after_scroll");
}
