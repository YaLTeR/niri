# Scrolling Layout Refactoring - Implementation Guide

## Overview
This guide provides step-by-step instructions for completing the refactoring of `scrolling.rs` into a modular structure with files under 400 lines.

## Current Progress

### ✅ Completed
1. Created directory structure:
   - `/src/layout/scrolling/`
   - `/src/layout/scrolling/column/`
   - `/src/layout/scrolling/space/`
   - `/src/layout/scrolling/manipulation/`

2. Implemented foundation modules:
   - `types.rs` (280 lines) - Core type definitions
   - `utils.rs` (160 lines) - Utility functions
   - `mod.rs` (partial) - Module shim

3. Started column module:
   - `column/mod.rs` (100 lines) - Struct definition

## Implementation Steps

### Step 1: Complete Column Module

#### 1.1 Extract column/core.rs (~350 lines)
**Source**: Lines 3951-4396 in original scrolling.rs

**Contents**:
```rust
// Construction and configuration
impl<W: LayoutElement> Column<W> {
    pub(super) fn new_with_tile(...) { }
    pub(super) fn update_config(...) { }
    
    // Animation
    pub fn advance_animations(&mut self) { }
    pub fn are_animations_ongoing(&self) -> bool { }
    pub fn are_transitions_ongoing(&self) -> bool { }
    
    // Rendering
    pub fn update_render_elements(...) { }
    pub fn render_offset(&self) -> Point<f64, Logical> { }
    pub fn animate_move_from(&mut self, from_x_offset: f64) { }
    pub fn animate_move_from_with_config(...) { }
    pub fn offset_move_anim_current(&mut self, offset: f64) { }
    
    // State queries
    pub(super) fn sizing_mode(&self) -> SizingMode { }
    pub fn contains(&self, window: &W::Id) -> bool { }
    pub fn position(&self, window: &W::Id) -> Option<usize> { }
    pub fn is_pending_fullscreen(&self) -> bool { }
    pub fn is_pending_maximized(&self) -> bool { }
    pub fn pending_sizing_mode(&self) -> SizingMode { }
    
    // Tile management
    pub(super) fn activate_idx(&mut self, idx: usize) -> bool { }
    pub(super) fn activate_window(&mut self, window: &W::Id) { }
    pub(super) fn add_tile_at(&mut self, idx: usize, tile: Tile<W>) { }
    pub(super) fn update_window(&mut self, window: &W::Id) { }
}
```

#### 1.2 Extract column/sizing.rs (~400 lines)
**Source**: Lines 4397-4726 in original scrolling.rs

**Contents**:
```rust
impl<W: LayoutElement> Column<W> {
    pub(super) fn extra_size(&self) -> Size<f64, Logical> { }
    fn resolve_preset_width(&self, preset: PresetSize) -> ResolvedSize { }
    fn resolve_preset_height(&self, preset: PresetSize) -> ResolvedSize { }
    fn resolve_column_width(&self, width: ColumnWidth) -> f64 { }
    
    fn update_tile_sizes(&mut self, animate: bool) { }
    pub(super) fn update_tile_sizes_with_transaction(...) { }
    
    pub(super) fn width(&self) -> f64 { }
}
```

**Note**: The `update_tile_sizes_with_transaction` method contains complex height distribution logic (~300 lines). This is the largest single method and should remain intact.

#### 1.3 Extract column/operations.rs (~400 lines)
**Source**: Lines 4745-5183 in original scrolling.rs

**Contents**:
```rust
impl<W: LayoutElement> Column<W> {
    // Focus management
    fn focus_index(&mut self, index: u8) { }
    fn focus_up(&mut self) -> bool { }
    fn focus_down(&mut self) -> bool { }
    fn focus_top(&mut self) { }
    fn focus_bottom(&mut self) { }
    fn move_up(&mut self) -> bool { }
    fn move_down(&mut self) -> bool { }
    
    // Width operations
    fn toggle_width(&mut self, tile_idx: Option<usize>, forwards: bool) { }
    fn toggle_full_width(&mut self) { }
    fn set_column_width(&mut self, change: SizeChange, ...) { }
    
    // Height operations
    fn set_window_height(&mut self, change: SizeChange, ...) { }
    fn reset_window_height(&mut self, tile_idx: Option<usize>) { }
    fn toggle_window_height(&mut self, tile_idx: Option<usize>, forwards: bool) { }
    fn convert_heights_to_auto(&mut self) { }
    
    // Display mode
    fn set_fullscreen(&mut self, is_fullscreen: bool) { }
    fn set_maximized(&mut self, maximize: bool) { }
    fn set_column_display(&mut self, display: ColumnDisplay) { }
}
```

