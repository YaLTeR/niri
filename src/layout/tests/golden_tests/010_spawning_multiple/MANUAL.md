# Manual Testing Guide: Multiple Column Spawning

## Available Configs

| Config | Default Width | Use For |
|--------|--------------|---------|
| `ltr-default-1-3.kdl` | 1/3 (426px) | LTR 1/3 width tests |
| `rtl-default-1-3.kdl` | 1/3 (426px) | RTL 1/3 width tests |
| `ltr-default-1-2.kdl` | 1/2 (640px) | LTR 1/2 width tests |
| `rtl-default-1-2.kdl` | 1/2 (640px) | RTL 1/2 width tests |

## Screen Dimensions
All tests use: **1280x720**

---

## Test 1: spawn_one_third_one_tile

### Config
- **LTR:** `ltr-default-1-3.kdl`
- **RTL:** `rtl-default-1-3.kdl`

### Steps
1. Start niri: `niri --config src/layout/tests/golden_tests/01_spawning_multiple/manual/ltr-default-1-3.kdl`
2. Open 1 terminal window
3. Observe and record

### Expected (LTR)
- Column 0: X=**0**, width=**426px**
- Empty space: **854px** (2/3 of screen)
- Active column: **0**
- Active tile viewport X: **0**

### Expected (RTL)
- Column 0: X=**854**, width=**426px**
- Empty space: **854px** on left
- Active column: **0**
- Active tile viewport X: **854**

### What I Actually See

**LTR:**
```
Column 0 X: ______
Width: ______
Active tile viewport X: ______
Notes:




```

**RTL:**
```
Column 0 X: ______
Width: ______
Active tile viewport X: ______
Notes:




```

---

## Test 2: spawn_one_third_two_tiles

### Config
- **LTR:** `ltr-default-1-3.kdl`
- **RTL:** `rtl-default-1-3.kdl`

### Steps
1. Start niri with config
2. Open terminal window #1
3. Open terminal window #2
4. Observe and record

### Expected (LTR)
- Column 0: X=**0**, width=**426px**
- Column 1: X=**426**, width=**426px**
- Empty space: **428px** (1/3 of screen)
- Active column: **1**
- Active tile viewport X: **426**

### Expected (RTL)
- Column 0: X=**854**, width=**426px**
- Column 1: X=**428**, width=**426px**
- Empty space: **428px** on left
- Active column: **1**
- Active tile viewport X: **428**

### What I Actually See

**LTR:**
```
Column 0 X: ______ Width: ______
Column 1 X: ______ Width: ______
Active column: ______
Active tile viewport X: ______
Notes:




```

**RTL:**
```
Column 0 X: ______ Width: ______
Column 1 X: ______ Width: ______
Active column: ______
Active tile viewport X: ______
Notes:




```

---

## Test 3: spawn_one_third_three_tiles

### Config
- **LTR:** `ltr-default-1-3.kdl`
- **RTL:** `rtl-default-1-3.kdl`

### Steps
1. Start niri with config
2. Open terminal window #1
3. Open terminal window #2
4. Open terminal window #3
5. Observe and record

### Expected (LTR)
- Column 0: X=**0**, width=**426px**
- Column 1: X=**426**, width=**426px**
- Column 2: X=**852**, width=**428px** (fills remaining)
- **Workspace full, no scrolling**
- Active column: **2**
- Active tile viewport X: **852**

### Expected (RTL)
- Column 0: X=**854**, width=**426px**
- Column 1: X=**428**, width=**426px**
- Column 2: X=**0**, width=**428px**
- **Workspace full, no scrolling**
- Active column: **2**
- Active tile viewport X: **0**

### What I Actually See

**LTR:**
```
Column 0 X: ______ Width: ______
Column 1 X: ______ Width: ______
Column 2 X: ______ Width: ______
All columns visible? YES / NO
Scrolling needed? YES / NO
Active tile viewport X: ______
Notes:




```

**RTL:**
```
Column 0 X: ______ Width: ______
Column 1 X: ______ Width: ______
Column 2 X: ______ Width: ______
All columns visible? YES / NO
Scrolling needed? YES / NO
Active tile viewport X: ______
Notes:




```

---

## Test 4: spawn_one_third_four_tiles (OVERFLOW TEST)

### Config
- **LTR:** `ltr-default-1-3.kdl`
- **RTL:** `rtl-default-1-3.kdl`

### Steps
1. Start niri with config
2. Open terminal windows #1, #2, #3, #4
3. **Test scrolling:**
   - Press `Mod+H` to focus left
   - Press `Mod+L` to focus right
4. Observe and record

### Expected (LTR)
- Column 0: X=**0**, width=**426px**
- Column 1: X=**426**, width=**426px**
- Column 2: X=**852**, width=**428px**
- Column 3: X=**1280**, width=**426px** (initially off-screen)
- **Workspace overflows**, scrolling required
- Active column: **3**
- Active tile viewport X: Should be visible (approx **200-600**)

### Expected (RTL)
- Column 0: X=**854**, width=**426px**
- Column 1: X=**428**, width=**426px**
- Column 2: X=**0**, width=**428px**
- Column 3: X=**-426**, width=**426px** (initially off-screen left)
- **Workspace overflows**, scrolling required
- Active column: **3**
- Active tile viewport X: Should be visible (approx **600-800**)

### What I Actually See

