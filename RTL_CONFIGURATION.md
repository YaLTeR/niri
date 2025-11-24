# Right-to-Left (RTL) Configuration

This document describes the per-output RTL configuration feature for niri.

## Overview

By default, niri uses a left-to-right (LTR) scrolling layout where:
- New windows spawn on the right
- You scroll rightward to see more windows
- Columns are arranged from left to right

With RTL mode enabled:
- New windows spawn on the left
- You scroll leftward to see more windows
- Columns are arranged from right to left (mirrored)

## Configuration

RTL can be configured **per-output** or globally.

### Global Configuration

To enable RTL for all outputs by default, add it to the global `layout` section:

```kdl
layout {
    gaps 16
    right-to-left
    // ... other layout settings
}
```

### Per-Output Configuration

To enable RTL for specific outputs only, use the `layout` section within an output configuration:

```kdl
output "HDMI-A-1" {
    mode "1920x1080@60"
    scale 1
    
    layout {
        right-to-left
    }
}

output "eDP-1" {
    // This output will use LTR (default)
    mode "1920x1080@60"
}
```

### Mixed Configuration

You can set a global default and override it per-output:

```kdl
layout {
    // Global default: LTR
    gaps 16
}

output "HDMI-A-1" {
    // Override for this output: RTL
    layout {
        right-to-left
    }
}

output "DP-1" {
    // This output uses the global default (LTR)
    mode "2560x1440@144"
}
```

## Use Cases

### Multi-Monitor Setups

RTL is particularly useful in multi-monitor setups where you want different scrolling directions on different monitors:

```kdl
// Left monitor: RTL (scroll leftward)
output "HDMI-A-1" {
    position x=0 y=0
    layout {
        right-to-left
    }
}

// Right monitor: LTR (scroll rightward)
output "HDMI-A-2" {
    position x=1920 y=0
    // Uses default LTR
}
```

### Language/Cultural Preferences

Users who prefer right-to-left workflows (e.g., for languages like Arabic or Hebrew) can enable RTL globally:

```kdl
layout {
    right-to-left
    gaps 16
    // ... other settings
}
```

## Implementation Status

### ✅ Completed
- Configuration parsing (global and per-output)
- Configuration structure in `niri-config`
- Test infrastructure with mirroring calculator
- Documentation in default config

### ⚠️ Not Yet Implemented
- Actual RTL layout rendering logic
- Column positioning for RTL mode
- View offset calculations for RTL
- Navigation direction reversal

The configuration is ready to use, but the layout engine needs to be updated to respect the `right_to_left` flag. See the RTL snapshot tests in `src/layout/tests/snapshot_tests/00_rtl.rs` for expected behavior.

## Testing

Run the RTL snapshot tests to see expected vs. current behavior:

```bash
cargo test --lib rtl -- --nocapture
```

These tests currently fail, showing LTR behavior where RTL is expected. Once the layout engine is updated, these tests should pass.

## Technical Details

### Configuration Fields

- **Type**: `bool`
- **Default**: `false` (LTR)
- **Scope**: Can be set globally in `layout {}` or per-output in `output { layout {} }`
- **Merge behavior**: Per-output settings override global settings

### Data Structure

```rust
// In niri_config::Layout
pub struct Layout {
    // ... other fields
    pub right_to_left: bool,
}
```

### Mirroring Formula

For RTL layout, column positions are mirrored using:

```
RTL_x = OUTPUT_WIDTH - LTR_x - column_width
```

See `src/layout/tests/snapshot_tests/RTL_MIRRORING_GUIDE.md` for detailed mirroring calculations.
