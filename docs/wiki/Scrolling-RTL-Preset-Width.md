# Scrolling layout RTL preset column width semantics

This document explains **why** the `toggle_width` change in `ScrollingSpace` for RTL layouts is the correct fix for `switch-preset-column-width` (Mod+R), and how it relates to the existing LTR behavior.

The goal is to make RTL behave as a *behavioral mirror* of LTR, in spite of the coordinate system still being origin-at-top-left.

## Problem statement

In the scrolling layout with three equal columns (e.g. `1/3 + 1/3 + 1/3`), when the middle column is focused and the user presses `switch-preset-column-width` (Mod+R) in an RTL layout:

- **Expected behavior:**
  - The *right edge* of the active (middle) column stays visually anchored.
  - Increasing the preset width (e.g. from `1/3` to `1/2`) should expand the active column *leftwards*.
  - Intuitively: in RTL the active column should grow "towards the start" of the reading direction (the right), so the overflow should appear on the *left* side of the viewport.

- **Actual behavior before the fix:**
  - The *left edge* of the active column was effectively pinned.
  - The column grew/shrank in a way that ate into the *right* side of the viewport.
  - This is behaviorally identical to LTR and breaks the expectation that RTL should mirror LTR layout semantics.

The mismatch is easiest to see in the user's example:

- Baseline: `1/3 + 1/3 + 1/3`.
- Make the middle column active.
- Toggle preset width so the middle column becomes `1/2`.
- **Expected RTL view:** roughly

  - Leftmost column: only ~`1/6` visible.
  - Middle column: `1/2` visible.
  - Rightmost column: full `1/3` visible.

  i.e. the overflow happens on the *left* side.

- **Actual view before fix:** roughly

  - Leftmost column: full `1/3` visible.
  - Middle column: `1/2` visible.
  - Rightmost column: only ~`1/6` visible.

  i.e. the overflow happens on the *right* side.

This shows that the wrong edge was being treated as the invariant when the preset width changed.

## LTR baseline: which edge is pinned?

The key functions for computing column positions and view offset are in `src/layout/scrolling.rs`:

```rust
fn column_xs(&self, data: impl Iterator<Item = ColumnData>) -> impl Iterator<Item = f64> {
    let data: Vec<_> = data.collect();
    let gaps = self.options.layout.gaps;
    let mut positions = vec![0.; data.len() + 1];

    match self.dir() {
        LayoutDirection::Ltr => {
            let mut x = 0.;
            for (idx, column) in data.iter().enumerate() {
                positions[idx] = x;
                x += column.width + gaps;
            }
            positions[data.len()] = x;
        }
        LayoutDirection::Rtl => { /* ... */ }
    }

    positions.into_iter()
}

fn column_x(&self, column_idx: usize) -> f64 {
    self.column_xs(self.data.iter().copied())
        .nth(column_idx)
        .unwrap()
}

pub fn view_pos(&self) -> f64 {
    self.column_x(self.active_column_idx) + self.view_offset.current()
}
```

For **LTR**:

- `column_x[i] = sum_{j < i} (data[j].width + gaps)`.
- `view_pos()` is the camera origin used to position columns and tiles.

When `switch-preset-column-width` is invoked for the active column:

- Only the *active* column's width changes.
- LTR `column_x(active_idx)` depends only on widths of columns **before** it, which are unchanged.
- The view offset is *not* adjusted in LTR for this operation.

Therefore in LTR:

- The active column's **left edge** (at `column_x(active_idx)`) is stable in layout coordinates and on screen.
- The **right edge** moves when the width changes.
- Neighboring columns to the right are displaced to make room.

This is the intended LTR baseline: *pin the left edge, move the right edge*.

## RTL layout direction and column positions

For completeness, the RTL branch of `column_xs` is:

```rust
match self.dir() {
    LayoutDirection::Ltr => { /* above */ }
    LayoutDirection::Rtl => {
        let mut x = 0.;
        for idx in (0..data.len()).rev() {
            positions[idx] = x;
            x += data[idx].width + gaps;
        }
        positions[data.len()] = x;
    }
}
```

This means:

- Storage indices are still 0..N-1 from left to right in memory.
- In RTL, `column_x(idx)` becomes the *distance from the visual right edge moving leftwards* through the sequence of columns.

Crucially, if only the active column's width changes, **`column_x(active_idx)` still does not change**:

- The sum either ends before that column (LTR) or after it (RTL), in both cases involving only other columns' widths.

Therefore, in both LTR and RTL, the column's *anchor* in layout coordinates is the **same `column_x(idx)` value**.

