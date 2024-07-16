use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::atomic::Ordering;
use std::time::Duration;

use niri::animation::ANIMATION_SLOWDOWN;
use niri::render_helpers::border::BorderRenderElement;
use niri_config::{CornerRadius, GradientInterpolation};
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Rectangle, Size};

use super::TestCase;

pub struct GradientAngle {
    angle: f32,
    prev_time: Duration,
}

impl GradientAngle {
    pub fn new(_size: Size<i32, Logical>) -> Self {
        Self {
            angle: 0.,
            prev_time: Duration::ZERO,
        }
    }
}

impl TestCase for GradientAngle {
    fn are_animations_ongoing(&self) -> bool {
        true
    }

    fn advance_animations(&mut self, current_time: Duration) {
        let mut delta = if self.prev_time.is_zero() {
            Duration::ZERO
        } else {
            current_time.saturating_sub(self.prev_time)
        };
        self.prev_time = current_time;

        let slowdown = ANIMATION_SLOWDOWN.load(Ordering::SeqCst);
        if slowdown == 0. {
            delta = Duration::ZERO
        } else {
            delta = delta.div_f64(slowdown);
        }

        self.angle += delta.as_secs_f32() * PI;

        if self.angle >= PI * 2. {
            self.angle -= PI * 2.
        }
    }

    fn render(
        &mut self,
        _renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
    ) -> Vec<Box<dyn RenderElement<GlesRenderer>>> {
        let (a, b) = (size.w / 4, size.h / 4);
        let size = (size.w - a * 2, size.h - b * 2);
        let area = Rectangle::from_loc_and_size((a, b), size).to_f64();

        [BorderRenderElement::new(
            area.size,
            Rectangle::from_loc_and_size((0., 0.), area.size),
            GradientInterpolation::default(),
            [1., 0., 0., 1.],
            [0., 1., 0., 1.],
            self.angle - FRAC_PI_2,
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
