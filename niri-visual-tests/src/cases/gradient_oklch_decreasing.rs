use niri::render_helpers::border::BorderRenderElement;
use niri_config::{CornerRadius, GradientColorSpace, GradientInterpolation, HueInterpolation};
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Rectangle, Size};

use super::TestCase;

pub struct GradientOklchDecreasing {
    gradient_format: GradientInterpolation,
}

impl GradientOklchDecreasing {
    pub fn new(_size: Size<i32, Logical>) -> Self {
        Self {
            gradient_format: GradientInterpolation {
                color_space: GradientColorSpace::Oklch,
                hue_interpol: HueInterpolation::Decreasing,
            },
        }
    }
}

impl TestCase for GradientOklchDecreasing {
    fn render(
        &mut self,
        _renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
    ) -> Vec<Box<dyn RenderElement<GlesRenderer>>> {
        let (a, b) = (size.w / 6, size.h / 3);
        let size = (size.w - a * 2, size.h - b * 2);
        let area = Rectangle::from_loc_and_size((a, b), size).to_f64();

        [BorderRenderElement::new(
            area.size,
            Rectangle::from_loc_and_size((0., 0.), area.size),
            self.gradient_format,
            [1., 0., 0., 1.],
            [0., 1., 0., 1.],
            0.,
            Rectangle::from_loc_and_size((0., 0.), area.size),
            0.,
            CornerRadius::default(),
            1.,
        )
        .with_location(area.loc)]
        .into_iter()
        .map(|elem| Box::new(elem) as _)
        .collect()
    }
}
