use std::collections::HashMap;

use glam::{Mat3, Vec2};
use niri_config::CornerRadius;
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
#[derive(Debug, Clone)]
pub struct BorderRenderElement {
    inner: ShaderRenderElement,
    params: Parameters,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Parameters {
    size: Size<f64, Logical>,
    gradient_area: Rectangle<f64, Logical>,
    gradient_format: f32,
    color_from: [f32; 4],
    color_to: [f32; 4],
    angle: f32,
    geometry: Rectangle<f64, Logical>,
    border_width: f32,
    corner_radius: CornerRadius,
    // Should only be used for visual improvements, i.e. corner radius anti-aliasing.
    scale: f32,
}

impl BorderRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        size: Size<f64, Logical>,
        gradient_area: Rectangle<f64, Logical>,
        gradient_format: f32,
        color_from: [f32; 4],
        color_to: [f32; 4],
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
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
                gradient_format: 0.,
                color_from: Default::default(),
                color_to: Default::default(),
                angle: 0.,
                geometry: Default::default(),
                border_width: 0.,
                corner_radius: Default::default(),
                scale: 1.,
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
        gradient_format: f32,
        color_from: [f32; 4],
        color_to: [f32; 4],
        angle: f32,
        geometry: Rectangle<f64, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
        scale: f32,
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
        };
        if self.params == params {
            return;
        }

        self.params = params;
        self.update_inner();
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
        if grad_dir.y <= 0. {
            grad_vec = -grad_vec;
        }

        let area_size = Vec2::new(size.w as f32, size.h as f32);

        let geo_loc = Vec2::new(geometry.loc.x as f32, geometry.loc.y as f32);
        let geo_size = Vec2::new(geometry.size.w as f32, geometry.size.h as f32);

        let input_to_geo =
            Mat3::from_scale(area_size) * Mat3::from_translation(-geo_loc / area_size);

        self.inner.update(
            size,
            None,
            scale,
            vec![
                Uniform::new("grad_format", gradient_format),
                Uniform::new("color_from", color_from),
                Uniform::new("color_to", color_to),
                Uniform::new("grad_offset", grad_offset.to_array()),
                Uniform::new("grad_width", w),
                Uniform::new("grad_vec", grad_vec.to_array()),
                mat3_uniform("input_to_geo", input_to_geo),
                Uniform::new("geo_size", geo_size.to_array()),
                Uniform::new("outer_radius", <[f32; 4]>::from(corner_radius)),
                Uniform::new("border_width", border_width),
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
        frame: &mut GlesFrame<'_>,
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
        frame: &mut TtyFrame<'_, '_>,
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