#### 1.4 Extract column/positioning.rs (~350 lines)
**Source**: Lines 5185-5470 in original scrolling.rs

**Contents**:
```rust
impl<W: LayoutElement> Column<W> {
    fn tiles_origin(&self) -> Point<f64, Logical> { }
    
    fn tile_offsets_iter(...) -> impl Iterator<Item = Point<f64, Logical>> { }
    fn tile_offsets(&self) -> impl Iterator<Item = Point<f64, Logical>> + '_ { }
    fn tile_offset(&self, tile_idx: usize) -> Point<f64, Logical> { }
    fn tile_offsets_in_render_order(...) -> impl Iterator<...> { }
    
    pub fn tiles(&self) -> impl Iterator<Item = (&Tile<W>, Point<f64, Logical>)> + '_ { }
    fn tiles_mut(&mut self) -> impl Iterator<...> + '_ { }
    fn tiles_in_render_order(&self) -> impl Iterator<...> + '_ { }
    fn tiles_in_render_order_mut(&mut self) -> impl Iterator<...> + '_ { }
    
    fn tab_indicator_area(&self) -> Rectangle<f64, Logical> { }
    pub fn start_open_animation(&mut self, id: &W::Id) -> bool { }
    
    #[cfg(test)]
    fn verify_invariants(&self) { }
}
```

### Step 2: Complete Space Module

#### 2.1 Extract space/mod.rs (~100 lines)
**Source**: Lines 33-102 in original scrolling.rs

**Contents**:
```rust
pub struct ScrollingSpace<W: LayoutElement> {
    columns: Vec<Column<W>>,
    data: Vec<ColumnData>,
    active_column_idx: usize,
    interactive_resize: Option<InteractiveResize<W>>,
    view_offset: ViewOffset,
    activate_prev_column_on_removal: Option<f64>,
    view_offset_to_restore: Option<f64>,
    closing_windows: Vec<ClosingWindow>,
    view_size: Size<f64, Logical>,
    working_area: Rectangle<f64, Logical>,
    parent_area: Rectangle<f64, Logical>,
    scale: f64,
    clock: Clock,
    options: Rc<Options>,
}

niri_render_elements! {
    ScrollingSpaceRenderElement<R> => {
        Tile = TileRenderElement<R>,
        ClosingWindow = ClosingWindowRenderElement,
        TabIndicator = TabIndicatorRenderElement,
    }
}
```

#### 2.2 Extract space/core.rs (~350 lines)
**Source**: Lines 284-463, 2305-2406 in original scrolling.rs

**Contents**:
```rust
impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn new(...) -> Self { }
    pub fn snapshot(&self) -> String { }
    pub fn update_config(...) { }
    pub fn update_shaders(&mut self) { }
    
    pub fn advance_animations(&mut self) { }
    pub fn are_animations_ongoing(&self) -> bool { }
    pub fn are_transitions_ongoing(&self) -> bool { }
    pub fn update_render_elements(&mut self, is_active: bool) { }
    
    pub fn tiles(&self) -> impl Iterator<Item = &Tile<W>> + '_ { }
    pub fn tiles_mut(&mut self) -> impl Iterator<Item = &mut Tile<W>> + '_ { }
    pub fn is_empty(&self) -> bool { }
    pub fn active_window(&self) -> Option<&W> { }
    pub fn active_window_mut(&mut self) -> Option<&mut W> { }
    pub fn active_tile_mut(&mut self) -> Option<&mut Tile<W>> { }
    pub fn is_active_pending_fullscreen(&self) -> bool { }
    
    pub fn view_pos(&self) -> f64 { }
    pub fn target_view_pos(&self) -> f64 { }
    
    fn column_xs<'a>(...) -> Box<dyn Iterator<Item = f64> + 'a> { }
    fn column_x(&self, column_idx: usize) -> f64 { }
    fn column_xs_in_render_order<'a>(...) -> Box<dyn Iterator<...> + 'a> { }
    
    pub fn columns(&self) -> impl Iterator<Item = &Column<W>> { }
    fn columns_mut(&mut self) -> impl Iterator<...> + '_ { }
    fn columns_in_render_order(&self) -> impl Iterator<...> + '_ { }
    fn columns_in_render_order_mut(&mut self) -> impl Iterator<...> + '_ { }
}
```

