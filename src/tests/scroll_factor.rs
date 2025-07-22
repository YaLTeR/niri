use niri_config::{FloatOrInt, Mouse, Touchpad};

// Reproduce the macro from input/mod.rs for testing
macro_rules! extract_scroll_factors {
    ($cfg:expr) => {{
        let h_factor = $cfg
            .scroll_factor_horizontal
            .map(|x| x.0)
            .or_else(|| $cfg.scroll_factor.map(|x| x.0))
            .unwrap_or(1.);
        let v_factor = $cfg
            .scroll_factor_vertical
            .map(|y| y.0)
            .or_else(|| $cfg.scroll_factor.map(|y| y.0))
            .unwrap_or(1.);
        (h_factor, v_factor)
    }};
}

fn create_mouse_config(
    scroll_factor: Option<f64>,
    scroll_factor_horizontal: Option<f64>,
    scroll_factor_vertical: Option<f64>,
) -> Mouse {
    Mouse {
        off: false,
        natural_scroll: false,
        accel_speed: Default::default(),
        accel_profile: None,
        scroll_method: None,
        scroll_button: None,
        scroll_button_lock: false,
        left_handed: false,
        middle_emulation: false,
        scroll_factor: scroll_factor.map(FloatOrInt),
        scroll_factor_horizontal: scroll_factor_horizontal.map(FloatOrInt),
        scroll_factor_vertical: scroll_factor_vertical.map(FloatOrInt),
    }
}

fn create_touchpad_config(
    scroll_factor: Option<f64>,
    scroll_factor_horizontal: Option<f64>,
    scroll_factor_vertical: Option<f64>,
) -> Touchpad {
    Touchpad {
        off: false,
        tap: false,
        dwt: false,
        dwtp: false,
        drag: None,
        drag_lock: false,
        natural_scroll: false,
        click_method: None,
        accel_speed: Default::default(),
        accel_profile: None,
        scroll_method: None,
        scroll_button: None,
        scroll_button_lock: false,
        tap_button_map: None,
        left_handed: false,
        disabled_on_external_mouse: false,
        middle_emulation: false,
        scroll_factor: scroll_factor.map(FloatOrInt),
        scroll_factor_horizontal: scroll_factor_horizontal.map(FloatOrInt),
        scroll_factor_vertical: scroll_factor_vertical.map(FloatOrInt),
    }
}

#[test]
fn test_mouse_scroll_factor_defaults() {
    let config = create_mouse_config(None, None, None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 1.0);
    assert_eq!(v_factor, 1.0);
}

#[test]
fn test_mouse_scroll_factor_general_only() {
    let config = create_mouse_config(Some(2.0), None, None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 2.0);
    assert_eq!(v_factor, 2.0);
}

#[test]
fn test_mouse_scroll_factor_horizontal_override() {
    let config = create_mouse_config(Some(2.0), Some(3.0), None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 3.0); // horizontal-specific overrides general
    assert_eq!(v_factor, 2.0); // vertical falls back to general
}

#[test]
fn test_mouse_scroll_factor_vertical_override() {
    let config = create_mouse_config(Some(2.0), None, Some(4.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 2.0); // horizontal falls back to general
    assert_eq!(v_factor, 4.0); // vertical-specific overrides general
}

#[test]
fn test_mouse_scroll_factor_both_override() {
    let config = create_mouse_config(Some(2.0), Some(3.0), Some(4.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 3.0); // horizontal-specific overrides general
    assert_eq!(v_factor, 4.0); // vertical-specific overrides general
}

#[test]
fn test_mouse_scroll_factor_specific_without_general() {
    let config = create_mouse_config(None, Some(3.0), Some(4.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 3.0); // horizontal-specific used
    assert_eq!(v_factor, 4.0); // vertical-specific used
}

#[test]
fn test_mouse_scroll_factor_partial_specific() {
    let config = create_mouse_config(None, Some(3.0), None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 3.0); // horizontal-specific used
    assert_eq!(v_factor, 1.0); // vertical falls back to default
}

#[test]
fn test_touchpad_scroll_factor_defaults() {
    let config = create_touchpad_config(None, None, None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 1.0);
    assert_eq!(v_factor, 1.0);
}

#[test]
fn test_touchpad_scroll_factor_general_only() {
    let config = create_touchpad_config(Some(2.5), None, None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 2.5);
    assert_eq!(v_factor, 2.5);
}

#[test]
fn test_touchpad_scroll_factor_horizontal_override() {
    let config = create_touchpad_config(Some(2.5), Some(1.5), None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 1.5); // horizontal-specific overrides general
    assert_eq!(v_factor, 2.5); // vertical falls back to general
}

#[test]
fn test_touchpad_scroll_factor_vertical_override() {
    let config = create_touchpad_config(Some(2.5), None, Some(0.8));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 2.5); // horizontal falls back to general
    assert_eq!(v_factor, 0.8); // vertical-specific overrides general
}

#[test]
fn test_touchpad_scroll_factor_both_override() {
    let config = create_touchpad_config(Some(2.5), Some(1.5), Some(0.8));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 1.5); // horizontal-specific overrides general
    assert_eq!(v_factor, 0.8); // vertical-specific overrides general
}

#[test]
fn test_negative_values_for_inversion() {
    // Test negative values (used for direction inversion)
    let config = create_mouse_config(Some(1.0), Some(-2.0), Some(-1.5));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, -2.0); // negative horizontal factor
    assert_eq!(v_factor, -1.5); // negative vertical factor
}

