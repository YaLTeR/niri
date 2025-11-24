# Snapshot Format Quick Reference

## At a Glance: Is the Active Tile Visible?

Look at these two lines:
```
view_width=1280
active_tile_viewport_x=200.0     ← Is this between 0 and 1280? YES ✓
```

## Key Fields

| Field | Meaning | Example |
|-------|---------|---------|
| `view_width` | Screen width | `1280` |
| `view_pos` | Viewport scroll position | `0.0` |
| `active_column` | Index of active column | `1` |
| `active_column_x` | Column position in content space | `100.0` |
| `active_tile_viewport_x` | **Where tile appears on screen** | `100.0` |
| `active_tile_viewport_y` | Tile Y position within column | `0.0` |

## Visual Markers

```
column[1] [ACTIVE]: ...     ← This column is active
  tile[0] [ACTIVE]: ...     ← This tile is active
```

## Position Fields

```
column[0]: x=0.0 width=Fixed(100.0) ...
  tile[0]: x=0.0 y=0.0 w=100 h=720 ...
           ↑     ↑
           │     └─ Y position within column
           └─ X position in content space
```

## Quick Visibility Check

```rust
// Visible if:
0 ≤ active_tile_viewport_x < view_width

// Example:
view_width=1280
active_tile_viewport_x=200.0
// 0 ≤ 200 < 1280 → VISIBLE ✓
```

## Reading a Complete Snapshot

```
view_width=1280              # Screen dimensions
view_height=720
view_pos=0.0                 # Scroll position
active_column=1              # Column index
active_column_x=100.0        # Column content position
active_tile_viewport_x=100.0 # Tile screen position ← CHECK THIS
active_tile_viewport_y=0.0   # Tile Y in column

column[0]: x=0.0 width=Fixed(100.0) active_tile=0
  tile[0]: x=0.0 y=0.0 w=100 h=720 window_id=1
           
column[1] [ACTIVE]: x=100.0 width=Fixed(100.0) active_tile=0
  tile[0] [ACTIVE]: x=100.0 y=0.0 w=100 h=720 window_id=2
```

**Interpretation:**
- Active tile is at screen position X=100
- This is within [0, 1280), so it's visible ✓
- Tile occupies pixels 100-200 horizontally
- Tile is at the top of its column (Y=0)

## Common Patterns

### Single Row of Columns
```
active_tile_viewport_x=200.0
column[0]: x=0.0 ...
column[1] [ACTIVE]: x=100.0 ...   ← Active
column[2]: x=200.0 ...
```

### Multiple Tiles in Column
```
active_tile_viewport_y=240.0      ← Active tile is 240px down
column[0] [ACTIVE]: ...
  tile[0]: x=0.0 y=0.0 h=240 ...
  tile[1] [ACTIVE]: x=0.0 y=240.0 h=240 ...  ← Active
  tile[2]: x=0.0 y=480.0 h=240 ...
```

### With Scrolling
```
view_pos=100.0                    # Scrolled 100px right
active_column_x=200.0             # Column at 200 in content
active_tile_viewport_x=100.0      # Appears at 100 on screen
                                  # (200 - 100 = 100)
```

## Troubleshooting

### Active tile not visible?
```
active_tile_viewport_x=-50.0      # Negative = off-screen left
active_tile_viewport_x=1500.0     # > view_width = off-screen right
```

### Can't find active tile?
Look for `[ACTIVE]` markers:
```
column[2] [ACTIVE]: ...           ← Here's the active column
  tile[1] [ACTIVE]: ...           ← Here's the active tile
```

### Position doesn't make sense?
Check `view_pos` vs `active_column_x`:
```
view_pos=0.0
active_column_x=100.0
active_tile_viewport_x=100.0      # Correct: 100 - 0 = 100
```

## Documentation

- Full format: `docs/SNAPSHOT_FORMAT.md`
- Implementation: `SNAPSHOT_ENHANCEMENT_SUMMARY.md`
- Changes: `SNAPSHOT_CHANGES.md`
