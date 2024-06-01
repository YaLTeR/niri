use std::time::Duration;

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesTexture;

use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::render_helpers::RenderTarget;

pub const DELAY: Duration = Duration::from_millis(250);
pub const DURATION: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub struct ScreenTransition {
    /// Texture to crossfade from for each render target.
    from_texture: [TextureBuffer<GlesTexture>; 3],
    /// Monotonic time when to start the crossfade.
    start_at: Duration,
    /// Current crossfade alpha.
    alpha: f32,
}

impl ScreenTransition {
    pub fn new(from_texture: [TextureBuffer<GlesTexture>; 3], start_at: Duration) -> Self {
        Self {
            from_texture,
            start_at,
            alpha: 1.,
        }
    }

    pub fn advance_animations(&mut self, current_time: Duration) {
        if self.start_at + DURATION <= current_time {
            self.alpha = 0.;
        } else if self.start_at <= current_time {
            self.alpha = 1. - (current_time - self.start_at).as_secs_f32() / DURATION.as_secs_f32();
        } else {
            self.alpha = 1.;
        }
    }

    pub fn is_done(&self) -> bool {
        self.alpha == 0.
    }

    pub fn render(&self, target: RenderTarget) -> PrimaryGpuTextureRenderElement {
        let idx = match target {
            RenderTarget::Output => 0,
            RenderTarget::Screencast => 1,
            RenderTarget::ScreenCapture => 2,
        };

        PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
            self.from_texture[idx].clone(),
            (0., 0.),
            self.alpha,
            None,
            None,
            Kind::Unspecified,
        ))
    }
}
