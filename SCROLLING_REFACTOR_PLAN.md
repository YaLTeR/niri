# Scrolling Layout Refactoring Plan

## Overview
The `src/layout/scrolling.rs` file is 5619 lines long and needs to be refactored into smaller, more manageable modules.

## Current Status
✅ Created `src/layout/scrolling/` directory
✅ Created `types.rs` - Core type definitions
✅ Created `utils.rs` - Utility functions
✅ Created `mod.rs` - Module shim (placeholder)

## Proposed Module Structure (Files Under 400 Lines)

### 1. `types.rs` (✅ COMPLETED - 280 lines)
**Purpose**: Core type definitions and enums
**Contents**:
- `ColumnData` - Extra per-column data
- `TileData` - Extra per-tile data  
- `ColumnWidth` - Width of a column (Proportion/Fixed)
- `WindowHeight` - Height of a window (Auto/Fixed/Preset)
- `ScrollDirection` - Left/Right direction enum
- `ViewOffset` - View offset state machine (Static/Animation/Gesture)
- `ViewGesture` - Gesture tracking data
- `MoveAnimation` - Animation data for column movement
- `VIEW_GESTURE_WORKING_AREA_MOVEMENT` constant

### 2. `utils.rs` (✅ COMPLETED - 160 lines)
**Purpose**: Standalone utility functions
**Contents**:
- `compute_new_view_offset()` - Calculate view offset for column visibility
- `compute_working_area()` - Calculate working area with struts
- `compute_toplevel_bounds()` - Calculate toplevel bounds for windows
- `resolve_preset_size()` - Resolve preset sizes to actual dimensions
- Tests for working area calculations

### 3. `column/` (📝 TODO - Subfolder with 5 files, ~1800 lines total)
**Purpose**: Column struct split into manageable files

#### 3.1. `column/mod.rs` (~100 lines)
- Module declarations and re-exports
- `Column<W>` struct definition
- Public API surface

#### 3.2. `column/core.rs` (~350 lines)
- Construction: `new_with_tile()`
- Configuration: `update_config()`
- Animation: `advance_animations()`, `are_animations_ongoing()`, etc.
- State queries: `sizing_mode()`, `contains()`, `position()`
- Rendering: `update_render_elements()`, `render_offset()`
- Tile management: `add_tile_at()`, `update_window()`

#### 3.3. `column/sizing.rs` (~400 lines)
- Size calculation: `update_tile_sizes()`, `update_tile_sizes_with_transaction()`
- Width resolution: `resolve_column_width()`, `resolve_preset_width()`
- Height resolution: `resolve_preset_height()`
- Extra size: `extra_size()`
- Width computation logic (complex height distribution algorithm)

#### 3.4. `column/operations.rs` (~400 lines)
- Width operations: `toggle_width()`, `set_column_width()`, `toggle_full_width()`
- Height operations: `set_window_height()`, `reset_window_height()`, `toggle_window_height()`
- Height conversion: `convert_heights_to_auto()`
- Focus management: `focus_up()`, `focus_down()`, `move_up()`, `move_down()`, `focus_index()`, `focus_top()`, `focus_bottom()`
- Display mode: `set_column_display()`, `set_fullscreen()`, `set_maximized()`

#### 3.5. `column/positioning.rs` (~350 lines)
- Tile positioning: `tiles_origin()`, `tile_offsets()`, `tile_offset()`
- Iterators: `tiles()`, `tiles_mut()`, `tiles_in_render_order()`, `tiles_in_render_order_mut()`
- Tab indicator: `tab_indicator_area()`
- Animation: `start_open_animation()`
- Verification: `verify_invariants()` (test only)

### 4. `space/` (📝 TODO - Subfolder with 4 files, ~1200 lines total)
**Purpose**: ScrollingSpace struct split into manageable files