#### 2.3 Extract space/view_offset.rs (~400 lines)
**Source**: Lines 558-812 in original scrolling.rs

**Contents**:
```rust
impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn is_centering_focused_column(&self) -> bool { }
    
    fn compute_new_view_offset_fit(...) -> f64 { }
    fn compute_new_view_offset_centered(...) -> f64 { }
    fn compute_new_view_offset_for_column_fit(...) -> f64 { }
    fn compute_new_view_offset_for_column_centered(...) -> f64 { }
    fn compute_new_view_offset_for_column(...) -> f64 { }
    
    fn animate_view_offset(&mut self, idx: usize, new_view_offset: f64) { }
    fn animate_view_offset_with_config(...) { }
    fn animate_view_offset_to_column_centered(...) { }
    fn animate_view_offset_to_column_with_config(...) { }
    fn animate_view_offset_to_column(...) { }
    
    fn activate_column(&mut self, idx: usize) { }
    fn activate_column_with_anim_config(...) { }
    
    pub fn center_column(&mut self) { }
    pub fn center_window(&mut self, window: Option<&W::Id>) { }
    pub fn center_visible_columns(&mut self) { }
}
```

#### 2.4 Extract space/queries.rs (~350 lines)
**Source**: Lines 312-324, 473-551, 814-879, 2407-2613, 2999-3042, 3740-3815 in original scrolling.rs

**Contents**:
```rust
impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn new_window_toplevel_bounds(...) -> Size<i32, Logical> { }
    pub fn new_window_size(...) -> Size<i32, Logical> { }
    
    pub(super) fn insert_position(&self, pos: Point<f64, Logical>) -> InsertPosition { }
    pub(super) fn insert_hint_area(...) -> Option<Rectangle<f64, Logical>> { }
    
    pub fn active_tile_visual_rectangle(&self) -> Option<Rectangle<f64, Logical>> { }
    pub fn popup_target_rect(&self, id: &W::Id) -> Option<Rectangle<f64, Logical>> { }
    
    pub fn tiles_with_render_positions(&self) -> impl Iterator<...> { }
    pub fn tiles_with_render_positions_mut(&mut self, round: bool) -> impl Iterator<...> { }
    pub fn tiles_with_ipc_layouts(&self) -> impl Iterator<...> { }
    
    pub fn window_under(&self, pos: Point<f64, Logical>) -> Option<(&W, HitType)> { }
    
    #[cfg(test)]
    pub fn view_size(&self) -> Size<f64, Logical> { }
    #[cfg(test)]
    pub fn parent_area(&self) -> Rectangle<f64, Logical> { }
    #[cfg(test)]
    pub fn clock(&self) -> &Clock { }
    #[cfg(test)]
    pub fn options(&self) -> &Rc<Options> { }
    #[cfg(test)]
    pub fn active_column_idx(&self) -> usize { }
    #[cfg(test)]
    pub(super) fn view_offset(&self) -> &ViewOffset { }
    #[cfg(test)]
    pub fn verify_invariants(&self) { }
}
```

### Step 3: Complete Manipulation Module

#### 3.1 Create manipulation/mod.rs (~50 lines)
```rust
mod add_remove;
mod consume_swap;
mod movement;

pub use add_remove::*;
pub use consume_swap::*;
pub use movement::*;
```

#### 3.2 Extract manipulation/add_remove.rs (~400 lines)
**Source**: Lines 881-1258, 1421-1457 in original scrolling.rs

**Contents**: add_tile, add_tile_to_column, add_tile_right_of, add_column, remove_active_tile, remove_tile, remove_tile_by_idx, remove_active_column, remove_column_by_idx, update_window, scroll_amount_to_activate, activate_window

#### 3.3 Extract manipulation/movement.rs (~350 lines)
**Source**: Lines 1562-1681, 1684-1758 in original scrolling.rs

