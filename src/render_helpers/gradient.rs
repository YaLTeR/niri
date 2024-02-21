use std::f32::consts::{self, FRAC_PI_2, PI};

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, Uniform};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::{Buffer, Logical, Physical, Rectangle, Scale, Transform};

use super::primary_gpu_pixel_shader::PrimaryGpuPixelShaderRenderElement;
use super::renderer::NiriRenderer;
use super::shaders::Shaders;
use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};

/// Renders a sub- or super-rect of an angled linear gradient like CSS linear-gradient(angle, a, b).
#[derive(Debug)]
pub struct GradientRenderElement(PrimaryGpuPixelShaderRenderElement);

impl GradientRenderElement {
    pub fn new(
        renderer: &mut impl NiriRenderer,
        scale: Scale<f64>,
        area: Rectangle<i32, Logical>,
        gradient_area: Rectangle<i32, Logical>,
        color_from: [f32; 4],
        color_to: [f32; 4],
        mut angle: f32,
    ) -> Option<Self> {
        let shader = Shaders::get(renderer).gradient_border.clone()?;
        let g_offset = (area.loc - gradient_area.loc).to_f64().to_physical(scale);

        let g_size = gradient_area.size.to_f64().to_physical(scale);
        let (w, h) = (g_size.w as f32, g_size.h as f32);
        let g_area_angle = f32::atan2(h, w);
        let g_area_diag = f32::hypot(h, w);

        // Normalize the angle to [0°; 360°).
        while angle < 0. {
            angle += consts::TAU;
        }
        while angle >= consts::TAU {
            angle -= consts::TAU;
        }

        let angle_diag_to_grad =
            if (0. ..=FRAC_PI_2).contains(&angle) || (PI..=PI + FRAC_PI_2).contains(&angle) {
                angle - g_area_angle
            } else {
                (PI - angle) - g_area_angle
            };
        let g_total = angle_diag_to_grad.cos().abs() * g_area_diag;

        let elem = PixelShaderElement::new(
            shader,
            area,
            None,
            1.,
            vec![
                Uniform::new("color_from", color_from),
                Uniform::new("color_to", color_to),
                Uniform::new("angle", angle),
                Uniform::new("gradient_offset", (g_offset.x as f32, g_offset.y as f32)),
                Uniform::new("gradient_width", w),
                Uniform::new("gradient_total", g_total),
            ],
            Kind::Unspecified,
        );
        Some(Self(PrimaryGpuPixelShaderRenderElement(elem)))
    }
}

impl Element for GradientRenderElement {
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
    ) -> Vec<Rectangle<i32, Physical>> {
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

impl RenderElement<GlesRenderer> for GradientRenderElement {
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

impl<'render> RenderElement<TtyRenderer<'render>> for GradientRenderElement {
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
