# Snapshot Format Documentation

## Overview

The snapshot format captures the complete state of the scrolling layout for testing purposes. The enhanced snapshot format now includes **position information** and **visual markers** to make it immediately clear:

1. **Where the active tile is positioned** in content space
2. **Where the active tile appears on screen** (viewport position)
3. **Whether the active tile is visible** in the viewport

This addresses the critical testing requirement that **the active tile must always be visible**.

## Example Snapshot

```
view_width=1280
view_height=720
scale=1
working_area_x=0
working_area_y=0
working_area_width=1280
working_area_height=720
parent_area_x=0
parent_area_y=0
parent_area_width=1280
parent_area_height=720
gaps=0
view_offset=Static(-100.0)
view_pos=0.0                         ← Transformed viewport position for rendering
active_column=1                       ← Index of active column
active_column_x=100.0                 ← X position of active column in content space
active_tile_viewport_x=100.0          ← Where active tile appears on screen (X)
active_tile_viewport_y=0.0            ← Where active tile appears on screen (Y)
column[0]: x=0.0 width=Fixed(100.0) active_tile=0
  tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0   ← [ACTIVE] marker
  tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2      ← [ACTIVE] marker
```

## Key Fields Explained

### Position Fields (NEW)

- **`view_pos`**: The transformed viewport position used for rendering. This is the actual scroll position.
  - LTR: `view_pos = column_x(active_column) + view_offset`
  - RTL: `view_pos = view_offset`

- **`active_column_x`**: The X position of the active column in content space (before viewport transformation).

- **`active_tile_viewport_x`**: Where the active tile appears on screen, relative to viewport (0,0).
  - Calculated as: `active_column_x - view_pos`
  - **Critical for testing**: This value should typically be within `[0, view_width)` for the tile to be visible.

- **`active_tile_viewport_y`**: The Y position of the active tile within its column.
  - Sum of all tile heights above it (plus gaps).

### Column and Tile Positions (NEW)

Each column now includes:
- `x=<value>`: The X position in content space

Each tile now includes:
- `x=<value>`: The X position in content space (same as its column)
- `y=<value>`: The Y position within its column

### Active Markers (NEW)

- **`[ACTIVE]`** suffix on columns and tiles clearly marks which elements are active.
- Makes visual inspection of snapshots much easier.

## Verifying Active Tile Visibility

To verify an active tile is visible, check:

```rust
fn is_active_tile_visible(snapshot: &str) -> bool {
    let view_width = parse_field(snapshot, "view_width");
    let active_x = parse_field(snapshot, "active_tile_viewport_x");
    
    // Active tile should be within viewport bounds
    active_x >= 0.0 && active_x < view_width
}
```

## Benefits of Enhanced Snapshots

1. **Immediate visibility verification**: You can see at a glance if the active tile is on-screen.
2. **Easier debugging**: Position information makes it clear where things are going wrong.
3. **Better test coverage**: Tests can now verify positioning invariants.
4. **Clearer test failures**: When a test fails, the snapshot diff shows exactly what changed in terms of positions.

## Example: Catching Scrolling Bugs

**Before** (old format):
```
view_offset=Static(-200.0)
active_column=2
column[0]: width=Fixed(100.0) active_tile=0
  tile[0]: w=100 h=720 window_id=1
column[1]: width=Fixed(100.0) active_tile=0
  tile[0]: w=100 h=720 window_id=2
column[2]: width=Fixed(100.0) active_tile=0
  tile[0]: w=100 h=720 window_id=3
```
❌ **Problem**: Can't tell if column 2 is visible on screen!

**After** (new format):
```
view_offset=Static(-200.0)
view_pos=0.0
active_column=2
active_column_x=200.0
active_tile_viewport_x=200.0    ← Clearly visible at X=200
active_tile_viewport_y=0.0
column[0]: x=0.0 width=Fixed(100.0) active_tile=0
  tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
column[1]: x=100.0 width=Fixed(100.0) active_tile=0
  tile[0]: x=100.0 y=0.0 w=100 h=720 window_id=2
column[2] [ACTIVE]: x=200.0 width=Fixed(100.0) active_tile=0
  tile[0] [ACTIVE]: x=200.0 y=0.0 w=100 h=720 window_id=3
```
✅ **Success**: Active tile is at viewport X=200, clearly within [0, 1280)!

## RTL Considerations

In RTL mode, the position calculations are mirrored:
- Columns grow leftward from the right edge
- `view_pos` equals `view_offset` directly (no column offset added)
- Position information still shows where tiles appear in viewport space

The enhanced snapshot format works correctly for both LTR and RTL layouts.
