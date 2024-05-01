use std::collections::HashMap;

use glam::{Mat3, Vec2};
use niri_config::CornerRadius;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, Uniform};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet};
use smithay::utils::{Buffer, Logical, Physical, Rectangle, Scale, Transform};

use super::renderer::NiriRenderer;
use super::shader_element::{ShaderProgram, ShaderRenderElement};
use super::shaders::{mat3_uniform, Shaders};
use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};

/// Renders a wide variety of borders and border parts.
///
/// This includes:
/// * sub- or super-rect of an angled linear gradient like CSS linear-gradient(angle, a, b).
/// * corner rounding.
/// * as a background rectangle and as parts of a border line.
#[derive(Debug)]
pub struct BorderRenderElement(ShaderRenderElement);

impl BorderRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shader: ShaderProgram,
        scale: Scale<f64>,
        area: Rectangle<i32, Logical>,
        gradient_area: Rectangle<i32, Logical>,
        color_from: [f32; 4],
        color_to: [f32; 4],
        angle: f32,
        geometry: Rectangle<i32, Logical>,
        border_width: f32,
        corner_radius: CornerRadius,
    ) -> Self {
        let grad_offset = (area.loc - gradient_area.loc).to_f64().to_physical(scale);

        let grad_dir = Vec2::from_angle(angle);

        let grad_area_size = gradient_area.size.to_f64().to_physical(scale);
        let (w, h) = (grad_area_size.w as f32, grad_area_size.h as f32);

        let mut grad_area_diag = Vec2::new(w, h);
        if (grad_dir.x < 0. && 0. <= grad_dir.y) || (0. <= grad_dir.x && grad_dir.y < 0.) {
            grad_area_diag.x = -w;
        }

        let mut grad_vec = grad_area_diag.project_onto(grad_dir);
        if grad_dir.y <= 0. {
            grad_vec = -grad_vec;
        }

        let area_physical = area.to_physical_precise_round(scale);
        let area_loc = Vec2::new(area_physical.loc.x, area_physical.loc.y);
        let area_size = Vec2::new(area_physical.size.w, area_physical.size.h);

        let geo = geometry.to_physical_precise_round(scale);
        let geo_loc = Vec2::new(geo.loc.x, geo.loc.y);
        let geo_size = Vec2::new(geo.size.w, geo.size.h);

        let input_to_geo =
            Mat3::from_scale(area_size) * Mat3::from_translation((area_loc - geo_loc) / area_size);
        let corner_radius = corner_radius.scaled_by(scale.x as f32);
        let border_width = border_width * scale.x as f32;

        let elem = ShaderRenderElement::new(
            shader,
            HashMap::new(),
            area,
            area.size.to_f64().to_buffer(scale, Transform::Normal),
            None,
            1.,
            vec![
                Uniform::new("color_from", color_from),
                Uniform::new("color_to", color_to),
                Uniform::new("grad_offset", (grad_offset.x as f32, grad_offset.y as f32)),
                Uniform::new("grad_width", w),
                Uniform::new("grad_vec", grad_vec.to_array()),
                mat3_uniform("input_to_geo", input_to_geo),
                Uniform::new("geo_size", geo_size.to_array()),
                Uniform::new("outer_radius", <[f32; 4]>::from(corner_radius)),
                Uniform::new("border_width", border_width),
            ],
            Kind::Unspecified,
        );
        Self(elem)
    }

    pub fn shader(renderer: &mut impl NiriRenderer) -> Option<&ShaderProgram> {
        Shaders::get(renderer).border.as_ref()
    }
}

impl Element for BorderRenderElement {
    fn id(&self) -> &Id {
        self.0.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.0.current_commit()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.0.geometry(scale)
    }

    fn transform(&self) -> Transform {
        self.0.transform()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.0.src()
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.0.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> Vec<Rectangle<i32, Physical>> {
        self.0.opaque_regions(scale)
    }

    fn alpha(&self) -> f32 {
        self.0.alpha()
    }

    fn kind(&self) -> Kind {
        self.0.kind()
    }
}

impl RenderElement<GlesRenderer> for BorderRenderElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        RenderElement::<GlesRenderer>::draw(&self.0, frame, src, dst, damage)
    }

    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage> {
        self.0.underlying_storage(renderer)
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for BorderRenderElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        RenderElement::<TtyRenderer<'_>>::draw(&self.0, frame, src, dst, damage)
    }

    fn underlying_storage(&self, renderer: &mut TtyRenderer<'render>) -> Option<UnderlyingStorage> {
        self.0.underlying_storage(renderer)
    }
}