**LTR:**
```
After opening all 4 windows:
  Active column visible? YES / NO
  Active tile viewport X: ______

After pressing Mod+H:
  Which column now visible? ______
  Scroll happened? YES / NO

After pressing Mod+L:
  Back to column 3? YES / NO
  
Notes:




```

**RTL:**
```
After opening all 4 windows:
  Active column visible? YES / NO
  Active tile viewport X: ______

After pressing Mod+H (left in RTL = previous):
  Which column now visible? ______
  Scroll happened? YES / NO

After pressing Mod+L (right in RTL = next):
  Back to column 3? YES / NO
  
Notes:




```

---

## Test 5: spawn_one_half_one_tile

### Config
- **LTR:** `ltr-default-1-2.kdl`
- **RTL:** `rtl-default-1-2.kdl`

### Steps
1. Start niri with config
2. Open 1 terminal window
3. Observe and record

### Expected (LTR)
- Column 0: X=**0**, width=**640px**
- Empty space: **640px** (1/2 of screen)
- Active tile viewport X: **0**

### Expected (RTL)
- Column 0: X=**640**, width=**640px**
- Empty space: **640px** on left
- Active tile viewport X: **640**

### What I Actually See

**LTR:**
```
Column 0 X: ______
Width: ______
Active tile viewport X: ______
Notes:




```

**RTL:**
```
Column 0 X: ______
Width: ______
Active tile viewport X: ______
Notes:




```

---

## Test 6: spawn_one_half_two_tiles

### Config
- **LTR:** `ltr-default-1-2.kdl`
- **RTL:** `rtl-default-1-2.kdl`

### Steps
1. Start niri with config
2. Open terminal window #1
3. Open terminal window #2
4. Observe and record

### Expected (LTR)
- Column 0: X=**0**, width=**640px**
- Column 1: X=**640**, width=**640px**
- **Workspace exactly filled, no scrolling**
- Active tile viewport X: **640**

### Expected (RTL)
- Column 0: X=**640**, width=**640px**
- Column 1: X=**0**, width=**640px**
- **Workspace exactly filled, no scrolling**
- Active tile viewport X: **0**

### What I Actually See

**LTR:**
```
Column 0 X: ______ Width: ______
Column 1 X: ______ Width: ______
Workspace filled exactly? YES / NO
Active tile viewport X: ______
Notes:




```

**RTL:**
```
Column 0 X: ______ Width: ______
Column 1 X: ______ Width: ______
Workspace filled exactly? YES / NO
Active tile viewport X: ______
Notes:




```

---

## Test 7: spawn_one_half_three_tiles (OVERFLOW TEST)

### Config
- **LTR:** `ltr-default-1-2.kdl`
- **RTL:** `rtl-default-1-2.kdl`

### Steps
1. Start niri with config
2. Open terminal windows #1, #2, #3
3. Test scrolling with `Mod+H` and `Mod+L`
4. Observe and record

### Expected (LTR)
- Column 0: X=**0**, width=**640px**
- Column 1: X=**640**, width=**640px**
- Column 2: X=**1280**, width=**640px** (initially off-screen)
- **Workspace overflows**, scrolling required
- Active tile viewport X: Should be visible

### Expected (RTL)
- Column 0: X=**640**, width=**640px**
- Column 1: X=**0**, width=**640px**
- Column 2: X=**-640**, width=**640px** (initially off-screen left)
- **Workspace overflows**, scrolling required
- Active tile viewport X: Should be visible

### What I Actually See

**LTR:**
```
After opening all 3 windows:
  Active column visible? YES / NO
  Active tile viewport X: ______
  
Scrolling behavior:
  Mod+H works? YES / NO
  Mod+L works? YES / NO
  Active column stays visible? YES / NO
  
Notes:




```

**RTL:**
```
After opening all 3 windows:
  Active column visible? YES / NO
  Active tile viewport X: ______
  
Scrolling behavior:
  Mod+H works? YES / NO
  Mod+L works? YES / NO
  Active column stays visible? YES / NO
  
Notes:




```

---

## Verification Checklist

For each test, verify:

- [ ] **Active tile is ALWAYS visible** (critical!)
- [ ] Column X positions match expected
- [ ] Column widths match expected
- [ ] Active tile viewport X is within [0, 1280)
- [ ] For overflow tests: scrolling keeps active column visible
- [ ] RTL is mirrored from LTR

## Keyboard Shortcuts

- `Mod+L` - Focus right/next column
- `Mod+H` - Focus left/previous column
- `Mod+Shift+L` - Move column right
- `Mod+Shift+H` - Move column left
- `Mod+Q` - Close window

(Mod = Super/Windows key)

## Common Issues

### Active tile off-screen in overflow tests
**Symptom:** After spawning 4th window, can't see active window  
**Expected:** Workspace should scroll automatically to show active tile  
**Action:** This is a BUG - critical invariant violated

### No scrolling when expected
**Symptom:** All 4 columns visible when should overflow  
**Check:** Are column widths correct? Sum should exceed 1280px  
**Check:** Compare to golden file output

### RTL scrolling feels wrong
**Symptom:** Pressing H/L doesn't behave as expected  
**Note:** In RTL, "next" column is to the LEFT  
**Expected:** Mod+L focuses leftward, Mod+H focuses rightward

## Getting Automated Test Output

To see what the automated tests expect:

```bash
cargo test --lib golden_tests::01_spawning_multiple::spawn_one_third_three_tiles -- --nocapture
```

Compare the snapshot output to your observations.
