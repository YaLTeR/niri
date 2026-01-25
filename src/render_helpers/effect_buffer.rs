use std::cell::RefCell;

use anyhow::{ensure, Context as _};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{
    Bind as _, Color32F, ContextId, Offscreen as _, Renderer as _, Texture,
};
use smithay::utils::{Buffer, Physical, Scale, Size, Transform};

use crate::niri::OutputRenderElements;
use crate::render_helpers::blur::Blur;
use crate::render_helpers::shaders::Shaders;

#[derive(Debug)]
pub struct EffectBuffer {
    inner: RefCell<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Size of the effect buffer.
    size: Size<i32, Buffer>,
    /// Scale of the effect buffer.
    scale: Scale<f64>,
    blur_config: niri_config::Blur,

    /// Elements to be rendered on demand.
    elements: Option<Vec<OutputRenderElements<GlesRenderer>>>,
    /// Offscreen buffer where elements get rendered.
    offscreen: Option<Offscreen>,

    /// Commit counter that takes into account both original and blurred texture changes.
    commit_counter: CommitCounter,
}

#[derive(Debug)]
struct Offscreen {
    /// The texture with the offscreen contents.
    texture: GlesTexture,
    /// Id of the renderer context that the texture comes from.
    renderer_context_id: ContextId<GlesTexture>,
    /// Scale of the texture.
    scale: Scale<f64>,
    /// Damage tracker for drawing to the texture.
    damage: OutputDamageTracker,
    /// Blurring program, if available.
    blur: Option<Blur>,
    /// Rendered blurred version of the texture.
    ///
    /// When texture needs to be reblurred, this field must be reset to `None`.
    blurred: Option<GlesTexture>,
}

#[derive(Debug)]
pub struct PreparedEffectBuffer {
    /// The output texture with either original or blurred contents.
    pub texture: GlesTexture,
    /// Id of the renderer context that the texture comes from.
    pub renderer_context_id: ContextId<GlesTexture>,
    /// Scale of the texture.
    pub scale: Scale<f64>,
    /// Commit of the texture.
    pub commit: CommitCounter,
}

impl EffectBuffer {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Inner::new()),
        }
    }

    pub fn update_size(&self, size: Size<i32, Physical>, scale: Scale<f64>) {
        self.inner.borrow_mut().update_size(size, scale)
    }

    pub fn render(&self, renderer: &mut GlesRenderer, blur: bool) -> Option<PreparedEffectBuffer> {
        self.inner.borrow_mut().render(renderer, blur)
    }

    pub fn update_blur_config(&self, config: niri_config::Blur) {
        self.inner.borrow_mut().update_blur_config(config)
    }

    pub fn set_elements(&self, elements: Vec<OutputRenderElements<GlesRenderer>>) {
        self.inner.borrow_mut().set_elements(elements)
    }
}

impl Inner {
    fn new() -> Self {
        Self {
            size: Size::default(),
            scale: Scale::from(1.),
            blur_config: niri_config::Blur::default(),
            elements: None,
            offscreen: None,
            commit_counter: CommitCounter::default(),
        }
    }

    fn update_size(&mut self, size: Size<i32, Physical>, scale: Scale<f64>) {
        self.size = size.to_logical(1).to_buffer(1, Transform::Normal);
        self.scale = scale;
    }

    fn update_blur_config(&mut self, config: niri_config::Blur) {
        if self.blur_config == config {
            return;
        }

        self.blur_config = config;

        if let Some(offscreen) = &mut self.offscreen {
            if offscreen.blurred.is_some() {
                offscreen.blurred = None;
                self.commit_counter.increment();
            }
        }
    }

    fn set_elements(&mut self, elements: Vec<OutputRenderElements<GlesRenderer>>) {
        self.elements = Some(elements);
    }

