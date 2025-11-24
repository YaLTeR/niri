# Manual Testing Reorganization - Summary

## What Changed

Reorganized manual testing from **24 duplicate config files** to **12 minimal base configs + 2 comprehensive testing guides**.

## New Structure

```
src/layout/tests/golden_tests/
├── 00_spawning_single/
│   ├── MANUAL.md                    # 5 tests with observation forms
│   └── manual/
│       ├── ltr-default-1-3.kdl     # Base config: LTR, default width 1/3
│       ├── rtl-default-1-3.kdl     # Base config: RTL, default width 1/3
│       ├── ltr-default-1-2.kdl     # Base config: LTR, default width 1/2
│       ├── rtl-default-1-2.kdl     # Base config: RTL, default width 1/2
│       ├── ltr-default-2-3.kdl     # Base config: LTR, default width 2/3
│       └── rtl-default-2-3.kdl     # Base config: RTL, default width 2/3
│
└── 01_spawning_multiple/
    ├── MANUAL.md                    # 7 tests with observation forms
    └── manual/
        ├── ltr-default-1-3.kdl     # Same 6 base configs
        ├── rtl-default-1-3.kdl
        ├── ltr-default-1-2.kdl
        ├── rtl-default-1-2.kdl
        ├── ltr-default-2-3.kdl
        └── rtl-default-2-3.kdl
```

## Base Configs (Minimal Set)

Only **6 config files per category**, reused across all tests:

| Config | Default Width | Description |
|--------|--------------|-------------|
| `ltr-default-1-3.kdl` | proportion 0.33333 | LTR with 1/3 width columns |
| `rtl-default-1-3.kdl` | proportion 0.33333 | RTL with 1/3 width columns |
| `ltr-default-1-2.kdl` | proportion 0.5 | LTR with 1/2 width columns |
| `rtl-default-1-2.kdl` | proportion 0.5 | RTL with 1/2 width columns |
| `ltr-default-2-3.kdl` | proportion 0.66667 | LTR with 2/3 width columns |
| `rtl-default-2-3.kdl` | proportion 0.66667 | RTL with 2/3 width columns |

## MANUAL.md Format

Each test category has a comprehensive manual that:

### 1. Lists Available Configs
```markdown
## Available Configs

| Config | Default Width | Use For |
|--------|--------------|---------|
| `ltr-default-1-3.kdl` | 1/3 (426px) | LTR 1/3 width tests |
...
```

### 2. Provides Test-by-Test Instructions

For each test:
- **Which config to use**
- **Step-by-step instructions** (e.g., "Open 3 terminal windows")
- **Expected values** (column positions, widths, viewport positions)
- **Observation form** to fill out

### 3. Includes Observation Forms

Example:
```markdown
### What I Actually See

**LTR:**
```
Column X: ______
Width: ______
Active tile viewport X: ______
Notes:




```

**RTL:**
```
Column X: ______
Width: ______
Active tile viewport X: ______
Notes:




```
```

## Example Test Flow

### 1. Open the manual
```bash
cat src/layout/tests/golden_tests/00_spawning_single/MANUAL.md
```

### 2. Pick a test
Example: "Test 1: spawn_single_column_one_third"

### 3. See which config to use
```
Config:
- LTR: `ltr-default-1-3.kdl`
- RTL: `rtl-default-1-3.kdl`
```

### 4. Run it
```bash
niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-1-3.kdl
```

### 5. Follow steps
```
Steps:
1. Start niri (done)
2. Open 1 terminal window
3. Observe and record
```

### 6. Check expected values
```
Expected (LTR):
- Column X position: 0 (left edge)
- Column width: 426px (1/3 of 1280)
- Active tile viewport X: 0
```

### 7. Fill in observation form
Write what you actually see in the provided form

### 8. Repeat for RTL
Same test with `rtl-default-1-3.kdl`

## Benefits

### ✅ Minimal Config Files
- **Before:** 24 config files (lots of duplication)
- **After:** 12 config files (6 per category, reused)
- 50% reduction in files
- Easy to maintain

### ✅ Co-located with Tests
- Configs live in `manual/` subdirectory next to test code
- Easy to find related files
- Clear organization

### ✅ Comprehensive Test Guides
- Each MANUAL.md covers all tests in that category
- Tells you exactly which config to use
- Provides observation forms to fill out
- Includes expected values for comparison

### ✅ Structured Observation Forms
- Spaces to write what you actually see
- Compare directly to expected values
- Document issues as you test
- Easy to track which tests pass/fail

## File Count Comparison

| Approach | Config Files | Manuals | Total |
|----------|-------------|---------|-------|
| **Old (resources/golden-tests)** | 24 | 4 | 28 |
| **New (src/.../manual)** | 12 | 2 | 14 |

**50% fewer files, better organization!**

## Usage Example

### Category 00: Single Column Test

```bash
# 1. Read the manual
cat src/layout/tests/golden_tests/00_spawning_single/MANUAL.md

# 2. Run test 1 (LTR)
niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-1-3.kdl

# 3. Open a terminal window
# 4. Observe: Column at X=0, width=426px
# 5. Fill out observation form in MANUAL.md

# 6. Run test 1 (RTL)
niri --config src/layout/tests/golden_tests/00_spawning_single/manual/rtl-default-1-3.kdl

# 7. Verify: Column at X=854 (mirrored)
# 8. Fill out RTL observation form
```

### Category 01: Multiple Column Test

```bash
# 1. Read the manual
cat src/layout/tests/golden_tests/01_spawning_multiple/MANUAL.md

# 2. Run test 3: three tiles
niri --config src/layout/tests/golden_tests/01_spawning_multiple/manual/ltr-default-1-3.kdl

# 3. Open 3 terminal windows
# 4. Observe: All 3 columns fit, no scrolling
# 5. Record observations

# 6. Run test 4: four tiles (overflow)
# (same config, just open 4 windows instead)

# 7. Observe: Workspace scrolls, active column visible
# 8. Test scrolling with Mod+H and Mod+L
# 9. Record observations
```

## Deleted Files

Removed the old structure:
```bash
rm -rf resources/golden-tests/
```

This contained:
- 24 duplicate KDL files
- Generate script
- Multiple guide files

## Top-Level Documentation

Created **`MANUAL_TESTING.md`** at project root:
- Overview of manual testing structure
- Links to category-specific manuals
- Quick reference for config files
- Examples of testing workflow

## Critical Verification Points

Each manual emphasizes:

### 1. Active Tile Visibility ⚠️
**Most critical invariant!**
- Active window must have colored border
- Active window must be on screen
- `active_tile_viewport_x` must be within [0, 1280)

### 2. Column Positions
- Expected X coordinates provided
- Compare actual vs expected

### 3. RTL Mirroring
- RTL positions should mirror LTR
- Formula: `RTL_X = 1280 - LTR_X - width`

### 4. Scrolling Behavior (overflow tests)
- Viewport scrolls to keep active column visible
- Test with `Mod+H` and `Mod+L`

## Summary

**Before:**
- ❌ 24 duplicate configs scattered in resources/
- ❌ Generic guides not specific to tests
- ❌ No structured observation forms
- ❌ Hard to know which config for which test

**After:**
- ✅ 12 minimal base configs co-located with tests
- ✅ Test-specific manuals with detailed instructions
- ✅ Observation forms to record actual results
- ✅ Clear config-to-test mapping
- ✅ 50% fewer files, better organized

Now manual testing is **streamlined, organized, and easy to follow!**