#### 4.1. `space/mod.rs` (~100 lines)
- Module declarations and re-exports
- `ScrollingSpace<W>` struct definition
- Render element enum
- Public API surface

#### 4.2. `space/core.rs` (~350 lines)
- Construction: `new()`
- Configuration: `update_config()`, `update_shaders()`
- Animation: `advance_animations()`, `are_animations_ongoing()`, etc.
- Queries: `tiles()`, `tiles_mut()`, `is_empty()`, `active_window()`, etc.
- Column access: `columns()`, `columns_mut()`, `column_x()`, `column_xs()`
- View positioning: `view_pos()`, `target_view_pos()`
- Centering: `is_centering_focused_column()`

#### 4.3. `space/view_offset.rs` (~400 lines)
- View offset computation: `compute_new_view_offset_*()` methods
- View offset animation: `animate_view_offset*()` methods
- Column activation: `activate_column*()` methods
- Centering: `center_column()`, `center_window()`, `center_visible_columns()`

#### 4.4. `space/queries.rs` (~350 lines)
- Window lookup: `window_under()`, `popup_target_rect()`
- Bounds: `new_window_toplevel_bounds()`, `new_window_size()`
- Insert position: `insert_position()`, `insert_hint_area()`
- Visual rectangle: `active_tile_visual_rectangle()`
- Snapshot: `snapshot()`
- Verification: `verify_invariants()` (test only)

### 5. `gestures.rs` (📝 TODO - ~400 lines)
**Purpose**: Gesture handling for view offset
**Contents**:
- Gesture handling: `view_offset_gesture_begin()`, `view_offset_gesture_update()`, `view_offset_gesture_end()`
- DnD scroll: `dnd_scroll_gesture_begin()`, `dnd_scroll_gesture_scroll()`, `dnd_scroll_gesture_end()`
- Snapping logic for gesture end
- Helper functions for gesture calculations

### 6. `manipulation/` (📝 TODO - Subfolder with 3 files, ~1000 lines total)
**Purpose**: Window and tile manipulation operations

#### 6.1. `manipulation/mod.rs` (~50 lines)
- Module declarations and re-exports

#### 6.2. `manipulation/add_remove.rs` (~400 lines)
- Adding: `add_tile()`, `add_tile_to_column()`, `add_tile_right_of()`, `add_column()`
- Removing: `remove_active_tile()`, `remove_tile()`, `remove_tile_by_idx()`, `remove_active_column()`, `remove_column_by_idx()`
- Window updates: `update_window()`, `scroll_amount_to_activate()`, `activate_window()`

#### 6.3. `manipulation/movement.rs` (~350 lines)
- Moving: `move_left()`, `move_right()`, `move_column_to()`, `move_column_to_index()`, `move_column_to_first()`, `move_column_to_last()`, `move_down()`, `move_up()`
- Focus: `focus_left()`, `focus_right()`, `focus_column()`, `focus_column_first()`, `focus_column_last()`, `focus_window_in_column()`, `focus_down()`, `focus_up()`, `focus_down_or_left()`, `focus_down_or_right()`, `focus_up_or_left()`, `focus_up_or_right()`, `focus_top()`, `focus_bottom()`

#### 6.4. `manipulation/consume_swap.rs` (~350 lines)
- Consume/expel: `consume_or_expel_window_left()`, `consume_or_expel_window_right()`, `consume_into_column()`, `expel_from_column()`
- Swap: `swap_window_in_direction()`

### 7. `resize.rs` (📝 TODO - ~400 lines)
**Purpose**: Interactive resize and sizing operations
**Contents**:
- Width operations: `toggle_width()`, `toggle_full_width()`, `set_window_width()`, `toggle_window_width()`
- Height operations: `set_window_height()`, `reset_window_height()`, `toggle_window_height()`
- Column expansion: `expand_column_to_available_width()`
- Display mode: `toggle_column_tabbed_display()`, `set_column_display()`
- Interactive resize: `interactive_resize_begin()`, `interactive_resize_update()`, `interactive_resize_end()`
- Fullscreen/maximize: `set_fullscreen()`, `set_maximized()` (on ScrollingSpace)
- Helper: `cancel_resize_for_column()`

