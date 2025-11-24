# RTL Snapshot Test Mirroring Guide

This guide explains how to mirror LTR snapshot tests to create RTL snapshot tests.

## Overview

The test output is **1280×720 pixels**. In LTR mode, columns start at x=0 and grow rightward. In RTL mode, columns should start at x=1280 and grow leftward (right-aligned).

## Mirroring Formula

For a tile/column at LTR position `x` with width `w`:

```
RTL_x = OUTPUT_WIDTH - LTR_x - w
RTL_x = 1280 - LTR_x - w
```

### Edge Positions

- **LTR**: `left = x`, `right = x + w`
- **RTL**: `right = 1280 - x`, `left = 1280 - x - w`

## Examples

### Single Column (1/3 width = 426px)
- **LTR**: `left: 0`, `right: 426`, `width: 426`
- **RTL**: `left: 854`, `right: 1280`, `width: 426`
- Calculation: `854 = 1280 - 0 - 426`

### Single Column (1/2 width = 640px)
- **LTR**: `left: 0`, `right: 640`, `width: 640`
- **RTL**: `left: 640`, `right: 1280`, `width: 640`
- Calculation: `640 = 1280 - 0 - 640`

### Single Column (2/3 width = 853px)
- **LTR**: `left: 0`, `right: 853`, `width: 853`
- **RTL**: `left: 427`, `right: 1280`, `width: 853`
- Calculation: `427 = 1280 - 0 - 853`

### Single Column (Fixed 400px)
- **LTR**: `left: 0`, `right: 400`, `width: 400`
- **RTL**: `left: 880`, `right: 1280`, `width: 400`
- Calculation: `880 = 1280 - 0 - 400`

## Multiple Columns

For multiple columns, apply the formula to each column individually:

### Two Columns Example (both 1/3 width = 426px each)
**LTR**:
- Column 0: `left: 0`, `right: 426`, `width: 426`
- Column 1: `left: 426`, `right: 852`, `width: 426`

**RTL** (reversed order):
- Column 0: `left: 854`, `right: 1280`, `width: 426` (was rightmost)
- Column 1: `left: 428`, `right: 854`, `width: 426` (was leftmost)

Calculations:
- Column 0 RTL: `854 = 1280 - 426 - 0` (mirror of LTR Column 1)
- Column 1 RTL: `428 = 1280 - 852 - 0` (mirror of LTR Column 0)

## View Offset Mirroring

View offset also needs to be mirrored. If LTR has a view offset of `v`:

```
RTL_view_offset = -(total_width - OUTPUT_WIDTH - LTR_view_offset)
```

For simple cases where columns fit on screen, view offset is typically 0 in both LTR and RTL.

## Implementation Helper

The `00_rtl.rs` file includes a helper function:

```rust
const OUTPUT_WIDTH: f64 = 1280.0;

fn mirror_x(ltr_x: f64, width: f64) -> f64 {
    OUTPUT_WIDTH - ltr_x - width
}
```

## Test Status

Currently, all RTL tests are **failing** because RTL mode is not yet implemented. The tests show:
- **Current behavior**: LTR (columns at x=0)
- **Expected behavior**: RTL (columns right-aligned)

Once RTL is implemented, these tests should pass.

## Creating New RTL Tests

1. Start with the LTR test
2. Calculate mirrored positions using the formula above
3. Update snapshot expectations to show RTL positions
4. Add `_rtl` suffix to test names
5. Document the mirroring calculation in comments

## Verification

Use the `format_column_edges()` helper to verify actual positions:

```rust
let edges = format_column_edges(&layout);
assert_snapshot!(edges, @r"
left: 854 right:1280 width: 426
");
```

This shows the actual rendered position and makes it easy to verify the mirroring is correct.
