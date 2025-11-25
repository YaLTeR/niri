use niri_config::{PresetSize, Struts};
use smithay::utils::{Logical, Rectangle, Size};

use super::super::workspace::ResolvedSize;
use super::super::Options;

/// Compute the view offset needed to bring a column into view.
///
/// The view offset is relative to the column position: view_pos = column_x + view_offset.
///
/// In RTL mode, columns are positioned from the right edge of the screen, so the alignment
/// preferences are reversed (prefer right alignment in RTL, left alignment in LTR).
///
/// Returns the view offset (negative values scroll the view left).
pub fn compute_new_view_offset(
    cur_view_x: f64,
    view_width: f64,
    new_col_x: f64,
    new_col_width: f64,
    gaps: f64,
    is_rtl: bool,
) -> f64 {
    // If the column is wider than the view, always left-align it (offset = 0).
    if view_width <= new_col_width {
        return 0.;
    }

    // Compute the padding in case it needs to be smaller due to large tile width.
    let padding = ((view_width - new_col_width) / 2.).clamp(0., gaps);

    // Compute the desired column bounds with padding.
    let col_left_with_padding = new_col_x - padding;
    let col_right_with_padding = new_col_x + new_col_width + padding;

    // If the column is already fully visible, keep the current offset.
    if cur_view_x <= col_left_with_padding && col_right_with_padding <= cur_view_x + view_width {
        // In RTL, check if the column is within the screen bounds [0, view_width].
        // If so, prefer anchoring the view at x=0 to show the empty space on the left.
        if is_rtl && col_left_with_padding >= 0. && col_right_with_padding <= view_width {
            // Position the view at x=0, so view_offset = -new_col_x
            return -new_col_x;
        }
        return -(new_col_x - cur_view_x);
    }

    // Prefer the alignment that results in less motion from the current position.
    let dist_to_left = (cur_view_x - col_left_with_padding).abs();
    let dist_to_right = ((cur_view_x + view_width) - col_right_with_padding).abs();
    
    // view_offset = view_pos - column_x
    // To show column at left edge with padding: view_pos = col_left_with_padding
    //   → view_offset = col_left_with_padding - new_col_x = -padding
    // To show column at right edge with padding: view_pos = col_right_with_padding - view_width
    //   → view_offset = (col_right_with_padding - view_width) - new_col_x
    //                 = new_col_x + new_col_width + padding - view_width - new_col_x
    //                 = new_col_width + padding - view_width
    
    // In RTL, columns grow leftward (negative x direction).
    // When a column overflows to the left (col_left < 0), show it at the LEFT edge.
    // When a column overflows to the right (col_right > view_width), show it at the RIGHT edge.
    // This mirrors LTR behavior where columns grow rightward.
    if is_rtl {
        // Check if column is off-screen to the left
        if col_left_with_padding < 0. {
            // Show at left edge
            -padding
        } else if col_right_with_padding > view_width {
            // Show at right edge
            new_col_width + padding - view_width
        } else if dist_to_left <= dist_to_right {
            -padding
        } else {
            new_col_width + padding - view_width
        }
    } else {
        if dist_to_left <= dist_to_right {
            -padding
        } else {
            new_col_width + padding - view_width
        }
    }
}

pub fn compute_working_area(
    parent_area: Rectangle<f64, Logical>,
    scale: f64,
    struts: Struts,
) -> Rectangle<f64, Logical> {
    let mut working_area = parent_area;

    // Add struts.
    working_area.size.w = f64::max(0., working_area.size.w - struts.left.0 - struts.right.0);
    working_area.loc.x += struts.left.0;

    working_area.size.h = f64::max(0., working_area.size.h - struts.top.0 - struts.bottom.0);
    working_area.loc.y += struts.top.0;

    // Round location to start at a physical pixel.
    let loc = working_area
        .loc
        .to_physical_precise_ceil(scale)
        .to_logical(scale);

    let mut size_diff = (loc - working_area.loc).to_size();
    size_diff.w = f64::min(working_area.size.w, size_diff.w);
    size_diff.h = f64::min(working_area.size.h, size_diff.h);

    working_area.size -= size_diff;
    working_area.loc = loc;

    working_area
}

pub fn compute_toplevel_bounds(
    border_config: niri_config::Border,
    working_area_size: Size<f64, Logical>,
    extra_size: Size<f64, Logical>,
    gaps: f64,
) -> Size<i32, Logical> {
    let mut border = 0.;
    if !border_config.off {
        border = border_config.width * 2.;
    }

    Size::from((
        f64::max(working_area_size.w - gaps * 2. - extra_size.w - border, 1.),
        f64::max(working_area_size.h - gaps * 2. - extra_size.h - border, 1.),
    ))
    .to_i32_floor()
}

pub fn resolve_preset_size(
    preset: PresetSize,
    options: &Options,
    view_size: f64,
    extra_size: f64,
) -> ResolvedSize {
    match preset {
        PresetSize::Proportion(proportion) => ResolvedSize::Tile(
            (view_size - options.layout.gaps) * proportion - options.layout.gaps - extra_size,
        ),
        PresetSize::Fixed(width) => ResolvedSize::Window(f64::from(width)),
    }
}

#[cfg(test)]
mod tests {
    use niri_config::FloatOrInt;

    use super::*;
    use crate::utils::round_logical_in_physical;

    #[test]
    fn working_area_starts_at_physical_pixel() {
        let struts = Struts {
            left: FloatOrInt(0.5),
            right: FloatOrInt(1.),
            top: FloatOrInt(0.75),
            bottom: FloatOrInt(1.),
        };

        let parent_area = Rectangle::from_size(Size::from((1280., 720.)));
        let area = compute_working_area(parent_area, 1., struts);

        assert_eq!(round_logical_in_physical(1., area.loc.x), area.loc.x);
        assert_eq!(round_logical_in_physical(1., area.loc.y), area.loc.y);
    }

    #[test]
    fn large_fractional_strut() {
        let struts = Struts {
            left: FloatOrInt(0.),
            right: FloatOrInt(0.),
            top: FloatOrInt(50000.5),
            bottom: FloatOrInt(0.),
        };

        let parent_area = Rectangle::from_size(Size::from((1280., 720.)));
        compute_working_area(parent_area, 1., struts);
    }
}
