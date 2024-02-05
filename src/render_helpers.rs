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

// We need to implement RenderElement manually due to AsGlesFrame requirement.
// This macro does it for us.
#[macro_export]
macro_rules! niri_render_elements {
    ($name:ident => { $($variant:ident = $type:ty),+ $(,)? }) => {
        #[derive(Debug)]
        pub enum $name<R: $crate::render_helpers::NiriRenderer> {
            $($variant($type)),+
        }

        impl<R: $crate::render_helpers::NiriRenderer> smithay::backend::renderer::element::Element for $name<R> {
            fn id(&self) -> &smithay::backend::renderer::element::Id {
                match self {
                    $($name::$variant(elem) => elem.id()),+
                }
            }

            fn current_commit(&self) -> smithay::backend::renderer::utils::CommitCounter {
                match self {
                    $($name::$variant(elem) => elem.current_commit()),+
                }
            }

            fn geometry(&self, scale: smithay::utils::Scale<f64>) -> Rectangle<i32, smithay::utils::Physical> {
                match self {
                    $($name::$variant(elem) => elem.geometry(scale)),+
                }
            }

            fn transform(&self) -> smithay::utils::Transform {
                match self {
                    $($name::$variant(elem) => elem.transform()),+
                }
            }

            fn src(&self) -> smithay::utils::Rectangle<f64, smithay::utils::Buffer> {
                match self {
                    $($name::$variant(elem) => elem.src()),+
                }
            }

            fn damage_since(
                &self,
                scale: smithay::utils::Scale<f64>,
                commit: Option<smithay::backend::renderer::utils::CommitCounter>,
            ) -> Vec<smithay::utils::Rectangle<i32, smithay::utils::Physical>> {
                match self {
                    $($name::$variant(elem) => elem.damage_since(scale, commit)),+
                }
            }

            fn opaque_regions(&self, scale: smithay::utils::Scale<f64>) -> Vec<smithay::utils::Rectangle<i32, smithay::utils::Physical>> {
                match self {
                    $($name::$variant(elem) => elem.opaque_regions(scale)),+
                }
            }

            fn alpha(&self) -> f32 {
                match self {
                    $($name::$variant(elem) => elem.alpha()),+
                }
            }

            fn kind(&self) -> smithay::backend::renderer::element::Kind {
                match self {
                    $($name::$variant(elem) => elem.kind()),+
                }
            }
        }

        impl smithay::backend::renderer::element::RenderElement<smithay::backend::renderer::gles::GlesRenderer> for $name<smithay::backend::renderer::gles::GlesRenderer> {
            fn draw(
                &self,
                frame: &mut smithay::backend::renderer::gles::GlesFrame<'_>,
                src: smithay::utils::Rectangle<f64, smithay::utils::Buffer>,
                dst: smithay::utils::Rectangle<i32, smithay::utils::Physical>,
                damage: &[smithay::utils::Rectangle<i32, smithay::utils::Physical>],
            ) -> Result<(), smithay::backend::renderer::gles::GlesError> {
                match self {
                    $($name::$variant(elem) => {
                        smithay::backend::renderer::element::RenderElement::<smithay::backend::renderer::gles::GlesRenderer>::draw(elem, frame, src, dst, damage)
                    })+
                }
            }

            fn underlying_storage(&self, renderer: &mut smithay::backend::renderer::gles::GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage> {
                match self {
                    $($name::$variant(elem) => elem.underlying_storage(renderer)),+
                }
            }
        }

        impl<'render, 'alloc> smithay::backend::renderer::element::RenderElement<$crate::backend::tty::TtyRenderer<'render, 'alloc>>
            for $name<$crate::backend::tty::TtyRenderer<'render, 'alloc>>
        {
            fn draw(
                &self,
                frame: &mut $crate::backend::tty::TtyFrame<'render, 'alloc, '_>,
                src: smithay::utils::Rectangle<f64, smithay::utils::Buffer>,
                dst: smithay::utils::Rectangle<i32, smithay::utils::Physical>,
                damage: &[smithay::utils::Rectangle<i32, smithay::utils::Physical>],
            ) -> Result<(), $crate::backend::tty::TtyRendererError<'render, 'alloc>> {
                match self {
                    $($name::$variant(elem) => {
                        smithay::backend::renderer::element::RenderElement::<$crate::backend::tty::TtyRenderer<'render, 'alloc>>::draw(elem, frame, src, dst, damage)
                    })+
                }
            }

            fn underlying_storage(
                &self,
                renderer: &mut $crate::backend::tty::TtyRenderer<'render, 'alloc>,
            ) -> Option<smithay::backend::renderer::element::UnderlyingStorage> {
                match self {
                    $($name::$variant(elem) => elem.underlying_storage(renderer)),+
                }
            }
        }

        $(impl<R: $crate::render_helpers::NiriRenderer> From<$type> for $name<R> {
            fn from(x: $type) -> Self {
                Self::$variant(x)
            }
        })+
    };
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
