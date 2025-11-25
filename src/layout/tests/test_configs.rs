//! Shared test configuration presets for layout tests.
//!
//! This module defines named configuration presets that can be used across
//! snapshot tests and golden tests. Each preset is a unique configuration
//! that can be converted to both `Options` (for Rust tests) and `.kdl` files
//! (for manual testing).
//!
//! To regenerate `.config/*.kdl` files from these presets:
//! ```
//! cargo xtask generate-test-configs
//! ```

use niri_config::animations::{Curve, EasingParams, Kind};

use super::Options;

/// A named test configuration preset.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TestConfig {
    /// Unique identifier for this config (used for file names)
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// The layout configuration
    pub layout: LayoutConfig,
    /// Whether animations are enabled
    pub animations: AnimationConfig,
}

/// Layout-specific configuration.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct LayoutConfig {
    pub gaps: f64,
    pub struts: StrutsConfig,
    pub center_focused_column: CenterFocusedColumn,
    pub always_center_single_column: bool,
    pub default_column_width: Option<PresetSize>,
    pub preset_column_widths: PresetWidths,
    pub preset_window_heights: PresetWidths,
    pub default_column_display: ColumnDisplay,
    pub empty_workspace_above_first: bool,
    pub right_to_left: bool,
}

/// Preset widths array (up to 5 presets).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct PresetWidths {
    widths: [Option<PresetSize>; 5],
    len: usize,
}

impl PresetWidths {
    pub const fn new(widths: &[PresetSize]) -> Self {
        let mut arr = [None; 5];
        let mut i = 0;
        while i < widths.len() && i < 5 {
            arr[i] = Some(widths[i]);
            i += 1;
        }
        Self { widths: arr, len: widths.len() }
    }

