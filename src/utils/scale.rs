//! Default monitor scale calculation.
//!
//! This module follows logic and tests from Mutter:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/gnome-46/src/backends/meta-monitor.c

use smithay::utils::{Physical, Raw, Size};

const MIN_SCALE: i32 = 1;
const MAX_SCALE: i32 = 4;
const MIN_LOGICAL_AREA: i32 = 800 * 480;

const MOBILE_TARGET_DPI: f64 = 135.;
const LARGE_TARGET_DPI: f64 = 110.;
const LARGE_MIN_SIZE_INCHES: f64 = 20.;

/// Calculates the ideal scale for a monitor.
pub fn guess_monitor_scale(size_mm: Size<i32, Raw>, resolution: Size<i32, Physical>) -> f64 {
    if size_mm.w == 0 || size_mm.h == 0 {
        return 1.;
    }

    let diag_inches = f64::from(size_mm.w * size_mm.w + size_mm.h * size_mm.h).sqrt() / 25.4;

    let target_dpi = if diag_inches < LARGE_MIN_SIZE_INCHES {
        MOBILE_TARGET_DPI
    } else {
        LARGE_TARGET_DPI
    };

    let physical_dpi =
        f64::from(resolution.w * resolution.w + resolution.h * resolution.h).sqrt() / diag_inches;
    let perfect_scale = physical_dpi / target_dpi;

    // For integer scaling factors (we currently only do integer), bias the perfect scale down.
    let perfect_scale = perfect_scale - 0.15;

    supported_scales(resolution)
        .map(|scale| (scale, (scale - perfect_scale).abs()))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map_or(1., |(scale, _)| scale)
}

fn supported_scales(resolution: Size<i32, Physical>) -> impl Iterator<Item = f64> {
    (MIN_SCALE..=MAX_SCALE)
        .filter(move |scale| is_valid_for_resolution(resolution, *scale))
        .map(f64::from)
}

fn is_valid_for_resolution(resolution: Size<i32, Physical>, scale: i32) -> bool {
    let logical = resolution.to_logical(scale);
    logical.w * logical.h >= MIN_LOGICAL_AREA
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;

    fn check(size_mm: (i32, i32), resolution: (i32, i32)) -> f64 {
        guess_monitor_scale(Size::from(size_mm), Size::from(resolution))
    }

    #[test]
    fn test_guess_monitor_scale() {
        // Librem 5; not enough logical area when scaled
        snapshot!(check((65, 129), (720, 1440)), "1.0");
        // OnePlus 6
        snapshot!(check((68, 144), (1080, 2280)), "2.0");
        // Google Pixel 6a
        snapshot!(check((64, 142), (1080, 2400)), "2.0");
        // 13" MacBook Retina
        snapshot!(check((286, 179), (2560, 1600)), "2.0");
        // Surface Laptop Studio
        snapshot!(check((303, 202), (2400, 1600)), "1.0");
        // Dell XPS 9320
        snapshot!(check((290, 180), (3840, 2400)), "2.0");
        // Lenovo ThinkPad X1 Yoga Gen 6
        snapshot!(check((300, 190), (3840, 2400)), "2.0");
        // Generic 23" 1080p
        snapshot!(check((509, 286), (1920, 1080)), "1.0");
        // Generic 23" 4K
        snapshot!(check((509, 286), (3840, 2160)), "2.0");
        // Generic 27" 4K
        snapshot!(check((598, 336), (3840, 2160)), "1.0");
        // Generic 32" 4K
        snapshot!(check((708, 398), (3840, 2160)), "1.0");
        // Generic 25" 4K; ideal scale is 1.60, should round to 1.5 and 1.0
        snapshot!(check((554, 312), (3840, 2160)), "1.0");
        // Generic 23.5" 4K; ideal scale is 1.70, should round to 1.75 and 2.0
        snapshot!(check((522, 294), (3840, 2160)), "2.0");
        // Lenovo Legion 7 Gen 7 AMD 16"
        snapshot!(check((340, 210), (2560, 1600)), "1.0");
        // Acer Nitro XV320QU LV 31.5"
        snapshot!(check((700, 390), (2560, 1440)), "1.0");
        // Surface Pro 6
        snapshot!(check((260, 170), (2736, 1824)), "2.0");
    }

    #[test]
    fn guess_monitor_scale_unknown_size() {
        assert_eq!(check((0, 0), (1920, 1080)), 1.);
    }
}
