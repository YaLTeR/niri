use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet};
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Transform};

use super::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use super::render_to_texture;
use super::renderer::AsGlesFrame;
use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};

/// Renders elements into an off-screen buffer.
#[derive(Debug)]
pub struct OffscreenRenderElement {
    // The texture, if rendering succeeded.
    texture: Option<PrimaryGpuTextureRenderElement>,
    // The fallback buffer in case the rendering fails.
    fallback: SolidColorRenderElement,
}

impl OffscreenRenderElement {
    pub fn new(
        renderer: &mut GlesRenderer,
        scale: i32,
        elements: &[impl RenderElement<GlesRenderer>],
        result_alpha: f32,
    ) -> Self {
        let _span = tracy_client::span!("OffscreenRenderElement::new");

        let geo = elements
            .iter()
            .map(|ele| ele.geometry(Scale::from(f64::from(scale))))
            .reduce(|a, b| a.merge(b))
            .unwrap_or_default();
        let logical_size = geo.size.to_logical(scale);

        let fallback_buffer = SolidColorBuffer::new(logical_size, [1., 0., 0., 1.]);
        let fallback = SolidColorRenderElement::from_buffer(
            &fallback_buffer,
            geo.loc,
            Scale::from(scale as f64),
            result_alpha,
            Kind::Unspecified,
        );

        let elements = elements.iter().rev().map(|ele| {
            RelocateRenderElement::from_element(ele, (-geo.loc.x, -geo.loc.y), Relocate::Relative)
        });

        match render_to_texture(
            renderer,
            geo.size,
            Scale::from(scale as f64),
            Transform::Normal,
            Fourcc::Abgr8888,
            elements,
        ) {
            Ok((texture, _sync_point)) => {
                let buffer =
                    TextureBuffer::from_texture(renderer, texture, scale, Transform::Normal, None);
                let element = TextureRenderElement::from_texture_buffer(
                    geo.loc.to_f64(),
                    &buffer,
                    Some(result_alpha),
                    None,
                    None,
                    Kind::Unspecified,
                );
                Self {
                    texture: Some(PrimaryGpuTextureRenderElement(element)),
                    fallback,
                }
            }
            Err(err) => {
                warn!("error off-screening elements: {err:?}");
                Self {
                    texture: None,
                    fallback,
                }
            }
        }
    }
}

impl Element for OffscreenRenderElement {
    fn id(&self) -> &Id {
        if let Some(texture) = &self.texture {
            texture.id()
        } else {
            self.fallback.id()
        }
    }

    fn current_commit(&self) -> CommitCounter {
        if let Some(texture) = &self.texture {
            texture.current_commit()
        } else {
            self.fallback.current_commit()
        }
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        if let Some(texture) = &self.texture {
            texture.geometry(scale)
        } else {
            self.fallback.geometry(scale)
        }
    }

    fn transform(&self) -> Transform {
        if let Some(texture) = &self.texture {
            texture.transform()
        } else {
            self.fallback.transform()
        }
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        if let Some(texture) = &self.texture {
            texture.src()
        } else {
            self.fallback.src()
        }
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        if let Some(texture) = &self.texture {
            texture.damage_since(scale, commit)
        } else {
            self.fallback.damage_since(scale, commit)
        }
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> Vec<Rectangle<i32, Physical>> {
        if let Some(texture) = &self.texture {
            texture.opaque_regions(scale)
        } else {
            self.fallback.opaque_regions(scale)
        }
    }

    fn alpha(&self) -> f32 {
        if let Some(texture) = &self.texture {
            texture.alpha()
        } else {
            self.fallback.alpha()
        }
    }

    fn kind(&self) -> Kind {
        if let Some(texture) = &self.texture {
            texture.kind()
        } else {
            self.fallback.kind()
        }
    }
}

impl RenderElement<GlesRenderer> for OffscreenRenderElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        let gles_frame = frame.as_gles_frame();
        if let Some(texture) = &self.texture {
            RenderElement::<GlesRenderer>::draw(texture, gles_frame, src, dst, damage)?;
        } else {
            RenderElement::<GlesRenderer>::draw(&self.fallback, gles_frame, src, dst, damage)?;
        }
        Ok(())
    }

    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage> {
        if let Some(texture) = &self.texture {
            texture.underlying_storage(renderer)
        } else {
            self.fallback.underlying_storage(renderer)
        }
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for OffscreenRenderElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        let gles_frame = frame.as_gles_frame();
        if let Some(texture) = &self.texture {
            RenderElement::<GlesRenderer>::draw(texture, gles_frame, src, dst, damage)?;
        } else {
            RenderElement::<GlesRenderer>::draw(&self.fallback, gles_frame, src, dst, damage)?;
        }
        Ok(())
    }

    fn underlying_storage(&self, renderer: &mut TtyRenderer<'render>) -> Option<UnderlyingStorage> {
        if let Some(texture) = &self.texture {
            texture.underlying_storage(renderer)
        } else {
            self.fallback.underlying_storage(renderer)
        }
    }
}
