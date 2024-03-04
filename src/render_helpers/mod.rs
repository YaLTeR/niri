use anyhow::Context;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::{
    self, GlesMapping, GlesRenderbuffer, GlesRenderer, GlesTexture,
};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{Bind, ExportMem, Frame, Offscreen, Renderer};
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

pub fn render_to_texture(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
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

    let sync_point = render_elements(renderer, scale, size, elements)?;
    Ok((texture, sync_point))
}

pub fn render_and_download(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    fourcc: Fourcc,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<GlesMapping> {
    let _span = tracy_client::span!();

    let (_, sync_point) = render_to_texture(renderer, size, scale, fourcc, elements)?;
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
    fourcc: Fourcc,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<Vec<u8>> {
    let _span = tracy_client::span!();

    let mapping =
        render_and_download(renderer, size, scale, fourcc, elements).context("error rendering")?;
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
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<SyncPoint> {
    let _span = tracy_client::span!();
    renderer.bind(dmabuf).context("error binding texture")?;
    render_elements(renderer, scale, size, elements)
}

pub fn render_to_shm(
    renderer: &mut GlesRenderer,
    buffer: &WlBuffer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<()> {
    let buffer_size = smithay::backend::renderer::buffer_dimensions(buffer).unwrap();
    let offscreen_buffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, buffer_size)
        .context("error creating renderbuffer")?;
    renderer
        .bind(offscreen_buffer)
        .context("error binding renderbuffer")?;

    let sync_point = render_elements(renderer, scale, size, elements)?;

    shm::with_buffer_contents_mut(buffer, |shm_buffer, shm_len, buffer_data| {
        anyhow::ensure!(
            // The buffer prefers pixels in little endian ...
            buffer_data.format == wl_shm::Format::Argb8888
                && buffer_data.stride == size.w * 4
                && buffer_data.height == size.h
                && shm_len as i32 == buffer_data.stride * buffer_data.height,
            "invalid buffer format or size"
        );

        renderer.with_context(|gl| unsafe {
            gl.ReadPixels(
                0,
                0,
                size.w,
                size.h,
                // ... but OpenGL prefers them in big endian.
                gles::ffi::BGRA_EXT,
                gles::ffi::UNSIGNED_BYTE,
                shm_buffer.cast(),
            )
        })?;

        // gl.ReadPixels already waits for the rendering to finish.
        // as such, the SyncPoint is already reached.
        // and we needn't wait for it again.
        debug_assert!(sync_point.is_reached());

        Ok(())
    })
    .context("expected shm buffer, but didn't get one")?
}

pub fn render_to_shm_alt(
    renderer: &mut GlesRenderer,
    buffer: &WlBuffer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<()> {
    let mapping = render_and_download(renderer, size, scale, Fourcc::Argb8888, elements)?;
    let bytes = renderer
        .map_texture(&mapping)
        .context("error mapping texture")?;

    shm::with_buffer_contents_mut(buffer, |shm_buffer, shm_len, buffer_data| {
        anyhow::ensure!(
            // The buffer prefers pixels in little endian ...
            buffer_data.format == wl_shm::Format::Argb8888
                && buffer_data.stride == size.w * 4
                && buffer_data.height == size.h
                && shm_len as i32 == buffer_data.stride * buffer_data.height,
            "invalid buffer format or size"
        );

        debug!("copying {} bytes to shm buffer", bytes.len());
        debug!("shm buffer size: {}", shm_len);

        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                shm_buffer.cast(),
                bytes.len().min(shm_len),
            );
        }

        Ok(())
    })
    .context("expected shm buffer, but didn't get one")?
}

fn render_elements(
    renderer: &mut GlesRenderer,
    scale: Scale<f64>,
    size: Size<i32, Physical>,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<SyncPoint> {
    let output_rect = Rectangle::from_loc_and_size((0, 0), size);

    let mut frame = renderer
        .render(size, Transform::Normal)
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
