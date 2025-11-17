# Scrolling Layout RTL Notes

## Overall Goal
Implement per-output (or per-workspace) layout direction so the scrolling tiling layout can operate independently in left-to-right (LTR) or right-to-left (RTL) mode.

## Current Architecture Notes
1. **Config plumbing**
   - `niri_config::Layout` is merged per-output and per-workspace through `Options` (`src/layout/mod.rs`).
   - Each `Monitor` and `Workspace` clones/adjusts `Options`, so any new direction flag can live in `Layout` and travel down to `ScrollingSpace`.
2. **ScrollingSpace fundamentals**
   - Maintains `columns: Vec<Column<W>>`, `data: Vec<ColumnData>`, `active_column_idx`, and `view_offset`.
   - `column_xs()` builds monotonically increasing X coordinates assuming LTR. Rendering, hit testing, insert hints, and view offsets all pull from this helper.
   - Navigation and movement rely heavily on `active_column_idx` comparisons (0 as “left edge”, `len()-1` as “right edge”).
3. **Direction-sensitive entry points**
   - Column creation/removal: `add_column`, `add_tile`, `add_tile_right_of`, `remove_column_by_idx`, etc.
   - Navigation + focus: `focus_left/right`, `move_left/right`, `swap_window_in_direction`, `consume_or_expel_window_left/right`.
   - View logic + gestures: `view_pos`, `view_offset` helpers, `center_column`, `scroll_amount_to_activate`, `view_offset_gesture_*`, `dnd_scroll_gesture_*`.
   - Rendering/hit-testing: `tiles_with_render_positions`, `window_under`, `insert_hint_area`, popups.

## Strategy Outline
1. **LayoutDirection enum**
   - Add `LayoutDirection::{Ltr, Rtl}` to `niri_config::Layout` (default LTR) and expose via `Options`.
   - Workspaces inherit through `Options::with_merged_layout`, enabling per-output/workspace overrides via config parts.
   - Update `LayoutPart` KDL decoder + `default-config.kdl` comments so users can set `layout { direction "rtl" }` at global scope, and inside `output` / `workspace` overrides via their `layout` blocks.
2. **Helper methods in ScrollingSpace**
   - `fn layout_direction(&self) -> LayoutDirection` to read from options.
   - Index helpers: `first_column_index`, `last_column_index`, `neighbor_index(idx, ScrollDirection)`, `screen_left_index(idx)` etc., to abstract logical vs physical movement.
   - `fn physical_scroll_direction(dir: ScrollDirection) -> ScrollDirection` if needed for reuse across gesture code.
   - Keep helpers private to `scrolling.rs` to avoid leaking implementation detail to other modules.
3. **Column geometry**
   - Option A (preferred): keep internal vector LTR but flip coordinates when computing `column_xs()` or when consuming its values:
     - For RTL, compute positions from the right edge of total width (`total_width - cumulative_offset`), ensuring rendering + hit testing flip automatically.
   - Option B: reinterpret neighbor indices; more invasive because many call sites rely on `column_x()` being monotonic increasing.
   - Decision pending, but leaning toward flipping `column_xs()` to minimize call-site churn—must ensure `view_offset` math stays consistent.
4. **Navigation semantics**
   - Replace raw comparisons (`idx == 0`, `idx == len()-1`, `idx ± 1`) with helpers that map “screen-left”/“screen-right” to actual indices respecting direction.
   - Commands/gestures that treat ScrollDirection should pass through a `dir.adjust(direction)` helper so actions remain intuitive per output.
5. **View offset & gestures**
   - Gestures use deltas assuming positive X scrolls right. Need to invert deltas or `view_offset` adjustments when in RTL so screen-space intuition holds.
   - Snap-to-column logic compares against “leftmost/rightmost”; leverage helpers to pick appropriate boundary indices and to map deltas.
   - Verify `ViewOffset::offset` and gestures’ `delta_from_tracker` math work when we negate inputs; centralize the sign flip near gesture entrypoints.
6. **Testing**
   - `src/layout/scrolling.rs` has inline tests; plan to extend with:
     - RTL column X ordering test.
     - Focus movement tests verifying `focus_left/right` follow physical layout for both directions.
     - Basic gesture/view offset test if feasible (likely via smaller helper verifying computed offsets).
