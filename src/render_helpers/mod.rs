use std::ptr;

use anyhow::{ensure, Context};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::{Kind, RenderElement};
use smithay::backend::renderer::gles::{GlesMapping, GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{buffer_dimensions, Bind, ExportMem, Frame, Offscreen, Renderer};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::shm;

use self::primary_gpu_texture::PrimaryGpuTextureRenderElement;

pub mod gradient;
pub mod offscreen;
pub mod primary_gpu_pixel_shader;
pub mod primary_gpu_pixel_shader_with_textures;
pub mod primary_gpu_texture;
pub mod render_elements;
pub mod renderer;
pub mod resize;
pub mod resources;
pub mod shaders;
pub mod snapshot;
pub mod surface;

/// What we're rendering for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    /// Rendering to display on screen.
    Output,
    /// Rendering for a screencast.
    Screencast,
    /// Rendering for any other screen capture.
    ScreenCapture,
}

/// Buffer with location, src and dst.
#[derive(Debug)]
pub struct BakedBuffer<B> {
    pub buffer: B,
    pub location: Point<i32, Logical>,
    pub src: Option<Rectangle<f64, Logical>>,
    pub dst: Option<Size<i32, Logical>>,
}

pub trait ToRenderElement {
    type RenderElement;

    fn to_render_element(
        &self,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        kind: Kind,
    ) -> Self::RenderElement;
}

impl ToRenderElement for BakedBuffer<TextureBuffer<GlesTexture>> {
    type RenderElement = PrimaryGpuTextureRenderElement;

    fn to_render_element(
        &self,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        kind: Kind,
    ) -> Self::RenderElement {
        let elem = TextureRenderElement::from_texture_buffer(
            (location + self.location).to_physical_precise_round(scale),
            &self.buffer,
            Some(alpha),
            self.src,
            self.dst,
            kind,
        );
        PrimaryGpuTextureRenderElement(elem)
    }
}

impl ToRenderElement for BakedBuffer<SolidColorBuffer> {
    type RenderElement = SolidColorRenderElement;

    fn to_render_element(
        &self,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        kind: Kind,
    ) -> Self::RenderElement {
        SolidColorRenderElement::from_buffer(
            &self.buffer,
            (location + self.location)
                .to_physical_precise_round(scale)
                .to_i32_round(),
            scale,
            alpha,
            kind,
        )
    }
}

pub fn render_to_encompassing_texture(
    renderer: &mut GlesRenderer,
    scale: Scale<f64>,
    transform: Transform,
    fourcc: Fourcc,
    elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<(GlesTexture, SyncPoint, Rectangle<i32, Physical>)> {
    let geo = elements
        .iter()
        .map(|ele| ele.geometry(scale))
        .reduce(|a, b| a.merge(b))
        .unwrap_or_default();
    let elements = elements.iter().rev().map(|ele| {
        RelocateRenderElement::from_element(ele, (-geo.loc.x, -geo.loc.y), Relocate::Relative)
    });

    let (texture, sync_point) =
        render_to_texture(renderer, geo.size, scale, transform, fourcc, elements)?;

    Ok((texture, sync_point, geo))
}

pub fn render_to_texture(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
    fourcc: Fourcc,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<(GlesTexture, SyncPoint)> {
    let _span = tracy_client::span!();

    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);

    let texture: GlesTexture = renderer
        .create_buffer(fourcc, buffer_size)
        .context("error creating texture")?;

    renderer
        .bind(texture.clone())
        .context("error binding texture")?;

    let sync_point = render_elements(renderer, size, scale, transform, elements)?;
    Ok((texture, sync_point))
}

pub fn render_and_download(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
    fourcc: Fourcc,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<GlesMapping> {
    let _span = tracy_client::span!();

    let (_, _) = render_to_texture(renderer, size, scale, transform, fourcc, elements)?;

    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);
    let mapping = renderer
        .copy_framebuffer(Rectangle::from_loc_and_size((0, 0), buffer_size), fourcc)
        .context("error copying framebuffer")?;
    Ok(mapping)
}

pub fn render_to_vec(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
    fourcc: Fourcc,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<Vec<u8>> {
    let _span = tracy_client::span!();

    let mapping = render_and_download(renderer, size, scale, transform, fourcc, elements)
        .context("error rendering")?;
    let copy = renderer
        .map_texture(&mapping)
        .context("error mapping texture")?;
    Ok(copy.to_vec())
}

#[cfg(feature = "xdp-gnome-screencast")]
pub fn render_to_dmabuf(
    renderer: &mut GlesRenderer,
    dmabuf: smithay::backend::allocator::dmabuf::Dmabuf,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<SyncPoint> {
    let _span = tracy_client::span!();
    renderer.bind(dmabuf).context("error binding texture")?;
    render_elements(renderer, size, scale, transform, elements)
}

pub fn render_to_shm(
    renderer: &mut GlesRenderer,
    buffer: &WlBuffer,
    scale: Scale<f64>,
    transform: Transform,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<()> {
    let _span = tracy_client::span!();

    let buffer_size = buffer_dimensions(buffer).context("error getting buffer dimensions")?;
    let size = buffer_size.to_logical(1, Transform::Normal).to_physical(1);

    let mapping =
        render_and_download(renderer, size, scale, transform, Fourcc::Argb8888, elements)?;
    let bytes = renderer
        .map_texture(&mapping)
        .context("error mapping texture")?;

    shm::with_buffer_contents_mut(buffer, |shm_buffer, shm_len, buffer_data| {
        ensure!(
            // The buffer prefers pixels in little endian ...
            buffer_data.format == wl_shm::Format::Argb8888
                && buffer_data.stride == size.w * 4
                && buffer_data.height == size.h
                && shm_len as i32 == buffer_data.stride * buffer_data.height,
            "invalid buffer format or size"
        );

        ensure!(bytes.len() == shm_len, "mapped buffer has wrong length");

        unsafe {
            let _span = tracy_client::span!("copy_nonoverlapping");
            ptr::copy_nonoverlapping(bytes.as_ptr(), shm_buffer.cast(), shm_len);
        }

        Ok(())
    })
    .context("expected shm buffer, but didn't get one")?
}

fn render_elements(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<SyncPoint> {
    let transform = transform.invert();
    let output_rect = Rectangle::from_loc_and_size((0, 0), transform.transform_size(size));

    let mut frame = renderer
        .render(size, transform)
        .context("error starting frame")?;

    frame
        .clear([0., 0., 0., 0.], &[output_rect])
        .context("error clearing")?;

    for element in elements {
        let src = element.src();
        let dst = element.geometry(scale);

        if let Some(mut damage) = output_rect.intersection(dst) {
            damage.loc -= dst.loc;
            element
                .draw(&mut frame, src, dst, &[damage])
                .context("error drawing element")?;
        }
    }

    frame.finish().context("error finishing frame")
}