**Contents**: focus_left, focus_right, focus_column_first, focus_column_last, focus_column, focus_window_in_column, focus_down, focus_up, focus_down_or_left, focus_down_or_right, focus_up_or_left, focus_up_or_right, focus_top, focus_bottom, move_column_to_index, move_column_to, move_left, move_right, move_column_to_first, move_column_to_last, move_down, move_up

#### 3.4 Extract manipulation/consume_swap.rs (~350 lines)
**Source**: Lines 1776-2169 in original scrolling.rs

**Contents**: consume_or_expel_window_left, consume_or_expel_window_right, consume_into_column, expel_from_column, swap_window_in_direction

### Step 4: Create Remaining Modules

#### 4.1 Create gestures.rs (~400 lines)
**Source**: Lines 3044-3539 in original scrolling.rs

**Contents**: view_offset_gesture_begin, dnd_scroll_gesture_begin, view_offset_gesture_update, dnd_scroll_gesture_scroll, view_offset_gesture_end, dnd_scroll_gesture_end

#### 4.2 Create resize.rs (~400 lines)
**Source**: Lines 2171-2208, 2615-2755, 2757-2918, 3541-3654, 5554-5567 in original scrolling.rs

**Contents**: toggle_column_tabbed_display, set_column_display, toggle_width, toggle_full_width, set_window_width, set_window_height, reset_window_height, toggle_window_width, toggle_window_height, expand_column_to_available_width, set_fullscreen, set_maximized, interactive_resize_begin, interactive_resize_update, interactive_resize_end, cancel_resize_for_column

#### 4.3 Create render.rs (~400 lines)
**Source**: Lines 1459-1560, 2931-2997, 3656-3738 in original scrolling.rs

**Contents**: start_close_animation_for_window, start_close_animation_for_tile, start_open_animation, render_elements, refresh

### Step 5: Update Main mod.rs

Update `/src/layout/scrolling/mod.rs` to re-export everything:

```rust
mod column;
mod gestures;
mod manipulation;
mod render;
mod resize;
mod space;
mod types;
mod utils;

pub use column::Column;
pub use space::{ScrollingSpace, ScrollingSpaceRenderElement};
pub use types::{
    ColumnData, ColumnWidth, ScrollDirection, TileData, WindowHeight,
    VIEW_GESTURE_WORKING_AREA_MOVEMENT,
};
pub use utils::{compute_new_view_offset, compute_toplevel_bounds, compute_working_area, resolve_preset_size};

// Re-export internal types needed by other modules
pub(super) use types::{MoveAnimation, ViewGesture, ViewOffset};
```

## Code Extraction Tips

1. **Use grep to find method boundaries**:
   ```bash
   grep -n "pub fn\|pub(super) fn\|fn " scrolling.rs | less
   ```

2. **Extract in order**: Start with the simplest modules (positioning, queries) and work towards complex ones (sizing, gestures)

3. **Keep imports minimal**: Only import what each file needs

4. **Use `pub(super)`**: For methods that should only be visible within the scrolling module

5. **Test incrementally**: After extracting each module, try to compile and fix errors

6. **Preserve comments**: Keep all existing documentation and comments

## Common Patterns

### Module visibility:
- `pub` - Public API, visible outside scrolling module
- `pub(super)` - Visible within scrolling module only
- `pub(in crate::layout::scrolling)` - Visible within scrolling and submodules
- No modifier - Private to the file

### Import structure:
```rust
use std::...;  // Standard library
use external_crate::...;  // External crates
use crate::...;  // Internal crate imports
use super::...;  // Parent module imports
```

## Testing Strategy

After implementing each phase:

1. Run `cargo check` to verify syntax
2. Run `cargo test --lib layout::scrolling` to test the module
3. Run full test suite: `cargo test`
4. Fix any compilation errors before moving to next phase

## Final Steps

1. Delete original `/src/layout/scrolling.rs`
2. Update `/src/layout/mod.rs` if needed
3. Run full test suite
4. Update documentation
5. Commit changes

## Estimated Time

- Column module: 2-3 hours
- Space module: 2-3 hours
- Manipulation module: 1-2 hours
- Remaining modules: 2-3 hours
- Testing and fixes: 1-2 hours
- **Total: 8-13 hours**

## Notes

- The original file has 5619 lines
- Target: 18 files averaging ~300 lines each
- All files under 400 lines to avoid token limits
- Maintain backward compatibility - no API changes