7. **Non-goals / constraints**
   - Do not alter default LTR behavior—should remain byte-for-byte the same.
   - Keep RTL conditionals localized; avoid large-scale refactors.
   - No changes to floating layout or unrelated subsystems unless direction flag plumbing requires it.

## Next Steps
1. Extend `niri_config::Layout`/`LayoutPart` with `direction` option (default LTR) and update merge logic.
2. Surface direction through `Options` to `Workspace` and `ScrollingSpace`.
3. Implement helper utilities for direction-aware indexing and ScrollDirection translation.
4. Update geometry, navigation, movement, and gesture code paths to rely on helpers.
5. Add regression tests covering both directions.

## Detailed Implementation Plan (idiomatic and architecture-friendly)
1. **Configuration & defaults**
   - Extend `niri_config::Layout` with `pub direction: LayoutDirection`, defaulting to `LayoutDirection::Ltr`.
   - Update `LayoutPart` to parse an optional `direction` scalar (decode via `LayoutDirection` implementing `knuffel::DecodeScalar`).
   - Mirror the new field into `niri_ipc` if needed for external reporting (ensure it’s optional to keep IPC backward compatible).
   - Document the option in `resources/default-config.kdl` and the wiki snippets (future PR).
2. **Plumbing through Options**
   - `Options::from_config` already clones `config.layout`; no extra work beyond ensuring the struct is `#[derive(Clone, PartialEq)]` with the new field.
   - Because per-output and per-workspace overrides use `Options::with_merged_layout`, the direction flag will automatically honor overrides as long as `merge_with` copies it from `LayoutPart`.
   - Add a helper `Options::layout_direction()` returning the enum for ergonomic reuse.
3. **Workspace/Monitor integration**
   - No structural changes needed: `Workspace::new_with_config` builds `ScrollingSpace` with the adjusted `options`, so `ScrollingSpace` can trust `options.layout.direction`.
   - Consider caching `LayoutDirection` inside `ScrollingSpace` if repeated lookups become noisy, but prefer borrowing from `options` for single source of truth.
4. **Helper API inside ScrollingSpace**
   - `fn dir(&self) -> LayoutDirection`.
   - `fn screen_left_of(&self, idx: usize) -> Option<usize>` and `screen_right_of` to map ScrollDirection to indices.
   - `fn logical_to_physical_idx(&self, idx: usize) -> usize` if coordinate math needs it; otherwise rely on `column_x` adjustments.
   - Provide `fn left_edge_index(&self) -> Option<usize>` / `right_edge_index` for boundary checks (focus/move/consume/expel).
5. **Geometry updates**
   - Modify `column_xs` to compute positions differently per direction:
     - LTR: status quo.
     - RTL: precompute total width (`sum widths + gaps`) and walk columns from start, subtracting offsets so first column gets the highest X.
   - Ensure `column_x` remains monotonic relative to screen coordinates even in RTL so existing callers comparing X values keep working.
6. **Navigation & movement**
   - Rewrite direct index math (especially `idx == 0`, `len()-1`, `wrapping_add_signed`) to go through helpers so physical direction is decoupled from Vec order.
   - `ScrollDirection` entrypoints (`focus_left`, `move_left`, `swap_window_in_direction`, etc.) should call `self.dir().adjust(direction)` returning a `LogicalDir` representing actual neighbor in storage order.
   - Column creation helpers (`add_column`, `add_tile_right_of`) need to determine insertion index based on direction: for RTL, inserting “to the right” means `active_idx` (visual right) which is `active - 1` in storage order.
7. **View offset & gestures**
   - When computing `view_pos` or applying gesture deltas, flip the sign for RTL so positive pointer movement still tracks physical right.
   - Carefully audit `view_offset.offset` usage to avoid double-negating.
8. **Rendering/hit-testing**
   - Once `column_xs` reflects physical positions, most rendering APIs (tiles, popups, insert hints) get RTL support “for free”. Verify insert hints referencing `column_idx` vs `column_x` remain correct when hit testing across reversed coordinates.
9. **Tests**
   - Add unit tests for helper methods and a combined scenario verifying: column order, focus traversal, and view offset after invoking gestures in both directions.
   - Run existing suites to ensure LTR snapshots remain unchanged.

After completing the above, reassess doc vs implementation to guarantee adherence to existing architecture and style.
