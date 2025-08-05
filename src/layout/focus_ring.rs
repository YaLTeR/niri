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
    // Rainbow animation config
    rainbow_enabled: bool,
    rainbow_speed: f32,
    animation_time: f32,
    animate_focus_only: bool,
}

niri_render_elements! {
    FocusRingRenderElement => {
        SolidColor = SolidColorRenderElement,
        Gradient = BorderRenderElement,
    }
}

impl FocusRing {
    pub fn new(config: niri_config::FocusRing) -> Self {
        let (rainbow_enabled, rainbow_speed, animate_focus_only) = Self::read_rainbow_config(&config);
        Self {
            buffers: Default::default(),
            locations: Default::default(),
            sizes: Default::default(),
            borders: Default::default(),
            full_size: Default::default(),
            is_border: false,
            use_border_shader: false,
            config,
            rainbow_enabled,
            rainbow_speed,
            animation_time: 0.0,
            animate_focus_only,
        }
    }

    pub fn update_config(&mut self, config: niri_config::FocusRing) {
        let (rainbow_enabled, rainbow_speed, animate_focus_only) = Self::read_rainbow_config(&config);
        self.config = config;
        self.rainbow_enabled = rainbow_enabled;
        self.rainbow_speed = rainbow_speed;
        self.animate_focus_only = animate_focus_only;
    }

    pub fn set_rainbow_enabled(&mut self, enabled: bool) {
        self.rainbow_enabled = enabled;
        for border in &mut self.borders {
            border.set_rainbow_enabled(enabled);
        }
    }

    pub fn set_rainbow_speed(&mut self, speed: f32) {
        self.rainbow_speed = speed;
        for border in &mut self.borders {
            border.set_rainbow_speed(speed);
        }
    }

    pub fn set_focus_only_animation(&mut self, enabled: bool) {
        self.animate_focus_only = enabled;
    }

    pub fn update_animation_time(&mut self, animation_time: f32) {
        self.animation_time = animation_time;
        if self.rainbow_enabled {
            for border in &mut self.borders {
                border.update_animation_time(animation_time);
            }
        }
    }

    // Retrieve rainbow settings from config if available
    // Since the config doesn't have these fields yet, we'll use default values
    fn read_rainbow_config(_config: &niri_config::FocusRing) -> (bool, f32, bool) {
        // TODO: Once rainbow fields are added to FocusRing config, use them here
        // For now, return default values
        let rainbow_enabled = false; // config.rainbow_enabled.unwrap_or(false);
        let rainbow_speed = 1.0; // config.rainbow_speed.unwrap_or(1.0);
        let animate_focus_only = true; // config.rainbow_focus_only.unwrap_or(true);
        (rainbow_enabled, rainbow_speed, animate_focus_only)
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
        self.full_size = win_size + Size::from((width, width)).upscale(2.);

        let color = if is_active {
            self.config.active_color
        } else {
            self.config.inactive_color
        };

        for buf in &mut self.buffers {
            buf.set_color(color.to_array_premul());
        }

        let radius = radius.fit_to(self.full_size.w as f32, self.full_size.h as f32);

        let gradient = if is_active {
            self.config.active_gradient
        } else {
            self.config.inactive_gradient
        };

        // Set to use border shader if rainbow animation is enabled for this ring
        self.use_border_shader = self.rainbow_enabled
            || radius != CornerRadius::default()
            || gradient.is_some();

        // Set the defaults for solid color + rounded corners.
        let gradient = gradient.unwrap_or_else(|| Gradient::from(color));

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
            let top_left = f64::max(width, ceil(f64::from(radius.top_left)));
            let top_right = f64::min(
                self.full_size.w - top_left,
                f64::max(width, ceil(f64::from(radius.top_right))),
            );
            let bottom_left = f64::min(
                self.full_size.h - top_left,
                f64::max(width, ceil(f64::from(radius.bottom_left))),
            );
            let bottom_right = f64::min(
                self.full_size.h - top_right,
                f64::min(
                    self.full_size.w - bottom_left,
                    f64::max(width, ceil(f64::from(radius.bottom_right))),
                ),
            );

            // Top edge.
            self.sizes[0] = Size::from((win_size.w + width * 2. - top_left - top_right, width));
            self.locations[0] = Point::from((-width + top_left, -width));
            // Bottom edge.
            self.sizes[1] = Size::from((win_size.w + width * 2. - bottom_left - bottom_right, width));
            self.locations[1] = Point::from((-width + bottom_left, win_size.h));
            // Left edge.
            self.sizes[2] = Size::from((width, win_size.h + width * 2. - top_left - bottom_left));
            self.locations[2] = Point::from((-width, -width + top_left));
            // Right edge.
            self.sizes[3] = Size::from((width, win_size.h + width * 2. - top_right - bottom_right));
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

            for (_i, (border, (loc, size))) in zip(&mut self.borders, zip(self.locations, self.sizes)).enumerate() {
                border.update_with_animation(
                    size,
                    Rectangle::new(gradient_area.loc - loc, gradient_area.size),
                    gradient.in_,
                    gradient.from,
                    gradient.to,
                    ((gradient.angle as f32) - 90.).to_radians(),
                    Rectangle::new(full_rect.loc - loc, full_rect.size),
                    rounded_corner_border_width,
                    radius,
                    scale as f32,
                    alpha,
                    self.animation_time,
                    self.rainbow_speed,
                    self.rainbow_enabled && (!self.animate_focus_only || is_active),
                );
            }
        } else {
            self.sizes[0] = self.full_size;
            self.buffers[0].resize(self.sizes[0]);
            self.locations[0] = Point::from((-width, -width));

            self.borders[0].update_with_animation(
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
                self.animation_time,
                self.rainbow_speed,
                self.rainbow_enabled && (!self.animate_focus_only || is_active),
            );
        }

        self.is_border = is_border;
    }

    pub fn render(
        &self,
        renderer: &mut impl NiriRenderer,
        location: Point<f64, Logical>,
    ) -> impl Iterator<Item = FocusRingRenderElement> {
        let mut rv = ArrayVec::<_, 8>::new();

        if self.config.off {
            return rv.into_iter();
        }

        let border_width = -self.locations[0].y;

        if self.is_border && border_width == 0. {
            return rv.into_iter();
        }

        let has_border_shader = BorderRenderElement::has_shader(renderer);

        let mut push = |buffer, border: &BorderRenderElement, location: Point<f64, Logical>| {
            let elem = if self.use_border_shader && has_border_shader {
                border.clone().with_location(location).into()
            } else {
                let alpha = border.alpha();
                SolidColorRenderElement::from_buffer(buffer, location, alpha, Kind::Unspecified)
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
