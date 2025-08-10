use std::iter::zip;

use arrayvec::ArrayVec;
use niri_config::{CornerRadius, Gradient, GradientRelativeTo};
use smithay::backend::renderer::element::{Element as _, Kind};
use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::niri_render_elements;
use crate::render_helpers::border::BorderRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};

#[derive(Debug)]
pub struct FocusRing {
    buffers: [SolidColorBuffer; 8],
    locations: [Point<f64, Logical>; 8],
    sizes: [Size<f64, Logical>; 8],
    borders: [BorderRenderElement; 8],
    full_size: Size<f64, Logical>,
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

    #[allow(clippy::too_many_arguments)]
    pub fn update_render_elements(
        &mut self,
        win_size: Size<f64, Logical>,
        is_active: bool,
        is_border: bool,
        view_rect: Rectangle<f64, Logical>,
        radius: CornerRadius,
        scale: f64,
        alpha: f32,
    ) {
        let width = self.config.width.0;
        // Update full_size
        self.full_size = win_size + Size::from((width, width)).upscale(2.);

        // Set color based on activity
        let color = if is_active {
            self.config.active_color
        } else {
            self.config.inactive_color
        };

        // Set buffer colors
        for buf in &mut self.buffers {
            buf.set_color(color.to_array_premul());
        }

        // Fit radius
        let radius = radius.fit_to(self.full_size.w as f32, self.full_size.h as f32);

        // Gradient setup
        let gradient = if is_active {
            self.config.active_gradient
        } else {
            self.config.inactive_gradient
        };

        // Use border shader if rounded corners or gradient
        self.use_border_shader = radius != CornerRadius::default() || gradient.is_some();

        let gradient = gradient.unwrap_or_else(|| Gradient::from(color));

        // Define rectangles
        let full_rect = Rectangle::new(Point::from((-width, -width)), self.full_size);
        let gradient_area = match gradient.relative_to {
            GradientRelativeTo::Window => full_rect,
            GradientRelativeTo::WorkspaceView => view_rect,
        };

        let rounded_corner_border_width = if self.is_border {
            width as f32 + 0.5
        } else {
            0.
        };

        let ceil = |logical: f64| (logical * scale).ceil() / scale;

        if is_border {
            // -- Assign each segment with correct index --
            // Top edge
            self.sizes[0] = Size::from((win_size.w + width * 2. - f64::from(radius.top_left), width));
            self.locations[0] = Point::from((-width + f64::from(radius.top_left), -width));

            // Bottom edge
            self.sizes[1] = Size::from((win_size.w + width * 2. - f64::from(radius.bottom_left), width));
            self.locations[1] = Point::from((-width + f64::from(radius.bottom_left), win_size.h));

            // Left edge
            self.sizes[2] = Size::from((width, win_size.h + width * 2. - f64::from(radius.top_left) - f64::from(radius.bottom_left)));
            self.locations[2] = Point::from((-width, -width + f64::from(radius.top_left)));

            // Right edge
            self.sizes[3] = Size::from((width, win_size.h + width * 2. - f64::from(radius.top_right) - f64::from(radius.bottom_right)));
            self.locations[3] = Point::from((win_size.w, -width + f64::from(radius.top_right)));

            // Top-left corner
            self.sizes[4] = Size::from((f64::from(radius.top_left), f64::from(radius.top_left)));
            self.locations[4] = Point::from((-width, -width));

            // Top-right corner
            self.sizes[5] = Size::from((f64::from(radius.top_right), f64::from(radius.top_right)));
            self.locations[5] = Point::from((win_size.w + width - f64::from(radius.top_right), -width));

            // Bottom-right corner
            self.sizes[6] = Size::from((f64::from(radius.bottom_right), f64::from(radius.bottom_right)));
            self.locations[6] = Point::from((
                win_size.w + width - f64::from(radius.bottom_right),
                win_size.h + width - f64::from(radius.bottom_right),
            ));

            // Bottom-left corner
            self.sizes[7] = Size::from((f64::from(radius.bottom_left), f64::from(radius.bottom_left)));
            self.locations[7] = Point::from((
                -width,
                win_size.h + width - f64::from(radius.bottom_left),
            ));

            // Resize buffers
            for (buf, size) in self.buffers.iter_mut().zip(self.sizes.iter()) {
                buf.resize(*size);
            }

            // Update borders
            for (border, (loc, size)) in self.borders.iter_mut().zip(self.locations.iter().zip(self.sizes.iter())) {
                border.update(
                    *size,
                    Rectangle::new(gradient_area.loc - *loc, gradient_area.size),
                    gradient.in_,
                    gradient.from,
                    gradient.to,
                    ((gradient.angle as f32) - 90.).to_radians(),
                    Rectangle::new(full_rect.loc - *loc, full_rect.size),
                    rounded_corner_border_width,
                    radius,
                    scale as f32,
                    alpha,
                );
            }
        } else {
            // Non-border mode
            self.sizes[0] = self.full_size;
            self.buffers[0].resize(self.sizes[0]);
            self.locations[0] = Point::from((-width, -width));
            // Update border for single segment
            self.borders[0].update(
                self.sizes[0],
                Rectangle::new(gradient_area.loc - self.locations[0], gradient_area.size),
                gradient.in_,
                gradient.from,
                gradient.to,
                ((gradient.angle as f32) - 90.).to_radians(),
                Rectangle::new(full_rect.loc - self.locations[0], full_rect.size),
                rounded_corner_border_width,
                radius,
                scale as f32,
                alpha,
            );
        }
        self.is_border = is_border;
    }


    pub fn render(
        &self,
        renderer: &mut impl NiriRenderer,
        location: Point<f64, Logical>,
    ) -> impl Iterator<Item = FocusRingRenderElement> {
        let mut rv = ArrayVec::<_,8>::new();

        if self.config.off {
            return rv.into_iter();
        }

        let border_width = -self.locations[0].y;

        if self.is_border && border_width == 0. {
            return rv.into_iter();
        }

        let has_shader = BorderRenderElement::has_shader(renderer);

        let mut push = |buffer: &SolidColorBuffer, border: &BorderRenderElement, location: Point<f64, Logical>| {
            let elem = if self.use_border_shader && has_shader {
                border.clone().with_location(location).into()
            } else {
                let alpha = border.alpha();
                SolidColorRenderElement::from_buffer(buffer, location, alpha, Kind::Unspecified).into()
            };
            rv.push(elem);
        };

        if self.is_border {
            for ((buf, border), loc) in zip(&self.buffers, &self.borders).zip(self.locations.iter()) {
                push(buf, border, location + *loc);
            }
        } else {
            push(&self.buffers[0], &self.borders[0], location + self.locations[0]);
        }

        rv.into_iter()
    }

    pub fn width(&self) -> f64 {
        self.config.width.0
    }

    pub fn is_off(&self) -> bool {
        self.config.off
    }

    pub fn config(&self) -> &niri_config::FocusRing {
        &self.config
    }
}