The difference between LTR and RTL must therefore be expressed through the *view offset* and the way we decide which edge to keep fixed.

## Why the first RTL attempt failed

The first attempt to implement RTL edge pinning measured the width change like this:

```rust
let old_width = self.columns[active_idx].width();
self.columns[active_idx].toggle_width(None, forwards);
let new_width = self.columns[active_idx].width();
let delta = new_width - old_width;
self.view_offset.offset(delta);
```

However, `Column::width()` is defined as:

```rust
fn width(&self) -> f64 {
    let mut tiles_width = self
        .data
        .iter()
        .map(|data| NotNan::new(data.size.w).unwrap())
        .max()
        .map(NotNan::into_inner)
        .unwrap();

    if self.display_mode == ColumnDisplay::Tabbed && self.sizing_mode().is_normal() {
        let extra_size = self.tab_indicator.extra_size(self.tiles.len(), self.scale);
        tiles_width += extra_size.w;
    }

    tiles_width
}
```

`TileData.size.w` only updates after:

- `set_column_width` calls `update_tile_sizes(animate)`, and
- clients receive the configure, then commit the new buffer size.

Immediately after `toggle_width()` returns, the new desired width (`ColumnWidth`) is known, but the **actual tile sizes and `data.size.w` have not changed yet**. In that window:

- `old_width ≈ new_width`.
- `delta ≈ 0`.
- `self.view_offset.offset(delta)` is effectively a no-op.

So the view offset remained unchanged in RTL, and the column behaved just like LTR: the wrong edge appeared to be pinned.

This explains the observed behavior: the code *intended* to shift the view but measured the wrong quantity.

## Correct quantity to measure: resolved column width

The code already has a precise way to convert the requested `ColumnWidth` into a *resolved* pixel width using the same logic as `update_tile_sizes`:

```rust
fn resolve_column_width(&self, width: ColumnWidth) -> f64 {
    let working_size = self.working_area.size;
    let gaps = self.options.layout.gaps;
    let extra = self.extra_size();

    match width {
        ColumnWidth::Proportion(proportion) => {
            (working_size.w - gaps) * proportion - gaps - extra.w
        }
        ColumnWidth::Fixed(width) => width,
    }
}
```

`set_column_width` uses this resolved width to drive the size requests:

```rust
fn set_column_width(&mut self, change: SizeChange, tile_idx: Option<usize>, animate: bool) {
    let current = if self.is_full_width || self.is_pending_maximized {
        ColumnWidth::Proportion(1.)
    } else {
        self.width
    };

    // Compute new ColumnWidth from SizeChange...

    self.width = width;
    self.preset_width_idx = None;
    self.is_full_width = false;
    self.is_pending_maximized = false;
    self.update_tile_sizes(animate);
}
```

The key observation is:

- **Immediately after** `toggle_width` returns, `Column::width` (the `ColumnWidth` field) is already updated to the new preset.
- We can call `resolve_column_width` on `self.width` before and after the toggle to get a correct estimate of the column width in logical pixels *without waiting for client commits*.

This resolved width is the correct measure for how much the column will grow/shrink logically, and thus the right input for adjusting the view offset.

## The final RTL fix in `toggle_width`

The updated implementation in `ScrollingSpace::toggle_width` is:

```rust
pub fn toggle_width(&mut self, forwards: bool) {
    if self.columns.is_empty() {
        return;
    }

    let active_idx = self.active_column_idx;

    if self.dir() == LayoutDirection::Rtl {
        // In RTL, keep the right edge of the active column visually pinned when toggling
        // preset width. This mirrors LTR behavior (where the left edge is effectively
        // pinned) in a behavioral sense.

        // Measure the current *resolved* column width in logical pixels, using the same
        // ColumnWidth + resolve_column_width logic that update_tile_sizes() uses, but without
        // waiting for client commits.
        let old_width = {
            let col = &self.columns[active_idx];
            let current = if col.is_full_width || col.is_pending_maximized {
                ColumnWidth::Proportion(1.)
            } else {
                col.width
            };
            col.resolve_column_width(current)
        };

        // Apply the preset toggle, which updates the column's desired width and schedules
        // tile size changes.
        {
            let col = &mut self.columns[active_idx];
            col.toggle_width(None, forwards);
        }

        // Measure the new resolved width and compute how much it changed.
        let new_width = {
            let col = &self.columns[active_idx];
            let current = if col.is_full_width || col.is_pending_maximized {
                ColumnWidth::Proportion(1.)
            } else {
                col.width
            };
            col.resolve_column_width(current)
        };

        let delta = new_width - old_width;

        // Adjust the view offset so that the active column's right edge stays at the same
        // screen X position. Using ViewOffset::offset ensures ongoing animations/gestures
        // are translated consistently instead of being reset.
        if delta != 0. {
            self.view_offset.offset(delta);
        }

        let col = &mut self.columns[active_idx];
        cancel_resize_for_column(&mut self.interactive_resize, col);
    } else {
        let col = &mut self.columns[active_idx];
        col.toggle_width(None, forwards);

        cancel_resize_for_column(&mut self.interactive_resize, col);
    }
}
```

