use std::collections::HashMap;

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture, Uniform};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet};
use smithay::utils::{Buffer, Logical, Physical, Rectangle, Scale, Size, Transform};

use super::primary_gpu_pixel_shader_with_textures::PrimaryGpuPixelShaderWithTexturesRenderElement;
use super::renderer::AsGlesFrame;
use super::shaders::Shaders;
use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};

#[derive(Debug)]
pub struct CrossfadeRenderElement(PrimaryGpuPixelShaderWithTexturesRenderElement);

impl CrossfadeRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        renderer: &mut GlesRenderer,
        area: Rectangle<i32, Logical>,
        scale: Scale<f64>,
        texture_from: (GlesTexture, Rectangle<i32, Physical>),
        size_from: Size<i32, Logical>,
        texture_to: (GlesTexture, Rectangle<i32, Physical>),
        size_to: Size<i32, Logical>,
        amount: f32,
        result_alpha: f32,
    ) -> Option<Self> {
        let (texture_from, texture_from_geo) = texture_from;
        let (texture_to, texture_to_geo) = texture_to;

        let scale_from = area.size.to_f64() / size_from.to_f64();
        let scale_to = area.size.to_f64() / size_to.to_f64();

        let tex_from_geo = texture_from_geo.to_f64().upscale(scale_from);
        let tex_to_geo = texture_to_geo.to_f64().upscale(scale_to);
        let combined_geo = tex_from_geo.merge(tex_to_geo);

        let area = Rectangle::from_loc_and_size(
            area.loc + combined_geo.loc.to_logical(scale).to_i32_round(),
            combined_geo.size.to_logical(scale).to_i32_round(),
        );

        let tex_from_loc = (tex_from_geo.loc - combined_geo.loc)
            .downscale((combined_geo.size.w, combined_geo.size.h));
        let tex_to_loc = (tex_to_geo.loc - combined_geo.loc)
            .downscale((combined_geo.size.w, combined_geo.size.h));
        let tex_from_size = tex_from_geo.size / combined_geo.size;
        let tex_to_size = tex_to_geo.size / combined_geo.size;

        // FIXME: cropping this element will mess up the coordinates.
        Shaders::get(renderer).crossfade.clone().map(|shader| {
            Self(PrimaryGpuPixelShaderWithTexturesRenderElement::new(
                shader,
                HashMap::from([
                    (String::from("tex_from"), texture_from),
                    (String::from("tex_to"), texture_to),
                ]),
                area,
                None,
                result_alpha,
                vec![
                    Uniform::new(
                        "tex_from_loc",
                        (tex_from_loc.x as f32, tex_from_loc.y as f32),
                    ),
                    Uniform::new(
                        "tex_from_size",
                        (tex_from_size.x as f32, tex_from_size.y as f32),
                    ),
                    Uniform::new("tex_to_loc", (tex_to_loc.x as f32, tex_to_loc.y as f32)),
                    Uniform::new("tex_to_size", (tex_to_size.x as f32, tex_to_size.y as f32)),
                    Uniform::new("amount", amount),
                ],
                Kind::Unspecified,
            ))
        })
    }
}

impl Element for CrossfadeRenderElement {
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

impl RenderElement<GlesRenderer> for CrossfadeRenderElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        RenderElement::<GlesRenderer>::draw(&self.0, frame, src, dst, damage)?;
        Ok(())
    }

    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage> {
        self.0.underlying_storage(renderer)
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for CrossfadeRenderElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        let gles_frame = frame.as_gles_frame();
        RenderElement::<GlesRenderer>::draw(&self.0, gles_frame, src, dst, damage)?;
        Ok(())
    }

    fn underlying_storage(&self, renderer: &mut TtyRenderer<'render>) -> Option<UnderlyingStorage> {
        self.0.underlying_storage(renderer)
    }
}
