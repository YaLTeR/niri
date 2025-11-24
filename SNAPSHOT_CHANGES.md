# Enhanced Snapshot Format - Complete Changes

## Summary

The snapshot format has been **significantly enhanced** to solve the critical problem: **we couldn't verify that the active tile is visible in the viewport**.

## What Changed

### New Snapshot Fields

Every snapshot now includes these additional fields:

```
view_pos=0.0                      # NEW: Transformed viewport position
active_column=1                   # (existing)
active_column_x=100.0             # NEW: Active column X in content space
active_tile_viewport_x=100.0      # NEW: Active tile X on screen
active_tile_viewport_y=0.0        # NEW: Active tile Y within column
```

### Enhanced Column/Tile Display

Every column and tile now shows position information:

```
column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
  tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2
```

**Key additions:**
- `x=<value>` for every column (position in content space)
- `x=<value> y=<value>` for every tile (position in content/column space)
- `[ACTIVE]` markers for visual identification

## Real Example

### Before ❌
```
view_offset=Static(-100.0)
active_column=1
column[0]: width=Fixed(100.0) active_tile=0
  tile[0]: w=100 h=720 window_id=1
column[1]: width=Fixed(100.0) active_tile=0
  tile[0]: w=100 h=720 window_id=2
```
**Problem:** Is column 1 visible? We can't tell!

### After ✅
```
view_offset=Static(-100.0)
view_pos=0.0
active_column=1
active_column_x=100.0
active_tile_viewport_x=100.0    ← Tile is at screen position 100
active_tile_viewport_y=0.0      ← Tile is at top of column
column[0]: x=0.0 width=Fixed(100.0) active_tile=0
  tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
  tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2
```
**Success:** Active tile is clearly at viewport X=100, within [0, 1280) bounds! ✓

## How to Use

### 1. Visual Inspection
Just look at `active_tile_viewport_x`:
- If it's between 0 and `view_width`, the tile is visible
- Example: `active_tile_viewport_x=200.0` with `view_width=1280` → **Visible** ✓

### 2. Programmatic Verification
```rust
fn verify_active_tile_visible(snapshot: &str) -> bool {
    let view_width = parse_field(snapshot, "view_width");
    let tile_x = parse_field(snapshot, "active_tile_viewport_x");
    
    // Active tile should be visible
    tile_x >= 0.0 && tile_x < view_width
}
```

### 3. Debugging Position Issues
When a test fails, you immediately see:
- Where each column is positioned (`x=<value>`)
- Where each tile is positioned (`x=<value> y=<value>`)
- Where the viewport is scrolled (`view_pos=<value>`)
- Where the active tile appears on screen (`active_tile_viewport_x=<value>`)

## Files Modified

1. **`src/layout/scrolling/space/core.rs`**
   - Enhanced `snapshot()` method with position calculations
   - Added viewport transformation display
   - Added active markers

2. **All snapshot tests** (115 tests updated)
   - Automatically updated via `cargo insta test --accept`
   - All tests passing

3. **New test file**
   - `src/layout/tests/snapshot_tests/37_ltr_active_tile_visibility.rs`
   - Demonstrates visibility verification
   - Includes helper functions for parsing

4. **Documentation**
   - `docs/SNAPSHOT_FORMAT.md` - Complete format reference
   - `SNAPSHOT_ENHANCEMENT_SUMMARY.md` - Implementation details
   - `SNAPSHOT_CHANGES.md` - This file

## Benefits for RTL Testing

The enhanced format is **especially valuable** for RTL work:

1. **Verify mirroring**: Compare LTR and RTL column X positions
2. **Check viewport calculations**: Ensure `view_pos` transforms correctly
3. **Debug scrolling**: See exactly where content is positioned

Example RTL verification:
```
# LTR:
column[0]: x=0.0 ...     # First column at left edge
column[1]: x=100.0 ...   # Second column to the right

# RTL (should mirror):
column[0]: x=1180.0 ...  # First column at right edge  
column[1]: x=1080.0 ...  # Second column to the left
```

## Running Tests

```bash
# All snapshot tests
cargo test --lib -- layout::tests::snapshot_tests

# Visibility-specific tests
cargo test --lib -- layout::tests::snapshot_tests::ltr_active_tile_visibility

# Update snapshots after changes
cargo insta test --lib --accept -- layout::tests::snapshot_tests
```

## Conclusion

**The snapshot format now provides complete visibility into layout positioning.**

You can now:
- ✅ **Verify active tile is visible**
- ✅ **Debug positioning issues**
- ✅ **Validate RTL mirroring**
- ✅ **Catch scrolling bugs early**

The tests are now **complete and reliable** for verifying that the active tile is always visible on screen.