#[test]
fn test_zero_values() {
    // Test zero values (effectively disables scrolling)
    let config = create_touchpad_config(None, Some(0.0), Some(0.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 0.0);
    assert_eq!(v_factor, 0.0);
}

#[test]
fn test_fractional_values() {
    // Test fractional values for fine-tuning
    let config = create_mouse_config(None, Some(0.25), Some(0.75));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 0.25);
    assert_eq!(v_factor, 0.75);
}

#[test]
fn test_config_parsing_integration() {
    // Test actual config parsing to ensure our logic works with real parsed configs

    // Config with only general scroll-factor
    let config_text = r#"
        input {
            mouse {
                scroll-factor 2.5
            }
        }
    "#;
    let config = niri_config::Config::parse("test", config_text).unwrap();
    let (h_factor, v_factor) = extract_scroll_factors!(&config.input.mouse);
    assert_eq!(h_factor, 2.5);
    assert_eq!(v_factor, 2.5);

    // Config with specific factors overriding general
    let config_text = r#"
        input {
            mouse {
                scroll-factor 2.0
                scroll-factor-horizontal 3.0
                scroll-factor-vertical 4.0
            }
            touchpad {
                scroll-factor 1.5
                scroll-factor-horizontal 2.5
            }
        }
    "#;
    let config = niri_config::Config::parse("test", config_text).unwrap();

    // Mouse: both specific factors should override general
    let (h_factor, v_factor) = extract_scroll_factors!(&config.input.mouse);
    assert_eq!(h_factor, 3.0); // horizontal-specific overrides general
    assert_eq!(v_factor, 4.0); // vertical-specific overrides general

    // Touchpad: horizontal-specific overrides general, vertical falls back to general
    let (h_factor, v_factor) = extract_scroll_factors!(&config.input.touchpad);
    assert_eq!(h_factor, 2.5); // horizontal-specific overrides general
    assert_eq!(v_factor, 1.5); // vertical falls back to general

    // Config with negative values for direction inversion
    let config_text = r#"
        input {
            touchpad {
                scroll-factor-horizontal -1.5
                scroll-factor-vertical -2.0
            }
        }
    "#;
    let config = niri_config::Config::parse("test", config_text).unwrap();
    let (h_factor, v_factor) = extract_scroll_factors!(&config.input.touchpad);
    assert_eq!(h_factor, -1.5); // negative horizontal
    assert_eq!(v_factor, -2.0); // negative vertical

    // Config with mixed scenario: only horizontal-specific, no general
    let config_text = r#"
        input {
            mouse {
                scroll-factor-horizontal 0.8
            }
        }
    "#;
    let config = niri_config::Config::parse("test", config_text).unwrap();
    let (h_factor, v_factor) = extract_scroll_factors!(&config.input.mouse);
    assert_eq!(h_factor, 0.8); // uses horizontal-specific
    assert_eq!(v_factor, 1.0); // defaults to 1.0 (no general, no vertical-specific)
}

#[test]
fn test_priority_order_demonstration() {
    // This test specifically demonstrates the priority order:
    // 1. scroll-factor-horizontal/vertical (if present)
    // 2. scroll-factor (if present and specific not set)
    // 3. 1.0 (default if neither present)

    // Case 1: All three specified - specific should win
    let config = create_mouse_config(Some(10.0), Some(20.0), Some(30.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 20.0); // horizontal-specific wins over general
    assert_eq!(v_factor, 30.0); // vertical-specific wins over general

    // Case 2: Only general specified - should be used for both
    let config = create_touchpad_config(Some(5.0), None, None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 5.0); // falls back to general
    assert_eq!(v_factor, 5.0); // falls back to general

    // Case 3: Only one specific - other should fall back to general or default
    let config = create_mouse_config(Some(7.0), Some(8.0), None);
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 8.0); // uses horizontal-specific
    assert_eq!(v_factor, 7.0); // falls back to general

    // Case 4: Mixed with defaults
    let config = create_touchpad_config(None, None, Some(9.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 1.0); // defaults to 1.0 (no general, no horizontal-specific)
    assert_eq!(v_factor, 9.0); // uses vertical-specific
}

#[test]
fn test_config_parsing_edge_cases() {
    // Test with very small values
    let config = create_mouse_config(Some(0.01), Some(0.001), Some(0.1));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 0.001);
    assert_eq!(v_factor, 0.1);

    // Test with large values
    let config = create_touchpad_config(Some(100.0), Some(99.0), Some(101.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, 99.0);
    assert_eq!(v_factor, 101.0);

    // Test mixed positive/negative
    let config = create_mouse_config(Some(1.0), Some(-1.0), Some(2.0));
    let (h_factor, v_factor) = extract_scroll_factors!(&config);
    assert_eq!(h_factor, -1.0); // negative horizontal
    assert_eq!(v_factor, 2.0); // positive vertical
}