    pub fn iter(&self) -> impl Iterator<Item = &PresetSize> {
        self.widths[..self.len].iter().filter_map(|o| o.as_ref())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StrutsConfig {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CenterFocusedColumn {
    #[default]
    Never,
    Always,
    OnOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum PresetSize {
    Proportion(f64),
    Fixed(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ColumnDisplay {
    #[default]
    Normal,
    Tabbed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationConfig {
    /// All animations disabled
    Off,
    /// Linear animations with 1000ms duration (for testing animation progress)
    Linear1000ms,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            gaps: 0.0,
            struts: StrutsConfig::default(),
            center_focused_column: CenterFocusedColumn::Never,
            always_center_single_column: false,
            default_column_width: Some(PresetSize::Proportion(1.0 / 3.0)),
            preset_column_widths: STANDARD_PRESETS,
            preset_window_heights: STANDARD_PRESETS,
            default_column_display: ColumnDisplay::Normal,
            empty_workspace_above_first: false,
            right_to_left: false,
        }
    }
}

// ============================================================================
// PRESET DEFINITIONS
// ============================================================================

/// Base configuration: gaps=0, default_width=1/3, presets=[1/3, 1/2, 2/3], animations off
pub const BASE: TestConfig = TestConfig {
    name: "base",
    description: "Base config with 1/3 default width, standard presets, no gaps, animations off",
    layout: LayoutConfig {
        gaps: 0.0,
        struts: StrutsConfig { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        center_focused_column: CenterFocusedColumn::Never,
        always_center_single_column: false,
        default_column_width: Some(PresetSize::Proportion(1.0 / 3.0)),
        preset_column_widths: STANDARD_PRESETS,
        preset_window_heights: STANDARD_PRESETS,
        default_column_display: ColumnDisplay::Normal,
        empty_workspace_above_first: false,
        right_to_left: false,
    },
    animations: AnimationConfig::Off,
};

/// Base configuration with RTL enabled
pub const BASE_RTL: TestConfig = TestConfig {
    name: "base-rtl",
    description: "Base config with RTL enabled",
    layout: LayoutConfig {
        right_to_left: true,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Default 1/2 width configuration
pub const DEFAULT_1_2: TestConfig = TestConfig {
    name: "default-1-2",
    description: "Default column width 1/2",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Proportion(1.0 / 2.0)),
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Default 1/2 width configuration with RTL
pub const DEFAULT_1_2_RTL: TestConfig = TestConfig {
    name: "default-1-2-rtl",
    description: "Default column width 1/2 with RTL",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Proportion(1.0 / 2.0)),
        right_to_left: true,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Default 1/3 width configuration (same as BASE but explicit name)
pub const DEFAULT_1_3: TestConfig = TestConfig {
    name: "default-1-3",
    description: "Default column width 1/3",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Proportion(1.0 / 3.0)),
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Default 1/3 width configuration with RTL
pub const DEFAULT_1_3_RTL: TestConfig = TestConfig {
    name: "default-1-3-rtl",
    description: "Default column width 1/3 with RTL",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Proportion(1.0 / 3.0)),
        right_to_left: true,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Default 2/3 width configuration
pub const DEFAULT_2_3: TestConfig = TestConfig {
    name: "default-2-3",
    description: "Default column width 2/3",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Proportion(2.0 / 3.0)),
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Default 2/3 width configuration with RTL
pub const DEFAULT_2_3_RTL: TestConfig = TestConfig {
    name: "default-2-3-rtl",
    description: "Default column width 2/3 with RTL",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Proportion(2.0 / 3.0)),
        right_to_left: true,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Fixed 400px default width
pub const DEFAULT_FIXED_400: TestConfig = TestConfig {
    name: "default-fixed-400",
    description: "Default column width fixed 400px",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Fixed(400)),
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Fixed 400px default width with RTL
pub const DEFAULT_FIXED_400_RTL: TestConfig = TestConfig {
    name: "default-fixed-400-rtl",
    description: "Default column width fixed 400px with RTL",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Fixed(400)),
        right_to_left: true,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Alternative presets: 2/5, 3/5, 4/5
pub const PRESETS_FIFTHS: TestConfig = TestConfig {
    name: "presets-fifths",
    description: "Alternative presets: 2/5, 3/5, 4/5",
    layout: LayoutConfig {
        default_column_width: Some(PresetSize::Proportion(2.0 / 5.0)),
        preset_column_widths: FIFTHS_PRESETS,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// With gaps (16px)
pub const WITH_GAPS_16: TestConfig = TestConfig {
    name: "with-gaps-16",
    description: "16px gaps between windows",
    layout: LayoutConfig {
        gaps: 16.0,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// With gaps (20px)
pub const WITH_GAPS_20: TestConfig = TestConfig {
    name: "with-gaps-20",
    description: "20px gaps between windows",
    layout: LayoutConfig {
        gaps: 20.0,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Large gaps (200px) for extreme testing
pub const WITH_GAPS_200: TestConfig = TestConfig {
    name: "with-gaps-200",
    description: "200px gaps (extreme config)",
    layout: LayoutConfig {
        gaps: 200.0,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// With struts (20px on all sides)
pub const WITH_STRUTS: TestConfig = TestConfig {
    name: "with-struts",
    description: "20px struts on all sides",
    layout: LayoutConfig {
        struts: StrutsConfig { left: 20.0, right: 20.0, top: 10.0, bottom: 10.0 },
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Center focused column: Always
pub const CENTER_ALWAYS: TestConfig = TestConfig {
    name: "center-always",
    description: "Center focused column always",
    layout: LayoutConfig {
        center_focused_column: CenterFocusedColumn::Always,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Center focused column: OnOverflow
pub const CENTER_ON_OVERFLOW: TestConfig = TestConfig {
    name: "center-on-overflow",
    description: "Center focused column on overflow",
    layout: LayoutConfig {
        center_focused_column: CenterFocusedColumn::OnOverflow,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Combined: gaps + struts + center on overflow
pub const COMBINED_GAPS_STRUTS_CENTER: TestConfig = TestConfig {
    name: "combined-gaps-struts-center",
    description: "16px gaps, struts, center on overflow",
    layout: LayoutConfig {
        gaps: 16.0,
        struts: StrutsConfig { left: 20.0, right: 20.0, top: 10.0, bottom: 10.0 },
        center_focused_column: CenterFocusedColumn::OnOverflow,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Empty workspace above first enabled
pub const EMPTY_WORKSPACE_ABOVE: TestConfig = TestConfig {
    name: "empty-workspace-above",
    description: "Empty workspace above first enabled",
    layout: LayoutConfig {
        empty_workspace_above_first: true,
        ..BASE.layout
    },
    animations: AnimationConfig::Off,
};

/// Animation testing: linear 1000ms
pub const ANIM_LINEAR: TestConfig = TestConfig {
    name: "anim-linear",
    description: "Linear animations with 1000ms duration",
    layout: BASE.layout,
    animations: AnimationConfig::Linear1000ms,
};

/// Animation testing: linear 1000ms with RTL
pub const ANIM_LINEAR_RTL: TestConfig = TestConfig {
    name: "anim-linear-rtl",
    description: "Linear animations with 1000ms duration, RTL",
    layout: LayoutConfig {
        right_to_left: true,
        ..BASE.layout
    },
    animations: AnimationConfig::Linear1000ms,
};

// ============================================================================
// PRESET ARRAYS
// ============================================================================

/// Standard presets: 1/3, 1/2, 2/3
const STANDARD_PRESETS: PresetWidths = PresetWidths::new(&[
    PresetSize::Proportion(0.33333333333333337),  // 1/3
    PresetSize::Proportion(0.5),                   // 1/2
    PresetSize::Proportion(0.6666666666666667),   // 2/3
]);

/// Fifths presets: 2/5, 3/5, 4/5
const FIFTHS_PRESETS: PresetWidths = PresetWidths::new(&[
    PresetSize::Proportion(0.4),  // 2/5
    PresetSize::Proportion(0.6),  // 3/5
    PresetSize::Proportion(0.8),  // 4/5
]);

/// All available test configs (for iteration/generation)
pub const ALL_CONFIGS: &[&TestConfig] = &[
    &BASE,
    &BASE_RTL,
    &DEFAULT_1_2,
    &DEFAULT_1_2_RTL,
    &DEFAULT_1_3,
    &DEFAULT_1_3_RTL,
    &DEFAULT_2_3,
    &DEFAULT_2_3_RTL,
    &DEFAULT_FIXED_400,
    &DEFAULT_FIXED_400_RTL,
    &PRESETS_FIFTHS,
    &WITH_GAPS_16,
    &WITH_GAPS_20,
    &WITH_GAPS_200,
    &WITH_STRUTS,
    &CENTER_ALWAYS,
    &CENTER_ON_OVERFLOW,
    &COMBINED_GAPS_STRUTS_CENTER,
    &EMPTY_WORKSPACE_ABOVE,
    &ANIM_LINEAR,
    &ANIM_LINEAR_RTL,
];

// ============================================================================
// CONVERSION TO OPTIONS
// ============================================================================

impl TestConfig {
    /// Convert this test config to `Options` for use in tests.
    pub fn to_options(&self) -> Options {
        let mut options = Options {
            layout: niri_config::Layout {
                gaps: self.layout.gaps,
                struts: niri_config::Struts {
                    left: niri_config::FloatOrInt(self.layout.struts.left),
                    right: niri_config::FloatOrInt(self.layout.struts.right),
                    top: niri_config::FloatOrInt(self.layout.struts.top),
                    bottom: niri_config::FloatOrInt(self.layout.struts.bottom),
                },
                center_focused_column: match self.layout.center_focused_column {
                    CenterFocusedColumn::Never => niri_config::CenterFocusedColumn::Never,
                    CenterFocusedColumn::Always => niri_config::CenterFocusedColumn::Always,
                    CenterFocusedColumn::OnOverflow => niri_config::CenterFocusedColumn::OnOverflow,
                },
                always_center_single_column: self.layout.always_center_single_column,
                default_column_width: self.layout.default_column_width.map(|s| s.to_niri_preset()),
                preset_column_widths: self.layout.preset_column_widths.iter().map(|s| s.to_niri_preset()).collect(),
                preset_window_heights: self.layout.preset_window_heights.iter().map(|s| s.to_niri_preset()).collect(),
                default_column_display: match self.layout.default_column_display {
                    ColumnDisplay::Normal => niri_ipc::ColumnDisplay::Normal,
                    ColumnDisplay::Tabbed => niri_ipc::ColumnDisplay::Tabbed,
                },
                empty_workspace_above_first: self.layout.empty_workspace_above_first,
                right_to_left: self.layout.right_to_left,
                ..Default::default()
            },
            ..Options::default()
        };

        match self.animations {
            AnimationConfig::Off => {
                options.animations.window_open.anim.off = true;
                options.animations.window_close.anim.off = true;
                options.animations.window_resize.anim.off = true;
                options.animations.window_movement.0.off = true;
                options.animations.horizontal_view_movement.0.off = true;
            }
            AnimationConfig::Linear1000ms => {
                const LINEAR: Kind = Kind::Easing(EasingParams {
                    duration_ms: 1000,
                    curve: Curve::Linear,
                });
                options.animations.window_resize.anim.kind = LINEAR;
                options.animations.window_movement.0.kind = LINEAR;
                options.animations.horizontal_view_movement.0.kind = LINEAR;
            }
        }

        options
    }

    /// Generate KDL config file content for this preset.
    pub fn to_kdl(&self) -> String {
        let mut kdl = String::new();

        // Header
        kdl.push_str(&format!("// Test config: {}\n", self.name));
        kdl.push_str(&format!("// {}\n", self.description));
        kdl.push_str("// AUTO-GENERATED - DO NOT EDIT\n");
        kdl.push_str("// Regenerate with: cargo xtask generate-test-configs\n\n");

        // Input
        kdl.push_str("input {\n");
        kdl.push_str("    keyboard {\n");
        kdl.push_str("        xkb {\n");
        kdl.push_str("            layout \"us\"\n");
        kdl.push_str("        }\n");
        kdl.push_str("    }\n");
        kdl.push_str("}\n\n");

        // Output
        kdl.push_str("output \"HEADLESS-1\" {\n");
        kdl.push_str("    mode \"1280x720\"\n");
        kdl.push_str("}\n\n");

        // Layout
        kdl.push_str("layout {\n");
        kdl.push_str(&format!("    gaps {}\n", self.layout.gaps as i32));
        
        if self.layout.right_to_left {
            kdl.push_str("    right-to-left\n");
        }

        let center = match self.layout.center_focused_column {
            CenterFocusedColumn::Never => "never",
            CenterFocusedColumn::Always => "always",
            CenterFocusedColumn::OnOverflow => "on-overflow",
        };
        kdl.push_str(&format!("    center-focused-column \"{}\"\n", center));

        if self.layout.always_center_single_column {
            kdl.push_str("    always-center-single-column\n");
        }

        // Struts
        let s = &self.layout.struts;
        if s.left != 0.0 || s.right != 0.0 || s.top != 0.0 || s.bottom != 0.0 {
            kdl.push_str("    struts {\n");
            if s.left != 0.0 { kdl.push_str(&format!("        left {}\n", s.left as i32)); }
            if s.right != 0.0 { kdl.push_str(&format!("        right {}\n", s.right as i32)); }
            if s.top != 0.0 { kdl.push_str(&format!("        top {}\n", s.top as i32)); }
            if s.bottom != 0.0 { kdl.push_str(&format!("        bottom {}\n", s.bottom as i32)); }
            kdl.push_str("    }\n");
        }

        // Preset column widths
        kdl.push_str("    preset-column-widths {\n");
        for preset in self.layout.preset_column_widths.iter() {
            kdl.push_str(&format!("        {}\n", preset.to_kdl()));
        }
        kdl.push_str("    }\n");

        // Default column width
        if let Some(width) = &self.layout.default_column_width {
            kdl.push_str(&format!("    default-column-width {{ {}; }}\n", width.to_kdl()));
        }

        // Focus ring off (for cleaner testing)
        kdl.push_str("    focus-ring {\n");
        kdl.push_str("        off\n");
        kdl.push_str("    }\n");

        // Border
        kdl.push_str("    border {\n");
        kdl.push_str("        width 4\n");
        kdl.push_str("        active-color \"#ffc87f\"\n");
        kdl.push_str("        inactive-color \"#505050\"\n");
        kdl.push_str("    }\n");

        if self.layout.empty_workspace_above_first {
            kdl.push_str("    empty-workspace-above-first\n");
        }

        kdl.push_str("}\n\n");

        // Prefer no CSD
        kdl.push_str("prefer-no-csd\n\n");

        // Animations
        kdl.push_str("animations {\n");
        match self.animations {
            AnimationConfig::Off => {
                kdl.push_str("    off\n");
            }
            AnimationConfig::Linear1000ms => {
                kdl.push_str("    // Linear 1000ms for testing\n");
                kdl.push_str("    window-resize {\n");
                kdl.push_str("        duration-ms 1000\n");
                kdl.push_str("        curve \"linear\"\n");
                kdl.push_str("    }\n");
                kdl.push_str("    window-movement {\n");
                kdl.push_str("        duration-ms 1000\n");
                kdl.push_str("        curve \"linear\"\n");
                kdl.push_str("    }\n");
                kdl.push_str("    horizontal-view-movement {\n");
                kdl.push_str("        duration-ms 1000\n");
                kdl.push_str("        curve \"linear\"\n");
                kdl.push_str("    }\n");
            }
        }
        kdl.push_str("}\n\n");

        // Binds
        kdl.push_str("binds {\n");
        kdl.push_str("    Mod+T hotkey-overlay-title=\"Open a Terminal\" { spawn \"alacritty\"; }\n");
        kdl.push_str("    Mod+R hotkey-overlay-title=\"Resize Column\" { switch-preset-column-width; }\n");
        kdl.push_str("    Mod+F hotkey-overlay-title=\"Maximize Column\" { maximize-column; }\n");
        kdl.push_str("    Mod+Q hotkey-overlay-title=\"Close Window\" { close-window; }\n");
        kdl.push_str("    Mod+Shift+E hotkey-overlay-title=\"Exit Niri\" { quit; }\n");
        kdl.push_str("    Mod+H { focus-column-left; }\n");
        kdl.push_str("    Mod+L { focus-column-right; }\n");
        kdl.push_str("    Mod+Shift+H { move-column-left; }\n");
        kdl.push_str("    Mod+Shift+L { move-column-right; }\n");
        kdl.push_str("    Mod+Left { focus-column-left; }\n");
        kdl.push_str("    Mod+Right { focus-column-right; }\n");
        kdl.push_str("    Mod+Shift+Left { move-column-left; }\n");
        kdl.push_str("    Mod+Shift+Right { move-column-right; }\n");
        kdl.push_str("    Mod+Up { focus-window-up; }\n");
        kdl.push_str("    Mod+Down { focus-window-down; }\n");
        kdl.push_str("}\n");

        kdl
    }
}

impl PresetSize {
    fn to_niri_preset(&self) -> niri_config::PresetSize {
        match self {
            PresetSize::Proportion(p) => niri_config::PresetSize::Proportion(*p),
            PresetSize::Fixed(f) => niri_config::PresetSize::Fixed(*f),
        }
    }

    fn to_kdl(&self) -> String {
        match self {
            PresetSize::Proportion(p) => format!("proportion {:.5}", p),
            PresetSize::Fixed(f) => format!("fixed {}", f),
        }
    }
}

// ============================================================================
// HELPER FUNCTIONS FOR TESTS
// ============================================================================

/// Create a layout with the given config preset.
#[allow(dead_code)]
pub(super) fn layout_with_config(config: &TestConfig) -> super::Layout<super::TestWindow> {
    let ops = [super::Op::AddOutput(1)];
    super::check_ops_with_options(config.to_options(), ops)
}

/// Get the base config (most commonly used).
#[allow(dead_code)]
pub(super) fn base_options() -> Options {
    BASE.to_options()
}

/// Get the base RTL config.
#[allow(dead_code)]
pub(super) fn base_options_rtl() -> Options {
    BASE_RTL.to_options()
}

// ============================================================================
// CONFIG GENERATION
// ============================================================================

/// Generate all config files to the given directory.
/// 
/// This is called by the `generate_test_configs` test when run with
/// `cargo test generate_test_configs -- --ignored`
pub fn generate_all_configs(output_dir: &std::path::Path) -> std::io::Result<()> {
    use std::fs;
    
    fs::create_dir_all(output_dir)?;
    
    for config in ALL_CONFIGS {
        let path = output_dir.join(format!("{}.kdl", config.name));
        fs::write(&path, config.to_kdl())?;
        println!("Generated: {}", path.display());
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Run with: cargo test -p niri generate_test_configs -- --ignored --nocapture
    /// 
    /// This generates all .kdl config files in the golden_tests/.config directory.
    #[test]
    #[ignore]
    fn generate_test_configs() {
        let config_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/layout/tests/golden_tests/.config");
        
        generate_all_configs(&config_dir).expect("Failed to generate configs");
        println!("\n✅ Generated {} config files to {}", ALL_CONFIGS.len(), config_dir.display());
    }
    
    #[test]
    fn all_configs_have_unique_names() {
        let mut names = std::collections::HashSet::new();
        for config in ALL_CONFIGS {
            assert!(
                names.insert(config.name),
                "Duplicate config name: {}",
                config.name
            );
        }
    }
    
    #[test]
    fn all_configs_produce_valid_options() {
        for config in ALL_CONFIGS {
            let _options = config.to_options();
            // If this doesn't panic, the config is valid
        }
    }
}
