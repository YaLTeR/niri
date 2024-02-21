use std::iter::zip;

use arrayvec::ArrayVec;
use niri_config::{self, GradientRelativeTo};
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use crate::niri_render_elements;
use crate::render_helpers::gradient::GradientRenderElement;
use crate::render_helpers::renderer::NiriRenderer;

#[derive(Debug)]
pub struct FocusRing {
    buffers: [SolidColorBuffer; 4],
    locations: [Point<i32, Logical>; 4],
    sizes: [Size<i32, Logical>; 4],
    full_size: Size<i32, Logical>,
    is_active: bool,
    is_border: bool,
    config: niri_config::FocusRing,
}

niri_render_elements! {
    FocusRingRenderElement => {
        SolidColor = SolidColorRenderElement,
        Gradient = GradientRenderElement,
    }
}

impl FocusRing {
    pub fn new(config: niri_config::FocusRing) -> Self {
        Self {
            buffers: Default::default(),
            locations: Default::default(),
            sizes: Default::default(),
            full_size: Default::default(),
            is_active: false,
            is_border: false,
            config,
        }
    }

    pub fn update_config(&mut self, config: niri_config::FocusRing) {
        self.config = config;
    }

    pub fn update(&mut self, win_size: Size<i32, Logical>, is_border: bool) {
        let width = i32::from(self.config.width);
        self.full_size = win_size + Size::from((width * 2, width * 2));

        if is_border {
            self.sizes[0] = Size::from((win_size.w + width * 2, width));
            self.sizes[1] = Size::from((win_size.w + width * 2, width));
            self.sizes[2] = Size::from((width, win_size.h));
            self.sizes[3] = Size::from((width, win_size.h));

            for (buf, size) in zip(&mut self.buffers, self.sizes) {
                buf.resize(size);
            }

            self.locations[0] = Point::from((-width, -width));
            self.locations[1] = Point::from((-width, win_size.h));
            self.locations[2] = Point::from((-width, 0));
            self.locations[3] = Point::from((win_size.w, 0));
        } else {
            self.sizes[0] = self.full_size;
            self.buffers[0].resize(self.sizes[0]);
            self.locations[0] = Point::from((-width, -width));
        }

        self.is_border = is_border;
    }

    pub fn set_active(&mut self, is_active: bool) {
        let color = if is_active {
            self.config.active_color.into()
        } else {
            self.config.inactive_color.into()
        };

        for buf in &mut self.buffers {
            buf.set_color(color);
        }

        self.is_active = is_active;
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        view_size: Size<i32, Logical>,
    ) -> impl Iterator<Item = FocusRingRenderElement> {
        let mut rv = ArrayVec::<_, 4>::new();

        if self.config.off {
            return rv.into_iter();
        }

        let gradient = if self.is_active {
            self.config.active_gradient
        } else {
            self.config.inactive_gradient
        };

        let full_rect = Rectangle::from_loc_and_size(location + self.locations[0], self.full_size);
        let view_rect = Rectangle::from_loc_and_size((0, 0), view_size);

        let mut push = |buffer, location: Point<i32, Logical>, size: Size<i32, Logical>| {
            let elem = gradient.and_then(|gradient| {
                let gradient_area = match gradient.relative_to {
                    GradientRelativeTo::Window => full_rect,
                    GradientRelativeTo::WorkspaceView => view_rect,
                };
                GradientRenderElement::new(
                    renderer,
                    scale,
                    Rectangle::from_loc_and_size(location, size),
                    gradient_area,
                    gradient.from.into(),
                    gradient.to.into(),
                    ((gradient.angle as f32) - 90.).to_radians(),
                )
                .map(Into::into)
            });

            let elem = elem.unwrap_or_else(|| {
                SolidColorRenderElement::from_buffer(
                    buffer,
                    location.to_physical_precise_round(scale),
                    scale,
                    1.,
                    Kind::Unspecified,
                )
                .into()
            });
            rv.push(elem);
        };

        if self.is_border {
            for (buf, (loc, size)) in zip(&self.buffers, zip(self.locations, self.sizes)) {
                push(buf, location + loc, size);
            }
        } else {
            push(
                &self.buffers[0],
                location + self.locations[0],
                self.sizes[0],
            );
        }

        rv.into_iter()
    }

    pub fn width(&self) -> i32 {
        self.config.width.into()
    }

    pub fn is_off(&self) -> bool {
        self.config.off
    }
}
