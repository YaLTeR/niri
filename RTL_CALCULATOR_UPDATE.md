# RTL Calculator Update - Active Position Support

## Summary

Updated `rtl_calculator.rs` to calculate RTL positions for the new snapshot fields including active column and tile viewport positions.

## New Function

### `calculate_rtl_active_positions(ltr_snapshot: &str) -> Option<(f64, f64, f64)>`

Calculates RTL positions for:
- **`active_column_x`** - X position of the active column in RTL
- **`active_tile_viewport_x`** - X position of active tile on screen in RTL
- **`active_tile_viewport_y`** - Y position (same in LTR and RTL)

## How It Works

### RTL Column Positioning

In RTL, columns grow from **right to left**:

```
LTR:  [Col0][Col1][Col2]
      0     426   852

RTL:  [Col2][Col1][Col0]
      0/2   428   854
```

### Calculation Algorithm

1. Parse active column index from snapshot
2. Parse all column widths
3. Start from right edge: `x = working_area_x + working_area_width`
4. For each column (in order):
   - Subtract column width: `x -= column_width`
   - If this is the active column, record position
   - Subtract gap: `x -= gaps`
5. Return active column position

### Active Tile Viewport Position

Since RTL scrolling is not yet implemented (view_pos is always 0):
```rust
active_tile_viewport_x = active_column_x - view_pos
                       = active_column_x - 0
                       = active_column_x
```

## Examples

### Single 1/3 Width Column

**LTR:**
```
active_column_x=0.0
active_tile_viewport_x=0.0
```

**RTL:**
```
active_column_x=854.0        // 1280 - 426 = 854
active_tile_viewport_x=854.0
```

### Three 1/3 Width Columns (Active = Column 2)

**LTR:**
```
active_column=2
active_column_x=852.0
active_tile_viewport_x=852.0
```

**RTL:**
```
active_column=2
active_column_x=2.0          // Leftmost in RTL
active_tile_viewport_x=2.0
```

Calculation:
- Col 0: 1280 - 426 = 854
- Col 1: 854 - 426 = 428
- Col 2: 428 - 426 = 2

### Half Width Column

**LTR:**
```
active_column_x=0.0
active_tile_viewport_x=0.0
```

**RTL:**
```
active_column_x=640.0        // 1280 - 640 = 640 (centered)
active_tile_viewport_x=640.0
```

## Helper Functions Added

### `parse_active_column(snapshot: &str) -> Option<usize>`

Parses the `active_column=N` field from snapshot.

## Tests Added

Three comprehensive tests:
1. **`test_calculate_rtl_active_positions_single_column`** - Single column at 1/3 width
2. **`test_calculate_rtl_active_positions_three_columns`** - Three columns with active=2
3. **`test_calculate_rtl_active_positions_half_width`** - Half-width column (centered)

All tests ✅ **PASSING**

## Usage in Golden Tests

The golden test framework can now use this function to verify RTL active positions:

```rust
let (rtl_active_col_x, rtl_viewport_x, rtl_viewport_y) = 
    calculate_rtl_active_positions(ltr_snapshot).unwrap();

// Compare against actual RTL snapshot
assert_eq!(rtl_snapshot_active_col_x, rtl_active_col_x);
assert_eq!(rtl_snapshot_viewport_x, rtl_viewport_x);
assert_eq!(rtl_snapshot_viewport_y, rtl_viewport_y);
```

## Key Insights

### Y Position Unchanged
Vertical positioning is identical in LTR and RTL:
```rust
active_tile_viewport_y_rtl = active_tile_viewport_y_ltr
```

### View Position
Currently always 0 in RTL (scrolling not implemented):
```rust
view_pos_rtl = 0.0
```

### Column Indices
Column indices remain the same in RTL (logical ordering):
- Column 0 is still "first" even though it appears on the right
- Active column index doesn't change between LTR and RTL

## Integration

This function integrates with the existing RTL calculator infrastructure:
- Uses existing `parse_snapshot_metadata()` 
- Uses existing `parse_columns()` and `parse_tiles()`
- Follows same calculation pattern as `calculate_rtl_positions()`
- Compatible with struts, gaps, and different screen sizes

## Next Steps

When RTL scrolling is implemented, update the calculation:
```rust
// Future: when view_pos is non-zero in RTL
active_tile_viewport_x = active_column_x - view_pos_rtl
```

Currently:
```rust
// Current: view_pos is always 0 in RTL
active_tile_viewport_x = active_column_x
```