    fn render(&mut self, renderer: &mut GlesRenderer, blur: bool) -> Option<PreparedEffectBuffer> {
        if let Err(err) = self.ensure_rendered(renderer) {
            warn!("error rendering: {err:?}");
            return None;
        };

        // TODO: would be good to be able to render blur entirely on-demand, i.e. in draw(). Though,
        // it's probably not too big a deal? Since this is xray blur, so it'll usually render very
        // infrequently. And currently, it'll render if any window on a visible workspace has blur
        // enabled (rather than any window on any workspace), so that's not entirely bad either.
        let blur = blur && !self.blur_config.off;
        if blur {
            if let Err(err) = self.ensure_rendered_blur(renderer) {
                warn!("error blurring: {err:?}");
                return None;
            }
        }

        let offscreen = self.offscreen.as_ref()?;
        let texture = if blur {
            offscreen.blurred.clone()?
        } else {
            offscreen.texture.clone()
        };
        Some(PreparedEffectBuffer {
            texture,
            renderer_context_id: offscreen.renderer_context_id.clone(),
            scale: offscreen.scale,
            commit: self.commit_counter,
        })
    }

    fn ensure_rendered(&mut self, renderer: &mut GlesRenderer) -> anyhow::Result<()> {
        let Some(elements) = self.elements.take() else {
            // No redrawing necessary.
            return Ok(());
        };

        let _span = tracy_client::span!("EffectBuffer::ensure_rendered");

        // Check if we need to create or recreate the texture.
        let size_string;
        let mut reason = "";
        if let Some(Offscreen {
            texture,
            renderer_context_id,
            ..
        }) = &mut self.offscreen
        {
            let old_size = texture.size();
            if old_size != self.size {
                size_string = format!(
                    "size changed from {} × {} to {} × {}",
                    old_size.w, old_size.h, self.size.w, self.size.h
                );
                reason = &size_string;

                self.offscreen = None;
            } else if !texture.is_unique_reference() {
                reason = "not unique";

                self.offscreen = None;
            } else if *renderer_context_id != renderer.context_id() {
                reason = "renderer id changed";

                self.offscreen = None;
            }
        } else {
            reason = "first render";
        }

        let offscreen = if let Some(offscreen) = &mut self.offscreen {
            offscreen
        } else {
            debug!("creating new texture: {reason}");
            let span = tracy_client::span!("creating effect original buffer");
            span.emit_text(reason);

            let texture: GlesTexture = renderer
                .create_buffer(Fourcc::Abgr8888, self.size)
                .context("error creating texture")?;

            let buffer_size = self.size.to_logical(1, Transform::Normal).to_physical(1);
            let damage = OutputDamageTracker::new(buffer_size, self.scale, Transform::Normal);

            let blur = Shaders::get(renderer).blur.clone().map(Blur::new);

            self.offscreen.insert(Offscreen {
                texture,
                renderer_context_id: renderer.context_id(),
                scale: self.scale,
                damage,
                blur,
                blurred: None,
            })
        };

        // Recreate the damage tracker if the scale changes. We already recreate it for buffer size
        // changes, and transform is always Normal.
        if offscreen.scale != self.scale {
            offscreen.scale = self.scale;

            trace!("recreating damage tracker due to scale change");
            let buffer_size = self.size.to_logical(1, Transform::Normal).to_physical(1);
            offscreen.damage = OutputDamageTracker::new(buffer_size, self.scale, Transform::Normal);
        }

        let res = {
            let mut target = renderer.bind(&mut offscreen.texture)?;
            offscreen
                .damage
                .render_output(renderer, &mut target, 1, &elements, Color32F::TRANSPARENT)
                .context("error rendering")?
        };

        if res.damage.is_some() {
            self.commit_counter.increment();

            // Original texture changed; reset the blurred texture.
            offscreen.blurred = None;
        }

        Ok(())
    }

    fn ensure_rendered_blur(&mut self, renderer: &mut GlesRenderer) -> anyhow::Result<()> {
        let offscreen = self.offscreen.as_mut().context("missing offscreen")?;
        if offscreen.blurred.is_some() {
            // Already rendered.
            return Ok(());
        }

        let Some(blur) = &mut offscreen.blur else {
            // Missing blur shader.
            return Ok(());
        };

        ensure!(
            offscreen.renderer_context_id == renderer.context_id(),
            "wrong renderer context id"
        );

        debug!("rendering blur");
        let texture = blur
            .render(renderer, &offscreen.texture, self.blur_config)
            .context("error rendering blur")?;
        offscreen.blurred = Some(texture);

        Ok(())
    }
}
