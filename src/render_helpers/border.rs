use std::collections::HashMap;

use glam::{Mat3, Vec2};
use niri_config::{
    Color, CornerRadius, GradientColorSpace, GradientInterpolation, HueInterpolation,
};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, Uniform};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use super::renderer::NiriRenderer;
use super::shader_element::ShaderRenderElement;
use super::shaders::{mat3_uniform, ProgramType, Shaders};
use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};

/// Renders a wide variety of borders and border parts.
///
/// This includes:
/// * sub- or super-rect of an angled linear gradient like CSS linear-gradient(angle, a, b).
/// * corner rounding.
/// * as a background rectangle and as parts of a border line.
/// * animated rainbow borders with clockwise rotation.
#[derive(Debug, Clone)]
pub struct BorderRenderElement {
    inner: ShaderRenderElement,
    params: Parameters,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Parameters {
    size: Size<f64, Logical>,
    gradient_area: Rectangle<f64, Logical>,
    gradient_format: GradientInterpolation,
    color_from: Color,
    color_to: Color,
    angle: f32,
    geometry: Rectangle<f64, Logical>,
    border_width: f32,
    corner_radius: CornerRadius,
    // Should only be used for visual improvements, i.e. corner radius anti-aliasing.
    scale: f32,
    alpha: f32,
    // Rainbow animation parameters
    animation_time: f32,
    rainbow_speed: f32,
    enable_rainbow: bool,
}

impl BorderRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        size: Size<f64, Logical>,
        gradient_area: Rectangle<f64, Logical>,
        gradient_format: GradientInterpolation,
        color_from: Color,
        color_to: Color,
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
        alpha: f32,
    ) -> Self {
        let inner = ShaderRenderElement::empty(ProgramType::Border, Kind::Unspecified);
        let mut rv = Self {
            inner,
            params: Parameters {
                size,
                gradient_area,
                gradient_format,
                color_from,
                color_to,
                angle,
                geometry,
                border_width,
                corner_radius,
                scale,
                alpha,
                animation_time: 0.0,
                rainbow_speed: 1.0,
                enable_rainbow: false,
            },
        };
        rv.update_inner();
        rv
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_animation(
        size: Size<f64, Logical>,
        gradient_area: Rectangle<f64, Logical>,
        gradient_format: GradientInterpolation,
        color_from: Color,
        color_to: Color,
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
        alpha: f32,
        animation_time: f32,
        rainbow_speed: f32,
        enable_rainbow: bool,
    ) -> Self {
        let inner = ShaderRenderElement::empty(ProgramType::Border, Kind::Unspecified);
        let mut rv = Self {
            inner,
            params: Parameters {
                size,
                gradient_area,
                gradient_format,
                color_from,
                color_to,
                angle,
                geometry,
                border_width,
                corner_radius,
                scale,
                alpha,
                animation_time,
                rainbow_speed,
                enable_rainbow,
            },
        };
        rv.update_inner();
        rv
    }

    pub fn empty() -> Self {
        let inner = ShaderRenderElement::empty(ProgramType::Border, Kind::Unspecified);
        Self {
            inner,
            params: Parameters {
                size: Default::default(),
                gradient_area: Default::default(),
                gradient_format: GradientInterpolation::default(),
                color_from: Default::default(),
                color_to: Default::default(),
                angle: 0.,
                geometry: Default::default(),
                border_width: 0.,
                corner_radius: Default::default(),
                scale: 1.,
                alpha: 1.,
                animation_time: 0.0,
                rainbow_speed: 1.0,
                enable_rainbow: false,
            },
        }
    }

    pub fn damage_all(&mut self) {
        self.inner.damage_all();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        size: Size<f64, Logical>,
        gradient_area: Rectangle<f64, Logical>,
        gradient_format: GradientInterpolation,
        color_from: Color,
        color_to: Color,
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
        alpha: f32,
    ) {
        let params = Parameters {
            size,
            gradient_area,
            gradient_format,
            color_from,
            color_to,
            angle,
            geometry,
            border_width,
            corner_radius,
            scale,
            alpha,
            // Keep existing animation parameters
            animation_time: self.params.animation_time,
            rainbow_speed: self.params.rainbow_speed,
            enable_rainbow: self.params.enable_rainbow,
        };
        if self.params == params {
            return;
        }

        self.params = params;
        self.update_inner();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_with_animation(
        &mut self,
        size: Size<f64, Logical>,
        gradient_area: Rectangle<f64, Logical>,
        gradient_format: GradientInterpolation,
        color_from: Color,
        color_to: Color,
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
        alpha: f32,
        animation_time: f32,
        rainbow_speed: f32,
        enable_rainbow: bool,
    ) {
        let params = Parameters {
            size,
            gradient_area,
            gradient_format,
            color_from,
            color_to,
            angle,
            geometry,
            border_width,
            corner_radius,
            scale,
            alpha,
            animation_time,
            rainbow_speed,
            enable_rainbow,
        };
        if self.params == params {
            return;
        }

        self.params = params;
        self.update_inner();
    }

    pub fn update_animation_time(&mut self, animation_time: f32) {
        if (self.params.animation_time - animation_time).abs() < f32::EPSILON {
            return;
        }

        self.params.animation_time = animation_time;
        if self.params.enable_rainbow {
            self.update_inner();
        }
    }

    pub fn set_rainbow_enabled(&mut self, enabled: bool) {
        if self.params.enable_rainbow == enabled {
            return;
        }

        self.params.enable_rainbow = enabled;
        self.update_inner();
    }

    pub fn set_rainbow_speed(&mut self, speed: f32) {
        if (self.params.rainbow_speed - speed).abs() < f32::EPSILON {
            return;
        }

        self.params.rainbow_speed = speed;
        if self.params.enable_rainbow {
            self.update_inner();
        }
    }

    fn update_inner(&mut self) {
        let Parameters {
            size,
            gradient_area,
            gradient_format,
            color_from,
            color_to,
            angle,
            geometry,
            border_width,
            corner_radius,
            scale,
            alpha,
            animation_time,
            rainbow_speed,
            enable_rainbow,
        } = self.params;

        let grad_offset = geometry.loc - gradient_area.loc;
        let grad_offset = Vec2::new(grad_offset.x as f32, grad_offset.y as f32);

        let grad_dir = Vec2::from_angle(angle);

        let (w, h) = (gradient_area.size.w as f32, gradient_area.size.h as f32);

        let mut grad_area_diag = Vec2::new(w, h);
        if (grad_dir.x < 0. && 0. <= grad_dir.y) || (0. <= grad_dir.x && grad_dir.y < 0.) {
            grad_area_diag.x = -w;
        }

        let mut grad_vec = grad_area_diag.project_onto(grad_dir);
        if grad_dir.y < 0. {
            grad_vec = -grad_vec;
        }

        let area_size = Vec2::new(size.w as f32, size.h as f32);

        let geo_loc = Vec2::new(geometry.loc.x as f32, geometry.loc.y as f32);
        let geo_size = Vec2::new(geometry.size.w as f32, geometry.size.h as f32);

        let input_to_geo =
            Mat3::from_scale(area_size) * Mat3::from_translation(-geo_loc / area_size);

        let colorspace = match gradient_format.color_space {
            GradientColorSpace::Srgb => 0.,
            GradientColorSpace::SrgbLinear => 1.,
            GradientColorSpace::Oklab => 2.,
            GradientColorSpace::Oklch => 3.,
        };

        let hue_interpolation = match gradient_format.hue_interpolation {
            HueInterpolation::Shorter => 0.,
            HueInterpolation::Longer => 1.,
            HueInterpolation::Increasing => 2.,
            HueInterpolation::Decreasing => 3.,
        };

        self.inner.update(
            size,
            None,
            scale,
            alpha,
            vec![
                Uniform::new("colorspace", colorspace),
                Uniform::new("hue_interpolation", hue_interpolation),
                Uniform::new("color_from", color_from.to_array_unpremul()),
                Uniform::new("color_to", color_to.to_array_unpremul()),
                Uniform::new("grad_offset", grad_offset.to_array()),
                Uniform::new("grad_width", w),
                Uniform::new("grad_vec", grad_vec.to_array()),
                mat3_uniform("input_to_geo", input_to_geo),
                Uniform::new("geo_size", geo_size.to_array()),
                Uniform::new("outer_radius", <[f32; 4]>::from(corner_radius)),
                Uniform::new("border_width", border_width),
                // Rainbow animation uniforms
                Uniform::new("animation_time", animation_time),
                Uniform::new("rainbow_speed", rainbow_speed),
                Uniform::new("enable_rainbow", if enable_rainbow { 1.0 } else { 0.0 }),
            ],
            HashMap::new(),
        );
    }

    pub fn with_location(mut self, location: Point<f64, Logical>) -> Self {
        self.inner = self.inner.with_location(location);
        self
    }

    pub fn has_shader(renderer: &mut impl NiriRenderer) -> bool {
        Shaders::get(renderer)
            .program(ProgramType::Border)
            .is_some()
    }

    pub fn is_rainbow_enabled(&self) -> bool {
        self.params.enable_rainbow
    }

    pub fn rainbow_speed(&self) -> f32 {
        self.params.rainbow_speed
    }

    pub fn animation_time(&self) -> f32 {
        self.params.animation_time
    }
}

impl Default for BorderRenderElement {
    fn default() -> Self {
        Self::empty()
    }
}

impl Element for BorderRenderElement {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.inner.opaque_regions(scale)
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl RenderElement<GlesRenderer> for BorderRenderElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        RenderElement::<GlesRenderer>::draw(&self.inner, frame, src, dst, damage, opaque_regions)
    }

    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage> {
        self.inner.underlying_storage(renderer)
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for BorderRenderElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        RenderElement::<TtyRenderer<'_>>::draw(&self.inner, frame, src, dst, damage, opaque_regions)
    }

    fn underlying_storage(&self, renderer: &mut TtyRenderer<'render>) -> Option<UnderlyingStorage> {
        self.inner.underlying_storage(renderer)
    }
}