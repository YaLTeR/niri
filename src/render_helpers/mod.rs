use anyhow::Context;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::{GlesMapping, GlesRenderer, GlesTexture};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{Bind, ExportMem, Frame, Offscreen, Renderer};
use smithay::utils::{Physical, Rectangle, Scale, Size, Transform};

pub mod primary_gpu_texture;
pub mod render_elements;
pub mod renderer;

pub fn render_to_texture(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    fourcc: Fourcc,
    elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
) -> anyhow::Result<(GlesTexture, SyncPoint)> {
    let _span = tracy_client::span!();

    let output_rect = Rectangle::from_loc_and_size((0, 0), size);
    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);

    let texture: GlesTexture = renderer
        .create_buffer(fourcc, buffer_size)
        .context("error creating texture")?;

    renderer
        .bind(texture.clone())
        .context("error binding texture")?;

    let mut frame = renderer
        .render(size, Transform::Normal)
        .context("error starting frame")?;

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

    let sync_point = frame.finish().context("error finishing frame")?;
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
) -> anyhow::Result<()> {
    let _span = tracy_client::span!();

    let output_rect = Rectangle::from_loc_and_size((0, 0), size);

    renderer.bind(dmabuf).context("error binding texture")?;
    let mut frame = renderer
        .render(size, Transform::Normal)
        .context("error starting frame")?;

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

    let _sync_point = frame.finish().context("error finishing frame")?;

    Ok(())
}
