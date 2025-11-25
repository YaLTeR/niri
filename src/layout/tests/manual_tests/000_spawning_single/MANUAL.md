# Manual Testing Guide: Single Column Spawning

## Available Configs

| Config | Default Width | Use For |
|--------|--------------|---------|
| `ltr-default-1-3.kdl` | 1/3 (426px) | LTR 1/3 width tests |
| `rtl-default-1-3.kdl` | 1/3 (426px) | RTL 1/3 width tests |
| `ltr-default-1-2.kdl` | 1/2 (640px) | LTR 1/2 width tests |
| `rtl-default-1-2.kdl` | 1/2 (640px) | RTL 1/2 width tests |
| `ltr-default-2-3.kdl` | 2/3 (853px) | LTR 2/3 width tests |
| `rtl-default-2-3.kdl` | 2/3 (853px) | RTL 2/3 width tests |

## Screen Dimensions
All tests use: **1280x720**

---

## Test 1: spawn_single_column_one_third

### Config
- **LTR:** `ltr-default-1-3.kdl`
- **RTL:** `rtl-default-1-3.kdl`

### Steps
1. Start niri: `niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-1-3.kdl`
2. Open 1 terminal window
3. Observe and record

### Expected (LTR)
- Column X position: **0** (left edge)
- Column width: **426px** (1/3 of 1280)
- Active tile viewport X: **0**

### Expected (RTL)
- Column X position: **854** (right edge: 1280-426)
- Column width: **426px** (1/3 of 1280)
- Active tile viewport X: **854**

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

---

## Test 2: spawn_single_column_one_half

### Config
- **LTR:** `ltr-default-1-2.kdl`
- **RTL:** `rtl-default-1-2.kdl`

### Steps
1. Start niri: `niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-1-2.kdl`
2. Open 1 terminal window
3. Observe and record

### Expected (LTR)
- Column X position: **0** (left edge)
- Column width: **640px** (1/2 of 1280)
- Active tile viewport X: **0**

### Expected (RTL)
- Column X position: **640** (right edge: 1280-640)
- Column width: **640px** (1/2 of 1280)
- Active tile viewport X: **640**

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

---

## Test 3: spawn_single_column_two_thirds

### Config
- **LTR:** `ltr-default-2-3.kdl`
- **RTL:** `rtl-default-2-3.kdl`

### Steps
1. Start niri: `niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-2-3.kdl`
2. Open 1 terminal window
3. Observe and record

### Expected (LTR)
- Column X position: **0** (left edge)
- Column width: **853px** (2/3 of 1280)
- Active tile viewport X: **0**

### Expected (RTL)
- Column X position: **427** (right edge: 1280-853)
- Column width: **853px** (2/3 of 1280)
- Active tile viewport X: **427**

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

---

## Test 4: spawn_single_column_fixed_width

### Config
- **LTR:** `ltr-default-1-3.kdl` (any config works)
- **RTL:** `rtl-default-1-3.kdl` (any config works)

### Steps
1. Start niri with any config
2. Open 1 terminal window
3. **Resize to 400px:** Press `Mod+R` then type `:400` and Enter
4. Observe and record

### Expected (LTR)
- Column X position: **0** (left edge)
- Column width: **400px** (fixed)
- Active tile viewport X: **0**

### Expected (RTL)
- Column X position: **880** (right edge: 1280-400)
- Column width: **400px** (fixed)
- Active tile viewport X: **880**

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

---

## Test 5: column_x_positions_single_column

### Config
- **LTR:** `ltr-default-1-3.kdl`
- **RTL:** `rtl-default-1-3.kdl`

### Steps
1. Start niri: `niri --config src/layout/tests/golden_tests/00_spawning_single/manual/ltr-default-1-3.kdl`
2. Open 1 terminal window
3. Verify position accuracy
4. Compare to golden file: `cat golden/column_x_positions_single_column.txt`

### Expected (LTR)
- Column X position: **0.0** (exact)
- Column width: **426.0px** (exact)
- Active column X: **0.0**

### Expected (RTL)
- Column X position: **854.0** (exact)
- Column width: **426.0px** (exact)
- Active column X: **854.0**

### What I Actually See

**LTR:**
```
Column X: ______
Width: ______
Active column X: ______
Match golden file? YES / NO
Notes:




```

**RTL:**
```
Column X: ______
Width: ______
Active column X: ______
Match golden file? YES / NO
Notes:




```

---

## Verification Checklist

For each test, verify:

- [ ] **Active tile is visible** (most critical!)
- [ ] Column X position matches expected
- [ ] Column width matches expected
- [ ] Active tile viewport X matches expected
- [ ] RTL is mirrored from LTR

## Common Issues

### Active tile off-screen
**Symptom:** Can't see the active window  
**Expected:** Active tile viewport X should be within [0, 1280)  
**Action:** This is a BUG - document and report

### Wrong column position
**Symptom:** Column not at expected X coordinate  
**Check:** Verify correct config file is being used  
**Check:** Compare `view_pos` in automated test output

### RTL not mirrored
**Symptom:** RTL looks same as LTR  
**Check:** Verify config has `right-to-left true`  
**Formula:** RTL_X = 1280 - LTR_X - column_width

## Getting Automated Test Output

To see what the automated tests expect:

```bash
cargo test --lib golden_tests::00_spawning_single::spawn_single_column_one_third -- --nocapture
```

Compare the snapshot output to your observations.
