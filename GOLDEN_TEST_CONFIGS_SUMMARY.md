# Golden Test Configurations - Summary

## What Was Created

Individual KDL configuration files for **every golden test**, replacing the shared `ltr-config-1-3.kdl` and `rtl-config-1-3.kdl` files.

## File Structure

```
resources/golden-tests/
├── README.md                                    # Test index and reference
├── QUICKSTART.md                                # Quick start guide
├── MANUAL_TESTING_GUIDE.md                      # Complete testing procedures
├── generate_all_configs.sh                      # Script to regenerate configs
│
├── 00_spawn_single_column_one_third_ltr.kdl    # LTR configs
├── 00_spawn_single_column_one_third_rtl.kdl    # RTL configs
├── 00_spawn_single_column_one_half_ltr.kdl
├── 00_spawn_single_column_one_half_rtl.kdl
├── ... (20 more test configs)
│
└── Total: 24 KDL files (12 LTR + 12 RTL)
```

## Benefits

### 1. Individual Test Reproduction
Each test can now be run manually with its exact configuration:
```bash
niri --config resources/golden-tests/01_spawn_one_third_three_tiles_ltr.kdl
```

### 2. Embedded Test Instructions
Every config file includes:
- Test description
- Expected behavior
- Step-by-step manual testing instructions
- Verification checklist

Example from a config file:
```kdl
// Golden Test: spawn_one_third_three_tiles (LTR)
// Description: Spawn three columns, each 1/3 width
// Expected: Workspace exactly filled, no scrolling needed

// Manual Test Steps:
// 1. Start: niri --config resources/golden-tests/01_spawn_one_third_three_tiles_ltr.kdl
// 2.1. Open terminal window #1
// 2.2. Open terminal window #2
// 2.3. Open terminal window #3
// 3. Verify the expected output matches the description above
```

### 3. Easy LTR/RTL Comparison
```bash
# Test LTR behavior
niri --config resources/golden-tests/<test>_ltr.kdl

# Test RTL behavior  
niri --config resources/golden-tests/<test>_rtl.kdl

# Compare visually - columns should be mirrored
```

### 4. Complete Documentation
Three levels of documentation:
- **QUICKSTART.md** - Get started in 2 minutes
- **README.md** - Test index and reference values
- **MANUAL_TESTING_GUIDE.md** - Complete procedures and troubleshooting

## Test Coverage

### Category 00: Single Column Spawning (5 tests × 2 modes = 10 configs)
- `spawn_single_column_one_third` - 1/3 width
- `spawn_single_column_one_half` - 1/2 width
- `spawn_single_column_two_thirds` - 2/3 width
- `spawn_single_column_fixed_width` - 400px fixed
- `column_x_positions_single_column` - Position verification

### Category 01: Multiple Column Spawning (7 tests × 2 modes = 14 configs)
- `spawn_one_third_one_tile` - One 1/3 column
- `spawn_one_third_two_tiles` - Two 1/3 columns
- `spawn_one_third_three_tiles` - Three 1/3 columns (full)
- `spawn_one_third_four_tiles` - Four 1/3 columns (overflow)
- `spawn_one_half_one_tile` - One 1/2 column
- `spawn_one_half_two_tiles` - Two 1/2 columns
- `spawn_one_half_three_tiles` - Three 1/2 columns (overflow)

## Usage Examples

### Basic Test
```bash
# Run a simple test
niri --config resources/golden-tests/00_spawn_single_column_one_third_ltr.kdl

# Open a terminal window
foot &

# Verify: Window at left edge, width ~426px
```

### Overflow Test
```bash
# Run overflow test
niri --config resources/golden-tests/01_spawn_one_third_four_tiles_ltr.kdl

# Open 4 terminals
# Verify: Workspace scrolls, active column visible
```

### RTL Verification
```bash
# Test RTL mirroring
niri --config resources/golden-tests/01_spawn_one_third_three_tiles_rtl.kdl

# Open 3 terminals
# Verify: Columns grow right-to-left (reversed from LTR)
```

