use std::time::Duration;

use anyhow::Context as _;
use niri_config::BlockOutFrom;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::utils::{
    Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::backend::renderer::element::{Id, Kind, RenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::Renderer as _;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size, Transform};

use crate::animation::Animation;
use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::snapshot::RenderSnapshot;
use crate::render_helpers::{render_to_encompassing_texture, RenderTarget};

#[derive(Debug)]
pub struct ClosingWindow {
    /// Contents of the window.
    texture: GlesTexture,

    /// Blocked-out contents of the window.
    blocked_out_texture: GlesTexture,

    /// Scale that the textures was rendered with.
    texture_scale: Scale<f64>,

    /// ID of the textures' renderer.
    texture_renderer_id: usize,

    /// Where the window should be blocked out from.
    block_out_from: Option<BlockOutFrom>,

    /// Size of the window geometry.
    geo_size: Size<i32, Logical>,

    /// Position in the workspace.
    pos: Point<i32, Logical>,

    /// How much the texture should be offset.
    texture_offset: Point<f64, Logical>,

    /// How much the blocked-out texture should be offset.
    blocked_out_texture_offset: Point<f64, Logical>,

    /// The closing animation.
    anim: Animation,
}

niri_render_elements! {
    ClosingWindowRenderElement => {
        Texture = RelocateRenderElement<RescaleRenderElement<PrimaryGpuTextureRenderElement>>,
    }
}

impl ClosingWindow {
    pub fn new<E: RenderElement<GlesRenderer>>(
        renderer: &mut GlesRenderer,
        snapshot: RenderSnapshot<E, E>,
        scale: Scale<f64>,
        geo_size: Size<i32, Logical>,
        pos: Point<i32, Logical>,
        anim: Animation,
    ) -> anyhow::Result<Self> {
        let _span = tracy_client::span!("ClosingWindow::new");

        let mut render_to_texture = |elements: Vec<E>| -> anyhow::Result<_> {
            let (texture, _sync_point, geo) = render_to_encompassing_texture(
                renderer,
                scale,
                Transform::Normal,
                Fourcc::Abgr8888,
                &elements,
            )
            .context("error rendering to texture")?;

            let offset = geo.loc.to_f64().to_logical(scale);

            Ok((texture, offset))
        };

        let (texture, texture_offset) =
            render_to_texture(snapshot.contents).context("error rendering contents")?;
        let (blocked_out_texture, blocked_out_texture_offset) =
            render_to_texture(snapshot.blocked_out_contents)
                .context("error rendering blocked-out contents")?;

        Ok(Self {
            texture,
            blocked_out_texture,
            texture_scale: scale,
            texture_renderer_id: renderer.id(),
            block_out_from: snapshot.block_out_from,
            geo_size,
            pos,
            texture_offset,
            blocked_out_texture_offset,
            anim,
        })
    }

    pub fn advance_animations(&mut self, current_time: Duration) {
        self.anim.set_current_time(current_time);
    }

    pub fn are_animations_ongoing(&self) -> bool {
        !self.anim.is_clamped_done()
    }

    pub fn render(
        &self,
        view_rect: Rectangle<i32, Logical>,
        scale: Scale<f64>,
        target: RenderTarget,
    ) -> ClosingWindowRenderElement {
        let val = self.anim.clamped_value();

        let (texture, offset) = if target.should_block_out(self.block_out_from) {
            (&self.blocked_out_texture, self.blocked_out_texture_offset)
        } else {
            (&self.texture, self.texture_offset)
        };

        let elem = TextureRenderElement::from_static_texture(
            Id::new(),
            self.texture_renderer_id,
            Point::from((0., 0.)),
            texture.clone(),
            self.texture_scale.x as i32,
            Transform::Normal,
            Some(val.clamp(0., 1.) as f32),
            None,
            None,
            None,
            Kind::Unspecified,
        );

        let elem = PrimaryGpuTextureRenderElement(elem);

        let center = self.geo_size.to_point().to_f64().downscale(2.);
        let elem = RescaleRenderElement::from_element(
            elem,
            (center - offset).to_physical_precise_round(scale),
            (val / 5. + 0.8).max(0.),
        );

        let mut location = self.pos.to_f64() + offset;
        location.x -= view_rect.loc.x as f64;
        let elem = RelocateRenderElement::from_element(
            elem,
            location.to_physical_precise_round(scale),
            Relocate::Relative,
        );

        elem.into()
    }
}
