use std::cell::RefCell;
use std::rc::Rc;

use niri_config::CornerRadius;
use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::Frame as _;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};
use crate::render_helpers::effect_buffer::{EffectBuffer, PreparedEffectBuffer};
use crate::render_helpers::renderer::AsGlesFrame as _;

#[derive(Debug)]
pub struct BackgroundEffect {
    id: Id,
    geometry: Rectangle<f64, Logical>,
    params: Parameters,
    inner: RefCell<Inner>,
}

#[derive(Debug)]
pub struct BackgroundEffectElement {
    id: Id,
    commit: CommitCounter,
    geometry: Rectangle<f64, Logical>,
    buffer: PreparedEffectBuffer,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Parameters {
    pub corner_radius: CornerRadius,
    pub xray: bool,
    pub blur: bool,
}

#[derive(Debug)]
struct Inner {
    commit_counter: CommitCounter,
    buffer: Option<(Rc<EffectBuffer>, Option<CommitCounter>)>,
}

impl Parameters {
    fn is_visible(&self) -> bool {
        self.xray || self.blur
    }
}

impl BackgroundEffect {
    pub fn new() -> Self {
        Self {
            id: Id::new(),
            geometry: Rectangle::zero(),
            params: Parameters::default(),
            inner: RefCell::new(Inner {
                commit_counter: CommitCounter::default(),
                buffer: None,
            }),
        }
    }

    pub fn update_params(&mut self, params: Parameters) {
        if self.params == params {
            return;
        }

        self.params = params;

        let inner = self.inner.get_mut();
        inner.commit_counter.increment();

        if !self.params.is_visible() {
            inner.buffer = None;
        }
    }

    pub fn update_buffer(&mut self, buffer: &Rc<EffectBuffer>) {
        if !self.params.is_visible() {
            return;
        }

        let inner = self.inner.get_mut();

        // Reset the buffer when it changes since we cannot compare commits across different ones.
        let prev = inner.buffer.as_ref();
        if prev.is_none_or(|prev| !Rc::ptr_eq(&prev.0, buffer)) {
            inner.buffer = Some((buffer.clone(), None));
            inner.commit_counter.increment();
        }
    }

    pub fn update_size(&mut self, size: Size<f64, Logical>) {
        self.geometry.size = size;
    }

    pub fn render(&self, renderer: &mut GlesRenderer) -> Option<BackgroundEffectElement> {
        let mut inner = self.inner.borrow_mut();
        // If this effect is invisible, this will return None.
        let (buffer, commit) = inner.buffer.as_mut()?;

        let prepared = buffer.render(renderer, self.params.blur)?;

        // Check if the buffer contents changed.
        if *commit != Some(prepared.commit) {
            *commit = Some(prepared.commit);
            inner.commit_counter.increment();
        }

        Some(BackgroundEffectElement {
            id: self.id.clone(),
            commit: inner.commit_counter,
            geometry: self.geometry,
            buffer: prepared,
        })
    }
}

impl BackgroundEffectElement {
    pub fn with_location(mut self, location: Point<f64, Logical>) -> Self {
        self.geometry.loc = location;
        self
    }
}

impl Element for BackgroundEffectElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        // TODO
        Rectangle::from_size(Size::from((1., 1.)))
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.geometry.to_physical_precise_round(scale)
    }
}

impl RenderElement<GlesRenderer> for BackgroundEffectElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        let buffer = &self.buffer;
        if frame.context_id() != buffer.renderer_context_id {
            warn!("trying to render texture from different renderer");
            return Ok(());
        }

        let tmp = dst.to_f64();
        let src = Rectangle::new(
            Point::new(tmp.loc.x, tmp.loc.y),
            Size::new(tmp.size.w, tmp.size.h),
        );

        frame.render_texture_from_to(
            &buffer.texture,
            src,
            dst,
            damage,
            opaque_regions,
            Transform::Normal,
            1.,
            None,
            &[],
        )
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for BackgroundEffectElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        let gles_frame = frame.as_gles_frame();
        RenderElement::<GlesRenderer>::draw(&self, gles_frame, src, dst, damage, opaque_regions)?;
        Ok(())
    }
}
