use std::cell::RefCell;

use anyhow::Context as _;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{Bind as _, Color32F, Offscreen as _, Texture as _};
use smithay::utils::{Logical, Point, Scale, Transform};

use super::encompassing_geo;
use super::texture::TextureBuffer;

/// Buffer for offscreen rendering.
#[derive(Debug)]
pub struct OffscreenBuffer {
    /// The cached texture buffer.
    ///
    /// Lazily created when `render` is called. Recreated when necessary.
    inner: RefCell<Option<Inner>>,
}

#[derive(Debug)]
struct Inner {
    buffer: TextureBuffer<GlesTexture>,
    damage: OutputDamageTracker,
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
        let elements = Vec::from_iter(elements.iter().map(|ele| {
            RelocateRenderElement::from_element(ele, geo.loc.upscale(-1), Relocate::Relative)
        }));

        let buffer_size = geo.size.to_logical(1).to_buffer(1, Transform::Normal);
        let offset = geo.loc.to_f64().to_logical(scale);

        let mut inner = self.inner.borrow_mut();

        // Check if we need to create or recreate the texture.
        let size_string;
        let mut reason = "";
        if let Some(Inner { buffer, .. }) = inner.as_mut() {
            let old_size = buffer.texture().size();
            if old_size != buffer_size {
                size_string = format!(
                    "size changed from {} × {} to {} × {}",
                    old_size.w, old_size.h, buffer_size.w, buffer_size.h
                );
                reason = &size_string;

                *inner = None;
            } else if !buffer.is_texture_reference_unique() {
                reason = "not unique";

                *inner = None;
            }
        } else {
            reason = "first render";
        }

        let inner = if let Some(inner) = inner.as_mut() {
            inner
        } else {
            trace!("creating new texture: {reason}");
            let span = tracy_client::span!("creating offscreen buffer");
            span.emit_text(reason);

            let texture: GlesTexture = renderer
                .create_buffer(Fourcc::Abgr8888, buffer_size)
                .context("error creating texture")?;

            let buffer = TextureBuffer::from_texture(
                renderer,
                texture,
                scale,
                Transform::Normal,
                Vec::new(),
            );
            let damage = OutputDamageTracker::new(geo.size, scale, Transform::Normal);

            inner.insert(Inner { buffer, damage })
        };

        // Recreate the damage tracker if the scale changes. We already recreate it for buffer size
        // changes, and transform is always Normal.
        if inner.buffer.texture_scale() != scale {
            inner.buffer.set_texture_scale(scale);

            trace!("recreating damage tracker due to scale change");
            inner.damage = OutputDamageTracker::new(geo.size, scale, Transform::Normal);
        }

        let res = {
            let mut texture = inner.buffer.texture().clone();
            let mut target = renderer.bind(&mut texture)?;
            inner.damage.render_output(
                renderer,
                &mut target,
                1,
                &elements,
                Color32F::TRANSPARENT,
            )?
        };

        if res.damage.is_some() {
            // Increment the commit counter if some contents updated.
            inner.buffer.increment_commit_counter();
        }

        Ok((inner.buffer.clone(), res.sync, offset))
    }
}

impl Default for OffscreenBuffer {
    fn default() -> Self {
        OffscreenBuffer {
            inner: RefCell::new(None),
        }
    }
}
