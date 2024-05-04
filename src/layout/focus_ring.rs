use std::cmp::{max, min};
use std::iter::zip;

use arrayvec::ArrayVec;
use niri_config::{CornerRadius, Gradient, GradientRelativeTo};
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use crate::niri_render_elements;
use crate::render_helpers::border::BorderRenderElement;
use crate::render_helpers::renderer::NiriRenderer;

#[derive(Debug)]
pub struct FocusRing {
    buffers: [SolidColorBuffer; 8],
    locations: [Point<i32, Logical>; 8],
    sizes: [Size<i32, Logical>; 8],
    borders: [BorderRenderElement; 8],
    full_size: Size<i32, Logical>,
    is_border: bool,
    use_border_shader: bool,
    config: niri_config::FocusRing,
}

niri_render_elements! {
    FocusRingRenderElement => {
        SolidColor = SolidColorRenderElement,
        Gradient = BorderRenderElement,
    }
}

impl FocusRing {
    pub fn new(config: niri_config::FocusRing) -> Self {
        Self {
            buffers: Default::default(),
            locations: Default::default(),
            sizes: Default::default(),
            borders: Default::default(),
            full_size: Default::default(),
            is_border: false,
            use_border_shader: false,
            config,
        }
    }

    pub fn update_config(&mut self, config: niri_config::FocusRing) {
        self.config = config;
    }

    pub fn update_shaders(&mut self) {
        for elem in &mut self.borders {
            elem.damage_all();
        }
    }

    pub fn update_render_elements(
        &mut self,
        win_size: Size<i32, Logical>,
        is_active: bool,
        is_border: bool,
        view_rect: Rectangle<i32, Logical>,
        radius: CornerRadius,
    ) {
        let width = i32::from(self.config.width);
        self.full_size = win_size + Size::from((width * 2, width * 2));

        let color = if is_active {
            self.config.active_color
        } else {
            self.config.inactive_color
        };

        for buf in &mut self.buffers {
            buf.set_color(color.into());
        }

        let radius = radius.fit_to(self.full_size.w as f32, self.full_size.h as f32);

        let gradient = if is_active {
            self.config.active_gradient
        } else {
            self.config.inactive_gradient
        };

        self.use_border_shader = radius != CornerRadius::default() || gradient.is_some();

        // Set the defaults for solid color + rounded corners.
        let gradient = gradient.unwrap_or(Gradient {
            from: color,
            to: color,
            angle: 0,
            relative_to: GradientRelativeTo::Window,
        });

        let full_rect = Rectangle::from_loc_and_size((-width, -width), self.full_size);
        let gradient_area = match gradient.relative_to {
            GradientRelativeTo::Window => full_rect,
            GradientRelativeTo::WorkspaceView => view_rect,
        };

        let rounded_corner_border_width = if self.is_border {
            // HACK: increase the border width used for the inner rounded corners a tiny bit to
            // reduce background bleed.
            width as f32 + 0.5
        } else {
            0.
        };

        if is_border {
            let top_left = max(width, radius.top_left.ceil() as i32);
            let top_right = min(
                self.full_size.w - top_left,
                max(width, radius.top_right.ceil() as i32),
            );
            let bottom_left = min(
                self.full_size.h - top_left,
                max(width, radius.bottom_left.ceil() as i32),
            );
            let bottom_right = min(
                self.full_size.h - top_right,
                min(
                    self.full_size.w - bottom_left,
                    max(width, radius.bottom_right.ceil() as i32),
                ),
            );

            // Top edge.
            self.sizes[0] = Size::from((win_size.w + width * 2 - top_left - top_right, width));
            self.locations[0] = Point::from((-width + top_left, -width));

            // Bottom edge.
            self.sizes[1] =
                Size::from((win_size.w + width * 2 - bottom_left - bottom_right, width));
            self.locations[1] = Point::from((-width + bottom_left, win_size.h));

            // Left edge.
            self.sizes[2] = Size::from((width, win_size.h + width * 2 - top_left - bottom_left));
            self.locations[2] = Point::from((-width, -width + top_left));

            // Right edge.
            self.sizes[3] = Size::from((width, win_size.h + width * 2 - top_right - bottom_right));
            self.locations[3] = Point::from((win_size.w, -width + top_right));

            // Top-left corner.
            self.sizes[4] = Size::from((top_left, top_left));
            self.locations[4] = Point::from((-width, -width));

            // Top-right corner.
            self.sizes[5] = Size::from((top_right, top_right));
            self.locations[5] = Point::from((win_size.w + width - top_right, -width));

            // Bottom-right corner.
            self.sizes[6] = Size::from((bottom_right, bottom_right));
            self.locations[6] = Point::from((
                win_size.w + width - bottom_right,
                win_size.h + width - bottom_right,
            ));

            // Bottom-left corner.
            self.sizes[7] = Size::from((bottom_left, bottom_left));
            self.locations[7] = Point::from((-width, win_size.h + width - bottom_left));

            for (buf, size) in zip(&mut self.buffers, self.sizes) {
                buf.resize(size);
            }

            for (border, (loc, size)) in zip(&mut self.borders, zip(self.locations, self.sizes)) {
                border.update(
                    size,
                    Rectangle::from_loc_and_size(gradient_area.loc - loc, gradient_area.size),
                    gradient.from.into(),
                    gradient.to.into(),
                    ((gradient.angle as f32) - 90.).to_radians(),
                    Rectangle::from_loc_and_size(full_rect.loc - loc, full_rect.size),
                    rounded_corner_border_width,
                    radius,
                );
            }
        } else {
            self.sizes[0] = self.full_size;
            self.buffers[0].resize(self.sizes[0]);
            self.locations[0] = Point::from((-width, -width));

            self.borders[0].update(
                self.sizes[0],
                Rectangle::from_loc_and_size(
                    gradient_area.loc - self.locations[0],
                    gradient_area.size,
                ),
                gradient.from.into(),
                gradient.to.into(),
                ((gradient.angle as f32) - 90.).to_radians(),
                Rectangle::from_loc_and_size(full_rect.loc - self.locations[0], full_rect.size),
                rounded_corner_border_width,
                radius,
            );
        }

        self.is_border = is_border;
    }

    pub fn render(
        &self,
        renderer: &mut impl NiriRenderer,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
    ) -> impl Iterator<Item = FocusRingRenderElement> {
        let mut rv = ArrayVec::<_, 8>::new();

        if self.config.off {
            return rv.into_iter();
        }

        let border_width = -self.locations[0].y;

        // If drawing as a border with width = 0, then there's nothing to draw.
        if self.is_border && border_width == 0 {
            return rv.into_iter();
        }

        let has_border_shader = BorderRenderElement::has_shader(renderer);

        let mut push = |buffer, border: &BorderRenderElement, location: Point<i32, Logical>| {
            let elem = if self.use_border_shader && has_border_shader {
                border.clone().with_location(location).into()
            } else {
                SolidColorRenderElement::from_buffer(
                    buffer,
                    location.to_physical_precise_round(scale),
                    scale,
                    1.,
                    Kind::Unspecified,
                )
                .into()
            };
            rv.push(elem);
        };

        if self.is_border {
            for ((buf, border), loc) in zip(zip(&self.buffers, &self.borders), self.locations) {
                push(buf, border, location + loc);
            }
        } else {
            push(
                &self.buffers[0],
                &self.borders[0],
                location + self.locations[0],
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
