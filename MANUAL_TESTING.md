# Manual Testing for Golden Tests

## Overview

Each golden test category has its own manual testing guide with minimal config files and structured testing forms.

## Test Directories

### Category 00: Single Column Spawning
- **Location:** `src/layout/tests/golden_tests/00_spawning_single/`
- **Manual Guide:** [MANUAL.md](src/layout/tests/golden_tests/00_spawning_single/MANUAL.md)
- **Configs:** `manual/ltr-default-*.kdl` and `manual/rtl-default-*.kdl`
- **Tests:** 5 tests covering single column with various widths

### Category 01: Multiple Column Spawning
- **Location:** `src/layout/tests/golden_tests/01_spawning_multiple/`
- **Manual Guide:** [MANUAL.md](src/layout/tests/golden_tests/01_spawning_multiple/MANUAL.md)
- **Configs:** `manual/ltr-default-*.kdl` and `manual/rtl-default-*.kdl`
- **Tests:** 7 tests covering multiple columns including overflow scenarios

## Configuration Files

Instead of one config per test, we have **minimal base configs** that are reused:

| Config | Default Width | Usage |
|--------|--------------|-------|
| `ltr-default-1-3.kdl` | 1/3 (426px) | LTR tests with 1/3 columns |
| `rtl-default-1-3.kdl` | 1/3 (426px) | RTL tests with 1/3 columns |
| `ltr-default-1-2.kdl` | 1/2 (640px) | LTR tests with 1/2 columns |
| `rtl-default-1-2.kdl` | 1/2 (640px) | RTL tests with 1/2 columns |
| `ltr-default-2-3.kdl` | 2/3 (853px) | LTR tests with 2/3 columns |
| `rtl-default-2-3.kdl` | 2/3 (853px) | RTL tests with 2/3 columns |

## How to Test

### 1. Choose a Test Category
```bash
# Single column tests
cd src/layout/tests/golden_tests/00_spawning_single
cat MANUAL.md

# Multiple column tests
cd src/layout/tests/golden_tests/01_spawning_multiple
cat MANUAL.md
```

### 2. Run the Config
Each test in the MANUAL.md tells you which config to use:

```bash
niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-1-3.kdl
```

### 3. Follow Test Steps
The MANUAL.md provides:
- Which config to use
- What buttons to press (e.g., open 3 windows)
- What you should see (expected values)
- A form to write what you actually see

### 4. Record Your Observations
Each test has a "What I Actually See" section where you fill in:
- Column X positions
- Column widths
- Active tile viewport position
- Whether behavior matches expectations
- Notes about any issues

## Example

From `00_spawning_single/MANUAL.md`:

```markdown
## Test 1: spawn_single_column_one_third

### Config
- **LTR:** `ltr-default-1-3.kdl`

### Steps
1. Start niri: `niri --config src/layout/tests/.../ltr-default-1-3.kdl`
2. Open 1 terminal window
3. Observe and record

### Expected (LTR)
- Column X position: **0** (left edge)
- Column width: **426px** (1/3 of 1280)
- Active tile viewport X: **0**

### What I Actually See

**LTR:**
```
Column X: ______
Width: ______
Active tile viewport X: ______
Notes:




```
```

## Critical Verification

### Active Tile Must Be Visible ⚠️

**This is the #1 invariant!**

For every test:
- [ ] Active window has colored border
- [ ] Active window is on screen
- [ ] `active_tile_viewport_x` is within [0, 1280)

If active tile is off-screen → **BUG**

### RTL Mirroring

For RTL tests, verify:
- [ ] Columns grow right-to-left (opposite of LTR)
- [ ] First column is at RIGHT edge, not left
- [ ] Column positions follow formula: `RTL_X = 1280 - LTR_X - width`

## Comparing to Automated Tests

After manual testing, compare to automated test output:

```bash
# Run automated test
cargo test --lib golden_tests::00_spawning_single::spawn_single_column_one_third -- --nocapture

# Compare the snapshot output to your manual observations
```

The automated test shows:
- `view_pos` - Viewport scroll position
- `active_column_x` - Column position in content space
- `active_tile_viewport_x` - Where active tile appears on screen
- Column and tile positions with `[ACTIVE]` markers

## Structure Benefits

### Minimal Configs
- Only 6 config files per category (3 widths × 2 modes)
- Configs are reused across multiple tests
- Easy to maintain

### Test-Specific Guides
- Each MANUAL.md is tailored to its test category
- Tells you exactly which config to use for each test
- Provides forms to record observations
- Includes expected values for verification

### Co-located with Tests
- Configs live next to the test code they verify
- Easy to find related files
- Clear organization by test category

## Next Steps

1. **Start with simple tests:** Begin with category 00 (single column)
2. **Record observations:** Fill out the "What I Actually See" forms
3. **Compare to expectations:** Check if values match
4. **Test RTL:** Run the same test in RTL mode and verify mirroring
5. **Try overflow tests:** Category 01 includes scrolling scenarios

## Documentation

- [Enhanced Snapshot Format](docs/SNAPSHOT_FORMAT.md) - Understanding test output
- [Snapshot Quick Reference](SNAPSHOT_QUICK_REFERENCE.md) - Field meanings
- Individual test manuals in each category directory
