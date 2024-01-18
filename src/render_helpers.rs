use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{
    Bind, ExportMem, ImportAll, ImportMem, Offscreen, Renderer, Texture,
};
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Transform};

use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};

/// Trait with our main renderer requirements to save on the typing.
pub trait NiriRenderer:
    ImportAll
    + ImportMem
    + ExportMem
    + Bind<Dmabuf>
    + Offscreen<GlesTexture>
    + Renderer<TextureId = Self::NiriTextureId, Error = Self::NiriError>
    + AsGlesRenderer
{
    // Associated types to work around the instability of associated type bounds.
    type NiriTextureId: Texture + Clone + 'static;
    type NiriError: std::error::Error
        + Send
        + Sync
        + From<<GlesRenderer as Renderer>::Error>
        + 'static;
}

impl<R> NiriRenderer for R
where
    R: ImportAll + ImportMem + ExportMem + Bind<Dmabuf> + Offscreen<GlesTexture> + AsGlesRenderer,
    R::TextureId: Texture + Clone + 'static,
    R::Error: std::error::Error + Send + Sync + From<<GlesRenderer as Renderer>::Error> + 'static,
{
    type NiriTextureId = R::TextureId;
    type NiriError = R::Error;
}

/// Trait for getting the underlying `GlesRenderer`.
pub trait AsGlesRenderer {
    fn as_gles_renderer(&mut self) -> &mut GlesRenderer;
}

impl AsGlesRenderer for GlesRenderer {
    fn as_gles_renderer(&mut self) -> &mut GlesRenderer {
        self
    }
}

impl<'render, 'alloc> AsGlesRenderer for TtyRenderer<'render, 'alloc> {
    fn as_gles_renderer(&mut self) -> &mut GlesRenderer {
        self.as_mut()
    }
}

/// Trait for getting the underlying `GlesFrame`.
pub trait AsGlesFrame<'frame>
where
    Self: 'frame,
{
    fn as_gles_frame(&mut self) -> &mut GlesFrame<'frame>;
}

impl<'frame> AsGlesFrame<'frame> for GlesFrame<'frame> {
    fn as_gles_frame(&mut self) -> &mut GlesFrame<'frame> {
        self
    }
}

impl<'render, 'alloc, 'frame> AsGlesFrame<'frame> for TtyFrame<'render, 'alloc, 'frame> {
    fn as_gles_frame(&mut self) -> &mut GlesFrame<'frame> {
        self.as_mut()
    }
}

/// Wrapper for a texture from the primary GPU for rendering with the primary GPU.
#[derive(Debug)]
pub struct PrimaryGpuTextureRenderElement(pub TextureRenderElement<GlesTexture>);

impl Element for PrimaryGpuTextureRenderElement {
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

impl RenderElement<GlesRenderer> for PrimaryGpuTextureRenderElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        let gles_frame = frame.as_gles_frame();
        RenderElement::<GlesRenderer>::draw(&self.0, gles_frame, src, dst, damage)?;
        Ok(())
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}

impl<'render, 'alloc> RenderElement<TtyRenderer<'render, 'alloc>>
    for PrimaryGpuTextureRenderElement
{
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render, 'alloc>> {
        let gles_frame = frame.as_gles_frame();
        RenderElement::<GlesRenderer>::draw(&self.0, gles_frame, src, dst, damage)?;
        Ok(())
    }

    fn underlying_storage(
        &self,
        _renderer: &mut TtyRenderer<'render, 'alloc>,
    ) -> Option<UnderlyingStorage> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}
