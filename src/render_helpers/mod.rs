use std::ptr;

use anyhow::{ensure, Context};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::{GlesMapping, GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{buffer_dimensions, Bind, ExportMem, Frame, Offscreen, Renderer};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Physical, Rectangle, Scale, Size, Transform};
use smithay::wayland::shm;

pub mod gradient;
pub mod offscreen;
pub mod primary_gpu_pixel_shader;
pub mod primary_gpu_texture;
pub mod render_elements;
pub mod renderer;
pub mod shaders;

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

    let (_, sync_point) = render_to_texture(renderer, size, scale, transform, fourcc, elements)?;
    sync_point.wait();

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
