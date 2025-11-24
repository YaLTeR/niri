# Golden Tests - Organized Structure

## Directory Structure

```
golden_tests/
├── README.md (this file)
├── mod.rs (shared helpers and test suite list)
└── 00_spawning_single/
    ├── mod.rs (test suite module)
    ├── ltr.rs (LTR tests with golden snapshots)
    ├── rtl.rs (RTL tests derived from LTR)
    └── golden/
        ├── spawn_single_column_one_third.txt
        ├── spawn_single_column_one_half.txt
        └── ... (immutable golden reference files)
```

## Philosophy

### LTR is the Specification
- **LTR snapshots are IMMUTABLE** - they define correct behavior
- Stored in `golden/*.txt` files
- Any change to LTR is a regression unless explicitly intended
- Use `assert_snapshot!()` with inline snapshots in `ltr.rs`

### RTL is Derived
- **RTL behavior is mathematically calculated** from LTR
- No separate RTL snapshots - prevents divergence
- RTL tests parse LTR golden files and calculate expected positions
- Uses mirror transformation: `rtl_x = OUTPUT_WIDTH - ltr_x - width`

### Single Source of Truth
- LTR golden files are the only source of truth
- RTL tests verify the mirror transformation is correct
- Prevents LTR and RTL from drifting apart

## File Responsibilities

### `mod.rs` (suite level)
- Declares `ltr` and `rtl` submodules
- May contain suite-specific helpers

### `ltr.rs`
- Contains LTR tests with `assert_snapshot!()`
- Tests define expected behavior
- Snapshots are inline in the test file

### `rtl.rs`
- Contains RTL tests that derive from LTR
- Parses LTR snapshots using `parse_snapshot_tiles()`
- Calculates expected RTL positions
- Verifies actual RTL geometry matches calculations

### `golden/*.txt` (optional, for future use)
- Could store extracted golden files if needed
- Currently using inline snapshots in `ltr.rs`

## Adding a New Test Suite

1. Create directory: `golden_tests/NN_test_name/`
2. Create `mod.rs`:
   ```rust
   use super::*;
   mod ltr;
   mod rtl;
   ```
3. Create `ltr.rs` with LTR tests
4. Create `rtl.rs` with RTL tests that derive from LTR
5. Add to `golden_tests/mod.rs`:
   ```rust
   #[path = "NN_test_name/mod.rs"]
   mod test_name;
   ```

## Running Tests

```bash
# Run all golden tests
cargo test --lib layout::tests::golden_tests

# Run specific suite
cargo test --lib layout::tests::golden_tests::spawning_single

# Run only LTR tests
cargo test --lib layout::tests::golden_tests::spawning_single::ltr

# Run only RTL tests
cargo test --lib layout::tests::golden_tests::spawning_single::rtl
```

## Migration from snapshot_tests

The old `snapshot_tests/` directory contains 33 numbered test files.
These will be gradually migrated to the new organized structure:
- `00_ltr_spawning_single.rs` → `00_spawning_single/ltr.rs`
- Create corresponding `rtl.rs` for each suite
- Group related tests together in directories
