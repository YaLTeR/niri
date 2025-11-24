# Scrolling Layout Logging Added

## Summary

Added comprehensive logging to the scrolling layout code to help debug and verify golden tests. All logging is wrapped in `#[cfg(test)]` so it only appears during testing.

## Logging Added

### 1. Snapshot Generation (`space/core.rs`)

**Location:** `ScrollingSpace::snapshot()`

**Log Message:**
```
📸 SNAPSHOT: columns={}, active={}, view_offset={:?}, view_pos={:.1}, rtl={}
```

**Fields:**
- `columns` - Number of columns
- `active` - Active column index
- `view_offset` - Current view offset (Static/Animated)
- `view_pos` - Transformed view position for rendering
- `rtl` - Whether RTL mode is enabled

**When:** Every time a snapshot is generated (for golden tests)

### 2. Window/Tile Addition (`manipulation/add_remove.rs`)

**Location:** `ScrollingSpace::add_tile()`

**Log Message:**
```
➕ ADD_TILE: window_id={:?}, col_idx={:?}, activate={}, width={:?}, is_full_width={}, rtl={}
```

**Fields:**
- `window_id` - ID of the window being added
- `col_idx` - Column index where it's being added (None = new column)
- `activate` - Whether this window should be activated
- `width` - Column width (Proportion/Fixed)
- `is_full_width` - Whether column should be full width
- `rtl` - Whether RTL mode is enabled

**When:** Every time a window is added to the layout

### 3. Column Activation (`space/view_offset.rs`)

**Location:** `ScrollingSpace::activate_column_with_anim_config()`

**Log Messages:**
```
🎯 ACTIVATE_COLUMN: from={} to={}, columns={}, view_offset={:?}, rtl={}
   → new view_offset={:?}, view_pos={:.1}
```

**Fields:**
- `from` - Previous active column index
- `to` - New active column index
- `columns` - Total number of columns
- `view_offset` - View offset before/after activation
- `view_pos` - View position after activation
- `rtl` - Whether RTL mode is enabled

**When:** Every time the active column changes

## Usage in Tests

### Enable Logging

To see the logs during tests, set the `RUST_LOG` environment variable:

```bash
# See all debug logs
RUST_LOG=debug cargo test --lib golden_tests

# See only scrolling layout logs
RUST_LOG=niri::layout::scrolling=debug cargo test --lib golden_tests

# See only specific test
RUST_LOG=debug cargo test --lib golden_tests::spawning_multiple::spawn_one_third_three_tiles -- --nocapture
```

### Example Output

```
📸 SNAPSHOT: columns=0, active=0, view_offset=Static(0.0), view_pos=0.0, rtl=false
➕ ADD_TILE: window_id=1, col_idx=None, activate=true, width=Proportion(0.33333), is_full_width=false, rtl=false
🎯 ACTIVATE_COLUMN: from=0 to=0, columns=1, view_offset=Static(0.0), rtl=false
   → new view_offset=Static(0.0), view_pos=0.0
➕ ADD_TILE: window_id=2, col_idx=None, activate=true, width=Proportion(0.33333), is_full_width=false, rtl=false
🎯 ACTIVATE_COLUMN: from=0 to=1, columns=2, view_offset=Static(0.0), rtl=false
   → new view_offset=Static(0.0), view_pos=0.0
➕ ADD_TILE: window_id=3, col_idx=None, activate=true, width=Proportion(0.33333), is_full_width=false, rtl=false
🎯 ACTIVATE_COLUMN: from=1 to=2, columns=3, view_offset=Static(0.0), rtl=false
   → new view_offset=Static(0.0), view_pos=0.0
📸 SNAPSHOT: columns=3, active=2, view_offset=Static(0.0), view_pos=0.0, rtl=false
```

## Benefits

### 1. **Golden Test Debugging**
- See exactly what operations are performed
- Track column activation and view offset changes
- Verify RTL vs LTR behavior

### 2. **Position Verification**
- See view_offset and view_pos at each step
- Track active column changes
- Verify scrolling behavior

### 3. **Test Failure Diagnosis**
- Quickly identify which operation caused unexpected state
- See the sequence of operations leading to a snapshot
- Compare LTR vs RTL operation sequences

### 4. **Future Test Development**
- Understand the expected operation sequence
- Verify new tests follow correct patterns
- Debug complex multi-column scenarios

## Log Symbols

- 📸 **SNAPSHOT** - Snapshot generation
- ➕ **ADD_TILE** - Window/tile addition
- 🎯 **ACTIVATE_COLUMN** - Column activation

## Files Modified

1. `/home/vince/Projects/niri/src/layout/scrolling/space/core.rs`
   - Added snapshot logging

2. `/home/vince/Projects/niri/src/layout/scrolling/manipulation/add_remove.rs`
   - Added tile addition logging

3. `/home/vince/Projects/niri/src/layout/scrolling/space/view_offset.rs`
   - Added column activation logging

## Performance Impact

**Zero impact in production** - All logging is wrapped in `#[cfg(test)]` and only compiled/executed during tests.

## Future Enhancements

Additional logging could be added for:
- Column removal
- Window resizing
- View offset animations
- Interactive resize
- Column movement
- Tile consumption/expulsion
- Scrolling gestures

## Example Test Run

```bash
$ RUST_LOG=debug cargo test --lib golden_tests::spawning_multiple::spawn_one_third_three_tiles -- --nocapture

➕ ADD_TILE: window_id=1, col_idx=None, activate=true, width=Proportion(0.33333333333333337), is_full_width=false, rtl=false
🎯 ACTIVATE_COLUMN: from=0 to=0, columns=1, view_offset=Static(0.0), rtl=false
   → new view_offset=Static(0.0), view_pos=0.0
➕ ADD_TILE: window_id=2, col_idx=None, activate=true, width=Proportion(0.33333333333333337), is_full_width=false, rtl=false
🎯 ACTIVATE_COLUMN: from=0 to=1, columns=2, view_offset=Static(0.0), rtl=false
   → new view_offset=Static(0.0), view_pos=0.0
➕ ADD_TILE: window_id=3, col_idx=None, activate=true, width=Proportion(0.33333333333333337), is_full_width=false, rtl=false
🎯 ACTIVATE_COLUMN: from=1 to=2, columns=3, view_offset=Static(0.0), rtl=false
   → new view_offset=Static(0.0), view_pos=0.0
📸 SNAPSHOT: columns=3, active=2, view_offset=Static(0.0), view_pos=0.0, rtl=false

test layout::tests::golden_tests::spawning_multiple::spawn_one_third_three_tiles ... ok
```

This shows the complete sequence of operations and helps verify the test is doing what you expect!
