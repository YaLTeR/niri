# Test Suite Status: 00_spawning_single

## Current Status: ✅ LTR Complete | ❌ RTL Not Implemented

### LTR Tests (5/5 passing ✓)
- `spawn_single_column_one_third` ✓
- `spawn_single_column_one_half` ✓
- `spawn_single_column_two_thirds` ✓
- `spawn_single_column_fixed_width` ✓
- `column_x_positions_single_column` ✓

### RTL Tests (0/5 passing - expected)
- `spawn_single_column_one_third_rtl` ❌ - Expected x=854, got x=0
- `spawn_single_column_one_half_rtl` ❌ - Expected x=640, got x=0
- `spawn_single_column_two_thirds_rtl` ❌ - Expected x=427, got x=0
- `spawn_single_column_fixed_width_rtl` ❌ - Expected x=880, got x=0
- `column_x_positions_single_column_rtl` ❌ - Expected x=854, got x=0

## What the failures mean

The RTL tests are **correctly failing** because RTL layout is not yet implemented.
All tiles are rendering at x=0 (left-aligned) instead of being right-aligned.

### Expected behavior (from calculator):
- 1/3 width (426px) should be at x=854 (right-aligned)
- 1/2 width (640px) should be at x=640 (right-aligned)
- 2/3 width (853px) should be at x=427 (right-aligned)
- Fixed 400px should be at x=880 (right-aligned)

### Actual behavior:
- All tiles render at x=0 (left-aligned)
- RTL mode is not affecting tile positioning

## Next Steps for RTL Implementation

These tests provide TDD guidance for implementing RTL:

1. **Modify tile positioning logic** to respect `right_to_left` config
2. **Calculate band_origin_x** differently for RTL (start at OUTPUT_WIDTH)
3. **Reverse column ordering** or adjust x-coordinate calculations
4. **Run tests** - they will pass when RTL is correctly implemented

## Test Design

### LTR Tests
- Use `assert_snapshot!()` with inline snapshots
- Define the immutable specification
- Capture logical state (widths, heights, structure)

### RTL Tests
- Parse LTR snapshots to extract tile widths
- Calculate expected RTL positions using `calculate_rtl_positions()`
- Verify actual geometry matches calculated expectations
- Also verify logical state is identical (direction-agnostic)

This design ensures:
- ✅ Single source of truth (LTR snapshots)
- ✅ RTL is mathematically derived
- ✅ No duplicate snapshots
- ✅ Tests guide implementation
