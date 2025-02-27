use std::cell::RefCell;

use anyhow::Context as _;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{Bind as _, Offscreen as _, Texture as _};
use smithay::utils::{Logical, Point, Scale, Transform};

use super::texture::TextureBuffer;
use super::{encompassing_geo, render_elements};

/// Buffer for offscreen rendering.
#[derive(Debug)]
pub struct OffscreenBuffer {
    /// The cached texture buffer.
    ///
    /// Lazily created when `render` is called. Recreated when necessary.
    buffer: RefCell<Option<TextureBuffer<GlesTexture>>>,
}

impl OffscreenBuffer {
    pub fn render(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
        elements: &[impl RenderElement<GlesRenderer>],
    ) -> anyhow::Result<(TextureBuffer<GlesTexture>, SyncPoint, Point<f64, Logical>)> {
        let _span = tracy_client::span!("OffscreenBuffer::render");

        let geo = encompassing_geo(scale, elements.iter());
        let elements = elements.iter().rev().map(|ele| {
            RelocateRenderElement::from_element(ele, geo.loc.upscale(-1), Relocate::Relative)
        });

        let buffer_size = geo.size.to_logical(1).to_buffer(1, Transform::Normal);
        let offset = geo.loc.to_f64().to_logical(scale);

        let mut buffer = self.buffer.borrow_mut();

        // Check if we need to create or recreate the texture.
        let size_string;
        let mut reason = "";
        if let Some(buf) = buffer.as_mut() {
            let old_size = buf.texture().size();
            if old_size != buffer_size {
                size_string = format!(
                    "size changed from {} × {} to {} × {}",
                    old_size.w, old_size.h, buffer_size.w, buffer_size.h
                );
                reason = &size_string;

                *buffer = None;
            } else if !buf.is_texture_reference_unique() {
                reason = "not unique";

                *buffer = None;
            }
        } else {
            reason = "first render";
        }

        let buffer = if let Some(buffer) = buffer.as_mut() {
            buffer
        } else {
            trace!("creating new texture: {reason}");
            let span = tracy_client::span!("creating offscreen buffer");
            span.emit_text(reason);

            let texture: GlesTexture = renderer
                .create_buffer(Fourcc::Abgr8888, buffer_size)
                .context("error creating texture")?;

            buffer.insert(TextureBuffer::from_texture(
                renderer,
                texture,
                scale,
                Transform::Normal,
                Vec::new(),
            ))
        };

        // Update the texture scale.
        buffer.set_texture_scale(scale);

        // Increment the commit counter since we're rendering new contents to the buffer.
        buffer.increment_commit_counter();

        // Render to the buffer.
        let mut texture = buffer.texture().clone();
        let mut target = renderer
            .bind(&mut texture)
            .context("error binding texture")?;

        let sync_point = render_elements(
            renderer,
            &mut target,
            geo.size,
            scale,
            Transform::Normal,
            elements,
        )?;

        Ok((buffer.clone(), sync_point, offset))
    }
}

impl Default for OffscreenBuffer {
    fn default() -> Self {
        OffscreenBuffer {
            buffer: RefCell::new(None),
        }
    }
}
