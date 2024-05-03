use std::f32::consts::{FRAC_PI_4, PI};
use std::sync::atomic::Ordering;
use std::time::Duration;

use niri::animation::ANIMATION_SLOWDOWN;
use niri::layout::focus_ring::FocusRing;
use niri::render_helpers::border::BorderRenderElement;
use niri_config::{Color, CornerRadius};
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};

use super::TestCase;

pub struct GradientArea {
    progress: f32,
    border: FocusRing,
    prev_time: Duration,
}

impl GradientArea {
    pub fn new(_size: Size<i32, Logical>) -> Self {
        let mut border = FocusRing::new(niri_config::FocusRing {
            off: false,
            width: 1,
            active_color: Color::new(255, 255, 255, 128),
            inactive_color: Color::default(),
            active_gradient: None,
            inactive_gradient: None,
        });
        border.set_active(true);

        Self {
            progress: 0.,
            border,
            prev_time: Duration::ZERO,
        }
    }
}

impl TestCase for GradientArea {
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

        self.progress += delta.as_secs_f32() * PI;

        if self.progress >= PI * 2. {
            self.progress -= PI * 2.
        }
    }

    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
    ) -> Vec<Box<dyn RenderElement<GlesRenderer>>> {
        let mut rv = Vec::new();

        let f = (self.progress.sin() + 1.) / 2.;

        let (a, b) = (size.w / 4, size.h / 4);
        let rect_size = (size.w - a * 2, size.h - b * 2);
        let area = Rectangle::from_loc_and_size((a, b), rect_size);

        let g_size = Size::from((
            (size.w as f32 / 8. + size.w as f32 / 8. * 7. * f).round() as i32,
            (size.h as f32 / 8. + size.h as f32 / 8. * 7. * f).round() as i32,
        ));
        let g_loc = ((size.w - g_size.w) / 2, (size.h - g_size.h) / 2);
        let g_area = Rectangle::from_loc_and_size(g_loc, g_size);

        self.border.update(g_size, true, CornerRadius::default());
        rv.extend(
            self.border
                .render(
                    renderer,
                    Point::from(g_loc),
                    Scale::from(1.),
                    size.to_logical(1),
                )
                .map(|elem| Box::new(elem) as _),
        );

        rv.extend(
            BorderRenderElement::shader(renderer)
                .map(|shader| {
                    BorderRenderElement::new(
                        shader,
                        Scale::from(1.),
                        area,
                        g_area,
                        [1., 0., 0., 1.],
                        [0., 1., 0., 1.],
                        FRAC_PI_4,
                        area,
                        0.,
                        CornerRadius::default(),
                    )
                })
                .into_iter()
                .map(|elem| Box::new(elem) as _),
        );

        rv
    }
}
