use std::time::Duration;

use anyhow::Context as _;
use niri_config::BlockOutFrom;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::utils::{
    Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::backend::renderer::element::{Kind, RenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::utils::{Logical, Point, Scale, Transform};

use crate::animation::Animation;
use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::snapshot::RenderSnapshot;
use crate::render_helpers::{render_to_encompassing_texture, RenderTarget};

#[derive(Debug)]
pub struct ClosingWindow {
    /// Contents of the window.
    buffer: TextureBuffer<GlesTexture>,

    /// Blocked-out contents of the window.
    blocked_out_buffer: TextureBuffer<GlesTexture>,

    /// Where the window should be blocked out from.
    block_out_from: Option<BlockOutFrom>,

    /// Center of the window geometry.
    center: Point<i32, Logical>,

    /// Position in the workspace.
    pos: Point<i32, Logical>,

    /// How much the buffer should be offset.
    buffer_offset: Point<i32, Logical>,

    /// How much the blocked-out buffer should be offset.
    blocked_out_buffer_offset: Point<i32, Logical>,

    /// The closing animation.
    anim: Animation,

    /// Alpha the animation should start from.
    starting_alpha: f32,

    /// Scale the animation should start from.
    starting_scale: f64,
}

niri_render_elements! {
    ClosingWindowRenderElement => {
        Texture = RelocateRenderElement<RescaleRenderElement<PrimaryGpuTextureRenderElement>>,
    }
}

impl ClosingWindow {
    #[allow(clippy::too_many_arguments)]
    pub fn new<E: RenderElement<GlesRenderer>>(
        renderer: &mut GlesRenderer,
        snapshot: RenderSnapshot<E, E>,
        scale: i32,
        center: Point<i32, Logical>,
        pos: Point<i32, Logical>,
        anim: Animation,
        starting_alpha: f32,
        starting_scale: f64,
    ) -> anyhow::Result<Self> {
        let _span = tracy_client::span!("ClosingWindow::new");

        let mut render_to_buffer = |elements: Vec<E>| -> anyhow::Result<_> {
            let (texture, _sync_point, geo) = render_to_encompassing_texture(
                renderer,
                Scale::from(scale as f64),
                Transform::Normal,
                Fourcc::Abgr8888,
                &elements,
            )
            .context("error rendering to texture")?;

            let buffer =
                TextureBuffer::from_texture(renderer, texture, scale, Transform::Normal, None);
            let offset = geo.loc.to_logical(scale);

            Ok((buffer, offset))
        };

        let (buffer, buffer_offset) =
            render_to_buffer(snapshot.contents).context("error rendering contents")?;
        let (blocked_out_buffer, blocked_out_buffer_offset) =
            render_to_buffer(snapshot.blocked_out_contents)
                .context("error rendering blocked-out contents")?;

        Ok(Self {
            buffer,
            blocked_out_buffer,
            block_out_from: snapshot.block_out_from,
            center,
            pos,
            buffer_offset,
            blocked_out_buffer_offset,
            anim,
            starting_alpha,
            starting_scale,
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
        view_pos: i32,
        scale: Scale<f64>,
        target: RenderTarget,
    ) -> ClosingWindowRenderElement {
        let val = self.anim.clamped_value();

        let block_out = match self.block_out_from {
            None => false,
            Some(BlockOutFrom::Screencast) => target == RenderTarget::Screencast,
            Some(BlockOutFrom::ScreenCapture) => target != RenderTarget::Output,
        };
        let (buffer, offset) = if block_out {
            (&self.blocked_out_buffer, self.blocked_out_buffer_offset)
        } else {
            (&self.buffer, self.buffer_offset)
        };

        let elem = TextureRenderElement::from_texture_buffer(
            Point::from((0., 0.)),
            buffer,
            Some(val.clamp(0., 1.) as f32 * self.starting_alpha),
            None,
            None,
            Kind::Unspecified,
        );

        let elem = PrimaryGpuTextureRenderElement(elem);

        let elem = RescaleRenderElement::from_element(
            elem,
            (self.center - offset).to_physical_precise_round(scale),
            ((val / 5. + 0.8) * self.starting_scale).max(0.),
        );

        let mut location = self.pos + offset;
        location.x -= view_pos;
        let elem = RelocateRenderElement::from_element(
            elem,
            location.to_physical_precise_round(scale),
            Relocate::Relative,
        );

        elem.into()
    }
}
