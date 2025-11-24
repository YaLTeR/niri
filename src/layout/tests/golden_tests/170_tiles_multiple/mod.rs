// Golden tests for 170_tiles_multiple
//
// Auto-generated stub. Customize as needed.

use super::*;

// ============================================================================
// Test Operations
// ============================================================================

fn four_tiles_in_column_equal_height_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn three_tiles_in_column_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn tiles_with_mixed_heights_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

fn tiles_with_preset_heights_ops() -> Vec<Op> {
    // TODO: Define test operations
    vec![]
}

// ============================================================================
// LTR Tests
// ============================================================================

#[test]
fn four_tiles_in_column_equal_height() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, four_tiles_in_column_equal_height_ops());
    assert_golden!(layout.snapshot(), "four_tiles_in_column_equal_height");
}

#[test]
fn three_tiles_in_column() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, three_tiles_in_column_ops());
    assert_golden!(layout.snapshot(), "three_tiles_in_column");
}

#[test]
fn tiles_with_mixed_heights() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, tiles_with_mixed_heights_ops());
    assert_golden!(layout.snapshot(), "tiles_with_mixed_heights");
}

#[test]
fn tiles_with_preset_heights() {
    let mut layout = set_up_empty();
    check_ops_on_layout(&mut layout, tiles_with_preset_heights_ops());
    assert_golden!(layout.snapshot(), "tiles_with_preset_heights");
}

// ============================================================================
// RTL Tests
// ============================================================================



#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn four_tiles_in_column_equal_height_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, four_tiles_in_column_equal_height_ops());
    assert_golden_rtl!(layout, "four_tiles_in_column_equal_height");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn three_tiles_in_column_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, three_tiles_in_column_ops());
    assert_golden_rtl!(layout, "three_tiles_in_column");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn tiles_with_mixed_heights_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, tiles_with_mixed_heights_ops());
    assert_golden_rtl!(layout, "tiles_with_mixed_heights");
}
#[test]
#[ignore = "RTL scrolling not yet implemented"]
fn tiles_with_preset_heights_rtl() {
    let mut layout = set_up_empty_rtl();
    check_ops_on_layout(&mut layout, tiles_with_preset_heights_ops());
    assert_golden_rtl!(layout, "tiles_with_preset_heights");
}
