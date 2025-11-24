# Golden Tests for Scrolling Layout

This directory contains golden tests that verify the **refactored** scrolling layout behavior
matches the **original** (known-good) implementation.

## Architecture

The test system uses two implementations:

1. **`scrolling_original.rs`** - The original monolithic scrolling implementation (known-good)
2. **`scrolling/`** - The refactored modular implementation (being tested)

**Snapshot tests** run with `--features scrolling-original` to capture the correct behavior.
**Golden tests** run without the feature to test the refactored code against those snapshots.

## Structure

```
golden_tests/
├── mod.rs                    # Main module, macros, and shared helpers
├── rtl_calculator.rs         # RTL position calculation utilities
├── 000_spawning_single/      # Single column spawning tests
│   ├── mod.rs               # Test definitions
│   └── golden/              # Golden snapshot files
│       └── *.txt
├── 010_spawning_multiple/    # Multiple column spawning tests
│   ├── mod.rs
│   └── golden/
│       └── *.txt
└── README.md                 # This file
```

## Workflow

### Running Golden Tests

```bash
# Run all golden tests
cargo test --package niri --lib layout::tests::golden_tests

# Run specific test module
cargo test --package niri --lib layout::tests::golden_tests::spawning_multiple
```

### Regenerating Golden Files

Use the xtask to sync snapshots from the original implementation to golden files:

```bash
# Dry run - see what would be done
cargo xtask sync-golden --dry-run

# Actually sync (runs snapshot tests with scrolling-original feature, then extracts to golden files)
cargo xtask sync-golden
```

This will:
1. Run snapshot tests with `--features scrolling-original` to capture correct behavior
2. Extract all snapshots from the test files
3. Write them to the corresponding `golden/` directories
4. Generate stub `mod.rs` files for new modules

## Test Philosophy

### LTR Tests
- Use `assert_golden!` macro to compare against golden files
- Golden files are the **source of truth** for expected behavior
- Located in `<module>/golden/*.txt`

### RTL Tests
- Use `assert_golden_rtl!` macro
- RTL geometry is calculated from LTR golden files using mirror transformation
- Currently **ignored** until RTL scrolling implementation is complete
- Will verify that RTL is a mathematical mirror of LTR

## Relationship to Snapshot Tests

- **Snapshot tests** (`snapshot_tests/`): Use insta for inline snapshots, quick iteration
- **Golden tests** (`golden_tests/`): Use file-based snapshots, more stable reference

The `generate_golden.rs` module can regenerate golden files from the current implementation,
ensuring golden tests stay in sync with actual behavior.

## Adding New Tests

1. Add test operations in the module's `mod.rs`
2. Add LTR test function using `assert_golden!`
3. Add RTL test function using `assert_golden_rtl!` (mark as `#[ignore]` if RTL not ready)
4. Add generator function in `generate_golden.rs`
5. Run the generator to create the golden file