## Key Verification Points

### 1. Active Tile Visibility ⚠️ CRITICAL
**The active tile MUST always be visible!**

- Look for colored border on active window
- Active window should be within screen bounds
- If active window is off-screen → **BUG**

### 2. Column Positions

**LTR expectations:**
```
Column 0: X=0 (left edge)
Column 1: X=426
Column 2: X=852
```

**RTL expectations (mirrored):**
```
Column 0: X=854 (right edge)  
Column 1: X=428
Column 2: X=0 (left edge)
```

### 3. Column Widths

| Config | Expected (1280px screen) |
|--------|-------------------------|
| proportion 0.33333 | 426px |
| proportion 0.5 | 640px |
| proportion 0.66667 | 853px |
| fixed 400 | 400px |

## Quick Start

### 1. Pick a Test
```bash
ls resources/golden-tests/*.kdl
```

### 2. Run It
```bash
niri --config resources/golden-tests/<test-name>.kdl
```

### 3. Follow Embedded Instructions
Instructions are in the KDL file itself (as comments at the bottom).

### 4. Verify Behavior
Check:
- Column positions match expected values
- Column widths are correct
- Active tile is visible
- Scrolling works (for overflow tests)

## Regenerating Configs

If you need to recreate the config files:

```bash
cd resources/golden-tests
./generate_all_configs.sh
```

This will regenerate all 24 config files.

## Documentation Files

| File | Purpose |
|------|---------|
| `QUICKSTART.md` | Get started quickly (2-minute read) |
| `README.md` | Test index, reference values, quick lookup |
| `MANUAL_TESTING_GUIDE.md` | Complete procedures, verification, troubleshooting |
| `generate_all_configs.sh` | Regenerate all config files |
| `*.kdl` | Individual test configurations (24 files) |

## Comparison to Old Approach

### Before ❌
- Shared configs: `ltr-config-1-3.kdl` and `rtl-config-1-3.kdl`
- Manual testing required guessing which test to reproduce
- No embedded instructions
- Hard to compare specific LTR/RTL scenarios

### After ✅
- Individual config per test (24 files)
- Each config is self-documenting
- Easy to run any specific test
- Clear LTR/RTL pairing with `_ltr.kdl` and `_rtl.kdl` suffixes
- Complete documentation and guides

## Example Testing Session

```bash
# 1. Quick start
cat resources/golden-tests/QUICKSTART.md

# 2. Pick a test
ls resources/golden-tests/*.kdl

# 3. Run LTR version
niri --config resources/golden-tests/01_spawn_one_third_three_tiles_ltr.kdl

# 4. Open 3 terminals and verify behavior

# 5. Run RTL version
niri --config resources/golden-tests/01_spawn_one_third_three_tiles_rtl.kdl

# 6. Compare - should be mirrored

# 7. Check golden file if needed
cat src/layout/tests/golden_tests/01_spawning_multiple/golden/spawn_one_third_three_tiles.txt
```

## Next Steps

1. **Try it out:**
   ```bash
   niri --config resources/golden-tests/QUICKSTART.md  # Read first
   niri --config resources/golden-tests/00_spawn_single_column_one_third_ltr.kdl
   ```

2. **Compare LTR and RTL:**
   Run the same test in both modes and observe the mirroring

3. **Test with overflow:**
   ```bash
   niri --config resources/golden-tests/01_spawn_one_third_four_tiles_ltr.kdl
   ```

4. **Read the full guide:**
   ```bash
   cat resources/golden-tests/MANUAL_TESTING_GUIDE.md
   ```

## Summary

✅ **24 individual test configs** (12 LTR + 12 RTL)  
✅ **Self-documenting** with embedded test steps  
✅ **Complete documentation** with 3-tier guide system  
✅ **Easy regeneration** via script  
✅ **Ready to use** for manual verification

Now every golden test can be run manually with a single command!