### Why this pins the **right** edge in RTL

Let:

- `col_x = column_x(active_idx)` — anchor position of the active column in layout coordinates.
- `w` — resolved column width before the toggle.
- `w'` — resolved width after the toggle.
- `v` — scalar view position (`view_pos()`), i.e. camera origin.

In the scrolling layout, the on-screen x of the active column's **right edge** is:

```text
screen_right = col_x + w - v
```

We want this to be invariant under the width change `w → w'` in RTL.

Before the change:

```text
screen_right_before = col_x + w - v
```

After the change, if we adjust view position to `v'` by applying `delta` to `view_offset`:

```text
v' = v + delta
screen_right_after = col_x + w' - v'
                   = col_x + w' - (v + delta)
                   = col_x + w' - v - delta
```

We choose:

```text
delta = w' - w
```

Then:

```text
screen_right_after = col_x + w' - v - (w' - w)
                   = col_x + w - v
                   = screen_right_before
```

That is, **for any** `col_x`, as long as we use the same `col_x` before and after (and we do, since column positions do not depend on the active column’s width), the right edge is guaranteed to stay fixed.

The code computes exactly this `delta` using the resolved widths:

```rust
let delta = new_width - old_width;
if delta != 0. {
    self.view_offset.offset(delta);
}
```

Because `view_pos()` is defined as:

```rust
pub fn view_pos(&self) -> f64 {
    self.column_x(self.active_column_idx) + self.view_offset.current()
}
```

and `column_x(self.active_column_idx)` is unchanged by the preset toggle, applying `delta` to `view_offset` corresponds precisely to shifting `v` by `delta` in the derivation above.

### Why LTR remains correct and unchanged

In LTR, we deliberately **do not** adjust the view offset:

```rust
} else {
    let col = &mut self.columns[active_idx];
    col.toggle_width(None, forwards);

    cancel_resize_for_column(&mut self.interactive_resize, col);
}
```

As discussed earlier, this preserves the intended LTR semantics:

- `column_x(active_idx)` is unchanged by the width toggle.
- `view_offset` is unchanged.
- Therefore the **left edge** of the active column remains fixed on screen.
- The right edge moves, which is the existing and desired behavior.

The RTL branch thus becomes a behavioral mirror of LTR:

- LTR: **left edge pinned**, right edge moves.
- RTL: **right edge pinned**, left edge moves.

Both are achieved in a way consistent with the existing coordinate system and with the semantics of `ColumnWidth` and `resolve_column_width`.

## Interaction with animations and gestures

`ViewOffset::offset` is implemented as:

```rust
impl ViewOffset {
    pub fn offset(&mut self, delta: f64) {
        match self {
            ViewOffset::Static(offset) => *offset += delta,
            ViewOffset::Animation(anim) => anim.offset(delta),
            ViewOffset::Gesture(gesture) => {
                gesture.stationary_view_offset += delta;
                gesture.delta_from_tracker += delta;
                gesture.current_view_offset += delta;
            }
        }
    }
}
```

Using `offset(delta)` rather than overwriting the view offset has two important properties:

1. **Scroll animations remain smooth.** If a horizontal view animation is already in progress, it simply gets translated by `delta` rather than snapping.
2. **Gesture state remains consistent.** For ongoing gestures, all relevant fields are updated by the same `delta`, keeping internal invariants intact.

This matches how other parts of the code keep view behavior smooth in response to dynamic width changes (e.g. interactive resize).

## Summary

The final RTL fix for `switch-preset-column-width` in the scrolling layout is correct because:

- It mirrors the existing, intentional LTR semantics: LTR pins the left edge; RTL now pins the right edge.
- It uses the *resolved* column width (`ColumnWidth` + `resolve_column_width`), which reflects the logical size change at the moment of the preset toggle, rather than depending on delayed client commits.
- It computes a view offset delta `delta = w' - w` and applies it via `ViewOffset::offset`, which mathematically guarantees that the active column's right edge remains at the same screen x position in RTL.
- It leaves LTR behavior untouched and consistent with existing tests and user expectations.
- It integrates cleanly with the existing animation and gesture system without introducing snapping or breaking invariants.
