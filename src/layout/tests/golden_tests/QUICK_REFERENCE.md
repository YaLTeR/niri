# Golden Tests - Quick Reference

## Structure

```
00_spawning_single/
├── MANUAL.md              # 5 tests with observation forms
└── manual/
    ├── ltr-default-1-3.kdl
    ├── ltr-default-1-2.kdl
    ├── ltr-default-2-3.kdl
    ├── rtl-default-1-3.kdl
    ├── rtl-default-1-2.kdl
    └── rtl-default-2-3.kdl

01_spawning_multiple/
├── MANUAL.md              # 7 tests with observation forms
└── manual/
    └── (same 6 configs)
```

## Config Quick Lookup

| Need | Use Config |
|------|-----------|
| 1/3 width LTR | `ltr-default-1-3.kdl` |
| 1/3 width RTL | `rtl-default-1-3.kdl` |
| 1/2 width LTR | `ltr-default-1-2.kdl` |
| 1/2 width RTL | `rtl-default-1-2.kdl` |
| 2/3 width LTR | `ltr-default-2-3.kdl` |
| 2/3 width RTL | `rtl-default-2-3.kdl` |

## How to Test

1. **Open the manual:**
   ```bash
   cat src/layout/tests/golden_tests/00_spawning_single/MANUAL.md
   ```

2. **Find your test** (e.g., "Test 1: spawn_single_column_one_third")

3. **Run the specified config:**
   ```bash
   niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-1-3.kdl
   ```

4. **Follow the steps** (e.g., "Open 1 terminal window")

5. **Compare** what you see to expected values

6. **Fill out the observation form** in MANUAL.md

## Critical Checks

- [ ] **Active tile visible?** (must be YES!)
- [ ] Column X matches expected?
- [ ] Column width matches expected?
- [ ] RTL mirrors LTR?

## Expected Values (1280x720 screen)

| Width | Pixels | LTR X | RTL X |
|-------|--------|-------|-------|
| 1/3 | 426 | 0 | 854 |
| 1/2 | 640 | 0 | 640 |
| 2/3 | 853 | 0 | 427 |

## Test Categories

### 00: Single Column (5 tests)
- spawn_single_column_one_third
- spawn_single_column_one_half
- spawn_single_column_two_thirds
- spawn_single_column_fixed_width
- column_x_positions_single_column

### 01: Multiple Columns (7 tests)
- spawn_one_third_one_tile
- spawn_one_third_two_tiles
- spawn_one_third_three_tiles ← full screen, no scrolling
- spawn_one_third_four_tiles ← **overflow test**
- spawn_one_half_one_tile
- spawn_one_half_two_tiles ← full screen, no scrolling
- spawn_one_half_three_tiles ← **overflow test**

## Full Documentation

- [MANUAL_TESTING.md](../../../MANUAL_TESTING.md) - Top-level guide
- `00_spawning_single/MANUAL.md` - Category 00 tests
- `01_spawning_multiple/MANUAL.md` - Category 01 tests
