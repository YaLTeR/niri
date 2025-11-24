# RTL Calculator - Granular Property Functions

## Summary

Refactored the RTL calculator to have **individual calculation functions for each snapshot property**, enabling precise testing and verification of the math for each field.

## New Granular Functions

### Core Calculation Functions

1. **`calculate_rtl_view_pos(ltr_snapshot: &str) -> f64`**
   - Returns RTL view position
   - Currently always `0.0` (RTL scrolling not implemented)
   - Test: `test_calculate_rtl_view_pos`

2. **`calculate_rtl_column_x(ltr_snapshot: &str, column_idx: usize) -> Option<f64>`**
   - Calculates RTL X position for a specific column
   - Formula: Start from right edge, subtract column widths
   - Tests: `test_calculate_rtl_column_x_single`, `test_calculate_rtl_column_x_three_columns`

3. **`calculate_rtl_active_column_x(ltr_snapshot: &str) -> Option<f64>`**
   - Calculates RTL X position of the active column
   - Uses `calculate_rtl_column_x` with active column index
   - Test: `test_calculate_rtl_active_column_x`

4. **`calculate_rtl_active_tile_viewport_x(ltr_snapshot: &str) -> Option<f64>`**
   - Calculates active tile's X position on screen
   - Formula: `active_column_x - view_pos`
   - Test: `test_calculate_rtl_active_tile_viewport_x`

5. **`calculate_rtl_active_tile_viewport_y(ltr_snapshot: &str) -> Option<f64>`**
   - Calculates active tile's Y position on screen
   - Y is the same in LTR and RTL (no vertical mirroring)
   - Test: `test_calculate_rtl_active_tile_viewport_y`

6. **`calculate_rtl_tile_x(ltr_snapshot: &str, column_idx: usize) -> Option<f64>`**
   - Calculates RTL X position for a tile
   - Tiles are positioned at their column's X
   - Uses `calculate_rtl_column_x`

7. **`calculate_rtl_tile_y(ltr_snapshot: &str, tile_y_ltr: f64) -> f64`**
   - Calculates RTL Y position for a tile
   - Always returns the same Y (no vertical change)
   - Test: `test_calculate_rtl_tile_y`

### Convenience Function

8. **`calculate_rtl_active_positions(ltr_snapshot: &str) -> Option<(f64, f64, f64)>`**
   - Combines the three active position calculations
   - Returns `(active_column_x, active_tile_viewport_x, active_tile_viewport_y)`
   - Uses the individual functions internally

## Test Coverage

### Property-Specific Tests (7 new tests)

1. ✅ `test_calculate_rtl_view_pos` - View position (always 0)
2. ✅ `test_calculate_rtl_column_x_single` - Single column X position
3. ✅ `test_calculate_rtl_column_x_three_columns` - Multiple column X positions
4. ✅ `test_calculate_rtl_active_column_x` - Active column X position
5. ✅ `test_calculate_rtl_active_tile_viewport_x` - Active tile viewport X
6. ✅ `test_calculate_rtl_active_tile_viewport_y` - Active tile viewport Y
7. ✅ `test_calculate_rtl_tile_y` - Tile Y position (unchanged)

### Existing Tests (18 tests)

- Multi-column scenarios (2, 3, 4 columns)
- Various widths and active positions
- Gaps, struts, overflow scenarios

**Total: 25 tests, all passing!** ✅

## Example Usage

### Testing Individual Properties

```rust
// Test view_pos calculation
let view_pos = calculate_rtl_view_pos(ltr_snapshot);
assert_eq!(view_pos, 0.0);

// Test specific column X position
let col_0_x = calculate_rtl_column_x(ltr_snapshot, 0).unwrap();
assert_eq!(col_0_x, 854.0);

let col_1_x = calculate_rtl_column_x(ltr_snapshot, 1).unwrap();
assert_eq!(col_1_x, 428.0);

// Test active column X
let active_x = calculate_rtl_active_column_x(ltr_snapshot).unwrap();
assert_eq!(active_x, 428.0);

// Test viewport positions
let viewport_x = calculate_rtl_active_tile_viewport_x(ltr_snapshot).unwrap();
let viewport_y = calculate_rtl_active_tile_viewport_y(ltr_snapshot).unwrap();
assert_eq!(viewport_x, 428.0);
assert_eq!(viewport_y, 0.0);
```

## Calculation Formulas

### Column X Position (RTL)
```
x = working_area_x + working_area_width
for each column from 0 to target:
    x -= column_width
    if column == target:
        return x
    x -= gaps
```

### Active Tile Viewport X
```
active_tile_viewport_x = active_column_x - view_pos
```

Currently: `view_pos = 0`, so:
```
active_tile_viewport_x = active_column_x
```

### Tile Y Position
```
tile_y_rtl = tile_y_ltr  // No change
```

## Example Calculations

### Single Column (1/3 width = 426px)

**LTR:**
- Column 0 X: `0`
- Active column X: `0`
- Viewport X: `0`

**RTL:**
- Column 0 X: `1280 - 426 = 854`
- Active column X: `854`
- Viewport X: `854 - 0 = 854`

### Three Columns (1/3 width each)

**LTR:**
- Column 0 X: `0`
- Column 1 X: `426`
- Column 2 X: `852`

**RTL:**
- Column 0 X: `1280 - 426 = 854`
- Column 1 X: `854 - 426 = 428`
- Column 2 X: `428 - 426 = 2`

### With Gaps (16px)

**LTR (2 columns, 1/2 width with gaps):**
- Column 0 X: `0` (width 632)
- Column 1 X: `648` (632 + 16 gap)

**RTL:**
- Column 0 X: `1280 - 632 = 648`
- Column 1 X: `648 - 16 - 632 = 0`

## Benefits

1. **Testable Math** - Each calculation can be verified independently
2. **Clear Formulas** - Each function documents its specific formula
3. **Debugging** - Easy to identify which calculation is wrong
4. **Composable** - Functions build on each other logically
5. **Documentation** - Function names clearly state what they calculate
6. **Maintainable** - Changes to one property don't affect others

## Important Note

⚠️ **Current Implementation Issue:**

The RTL calculator currently calculates **visual positions** for snapshot fields like `active_column_x`. However, the golden test framework expects these fields to be **logical/content-space positions** that are IDENTICAL between LTR and RTL.

Only the **render positions** (from `format_column_edges`) should be mirrored, not the snapshot fields.

This is a fundamental issue that needs to be fixed in the actual niri RTL implementation, not just the calculator.

## Files Modified

- `/home/vince/Projects/niri/src/layout/tests/golden_tests/rtl_calculator.rs`
  - Added 7 granular calculation functions
  - Added 7 property-specific tests
  - Total: 25 tests (was 18)
  - All tests passing ✅

## Next Steps

1. **Verify Snapshot Semantics** - Determine if snapshot fields should be logical or visual
2. **Update Implementation** - Fix niri's RTL to use correct position semantics
3. **Update Calculator** - Adjust calculations based on correct semantics
4. **Update Tests** - Ensure tests verify the correct behavior