### 8. `render.rs` (📝 TODO - ~400 lines)
**Purpose**: Rendering-related methods
**Contents**:
- Element generation: `render_elements()`
- Position calculation: `tiles_with_render_positions()`, `tiles_with_render_positions_mut()`
- Layout info: `tiles_with_ipc_layouts()`
- Refresh: `refresh()` - Configure and refresh all windows
- Animation: `start_close_animation_for_window()`, `start_close_animation_for_tile()`, `start_open_animation()`

### 9. `mod.rs` (✅ PARTIAL - needs completion)
**Purpose**: Module organization and re-exports
**Contents**:
- Module declarations
- Public re-exports to maintain API compatibility
- Documentation

## Implementation Strategy

### Phase 1: Foundation (✅ COMPLETED)
1. Create directory structure
2. Extract types and enums → `types.rs`
3. Extract utility functions → `utils.rs`
4. Create initial `mod.rs`

### Phase 2: Column Subfolder (📝 TODO)
1. Create `column/` directory
2. Extract Column struct definition → `column/mod.rs`
3. Extract core Column methods → `column/core.rs`
4. Extract sizing logic → `column/sizing.rs`
5. Extract operations → `column/operations.rs`
6. Extract positioning → `column/positioning.rs`

### Phase 3: Space Subfolder (📝 TODO)
1. Create `space/` directory
2. Extract ScrollingSpace struct → `space/mod.rs`
3. Extract core space methods → `space/core.rs`
4. Extract view offset methods → `space/view_offset.rs`
5. Extract query methods → `space/queries.rs`

### Phase 4: Manipulation Subfolder (📝 TODO)
1. Create `manipulation/` directory
2. Extract add/remove operations → `manipulation/add_remove.rs`
3. Extract movement/focus → `manipulation/movement.rs`
4. Extract consume/swap → `manipulation/consume_swap.rs`
5. Create module shim → `manipulation/mod.rs`

### Phase 5: Remaining Modules (📝 TODO)
1. Extract gesture handling → `gestures.rs`
2. Extract resize operations → `resize.rs`
3. Extract rendering → `render.rs`

### Phase 6: Integration (📝 TODO)
1. Update main `mod.rs` with all re-exports
2. Update imports in `src/layout/mod.rs`
3. Verify all tests pass
4. Update documentation
5. Remove original `scrolling.rs`

## Benefits

1. **Readability**: Each file focuses on a specific aspect
2. **Maintainability**: Easier to find and modify specific functionality
3. **Testing**: Can test modules independently
4. **Collaboration**: Multiple developers can work on different modules
5. **Documentation**: Each module can have focused documentation

## Notes

- All public APIs must remain unchanged for backward compatibility
- Use `pub(super)` for internal module visibility
- Keep related functionality together (e.g., all Column methods in column.rs)
- Maintain existing test structure
- The original scrolling.rs will be replaced by the scrolling/ directory

## Next Steps

1. Create `column/` subfolder with 5 files (mod.rs, core.rs, sizing.rs, operations.rs, positioning.rs)
2. Create `space/` subfolder with 4 files (mod.rs, core.rs, view_offset.rs, queries.rs)
3. Create `manipulation/` subfolder with 4 files (mod.rs, add_remove.rs, movement.rs, consume_swap.rs)
4. Create `gestures.rs` for gesture handling
5. Create `resize.rs` for resize operations
6. Create `render.rs` for rendering methods
7. Update main `mod.rs` with all re-exports
8. Remove original `scrolling.rs` file
9. Run tests and fix any issues
10. Update documentation

## File Size Target

**All files should be under 400 lines** to avoid token limits and improve readability.
