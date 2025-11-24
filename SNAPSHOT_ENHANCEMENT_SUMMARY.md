# Snapshot Enhancement Summary

## Problem

The original snapshot format did not capture critical position information:

1. **Active tile position**: Where is the active tile in content space?
2. **Viewport position**: Where does the active tile appear on screen?
3. **Visibility**: Is the active tile actually visible?

This made it **impossible to verify** that the active tile is within the viewport, which is a fundamental requirement for a working scrolling layout.

## Solution

Enhanced the snapshot format in `src/layout/scrolling/space/core.rs` to include:

### New Fields

1. **`view_pos`**: The transformed viewport position for rendering
   ```
   view_pos=0.0
   ```

2. **`active_column_x`**: X position of the active column in content space
   ```
   active_column_x=100.0
   ```

3. **`active_tile_viewport_x`**: Where the active tile appears on screen (X coordinate)
   ```
   active_tile_viewport_x=100.0
   ```

4. **`active_tile_viewport_y`**: Where the active tile appears within its column (Y coordinate)
   ```
   active_tile_viewport_y=0.0
   ```

### Enhanced Display

1. **Position information**: Each column and tile now shows its `x=` and `y=` coordinates
   ```
   column[1]: x=100.0 width=Fixed(100.0) active_tile=0
     tile[0]: x=100.0 y=0.0 w=100 h=720 window_id=2
   ```

2. **Visual markers**: Active columns and tiles are clearly marked
   ```
   column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
     tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2
   ```

## Changes Made

### 1. Enhanced `snapshot()` method
**File**: `src/layout/scrolling/space/core.rs`

- Added `view_pos` calculation
- Added `active_column_x` tracking
- Added `active_tile_viewport_x` and `active_tile_viewport_y` calculation
- Added `[ACTIVE]` markers to columns and tiles
- Added x, y position information for all columns and tiles

### 2. Updated all snapshot tests
All 107 existing snapshot tests were automatically updated using:
```bash
cargo insta test --lib --accept -- layout::tests::snapshot_tests
```

### 3. Added new visibility tests
**File**: `src/layout/tests/snapshot_tests/37_ltr_active_tile_visibility.rs`

Three new tests that demonstrate:
- Active tile visibility verification
- Position tracking with multiple tiles
- Helper function to parse and verify tile visibility

### 4. Documentation
**File**: `docs/SNAPSHOT_FORMAT.md`

Complete documentation of the new snapshot format with:
- Field explanations
- Examples showing before/after
- Code examples for verifying visibility
- RTL considerations

## Benefits

### 1. Immediate Visibility Verification
You can now see at a glance whether the active tile is visible:
```
active_tile_viewport_x=200.0  ← In view [0, 1280)? YES ✓
```

### 2. Easier Debugging
When tests fail, you immediately see:
- Where columns are positioned
- Where tiles are positioned
- Where the viewport is scrolled to
- Whether the active tile is on-screen

### 3. Complete Test Coverage
Tests can now verify positioning invariants:
```rust
assert!(active_tile_viewport_x >= 0.0);
assert!(active_tile_viewport_x < view_width);
```

### 4. RTL Compatibility
The enhanced format works correctly for both LTR and RTL layouts, making it easier to verify RTL mirroring is correct.

## Example Before/After

### Before ❌
```
view_offset=Static(-100.0)
active_column=1
column[0]: width=Fixed(100.0) active_tile=0
  tile[0]: w=100 h=720 window_id=1
column[1]: width=Fixed(100.0) active_tile=0
  tile[0]: w=100 h=720 window_id=2
```
**Problem**: Can't tell where column 1 appears on screen!

### After ✅
```
view_offset=Static(-100.0)
view_pos=0.0
active_column=1
active_column_x=100.0
active_tile_viewport_x=100.0    ← Clearly at X=100 on screen
active_tile_viewport_y=0.0
column[0]: x=0.0 width=Fixed(100.0) active_tile=0
  tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
  tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2
```
**Success**: Active tile position is explicit and verifiable!

## Testing

All tests pass:
```bash
# Run all snapshot tests
cargo test --lib -- layout::tests::snapshot_tests

# Run visibility-specific tests
cargo test --lib -- layout::tests::snapshot_tests::ltr_active_tile_visibility
```

## Next Steps

1. **Use the new format**: All future snapshot tests automatically benefit from the enhanced format.

2. **Add visibility assertions**: When writing new tests, you can now add explicit checks:
   ```rust
   let snapshot = layout.snapshot();
   assert!(parse_active_tile_in_viewport(&snapshot), 
       "Active tile must be visible in viewport");
   ```

3. **Debug positioning issues**: When investigating RTL bugs or scrolling issues, the snapshot now shows exactly where everything is positioned.

## Conclusion

The enhanced snapshot format provides **complete visibility** into the layout's positioning state, making it possible to verify that the active tile is always visible and catch positioning bugs early. This is especially critical for the RTL refactor where viewport calculations are complex.
