use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::{import_surface, RendererSurfaceStateUserData};
use smithay::backend::renderer::Renderer as _;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Physical, Point, Scale};
use smithay::wayland::compositor::{with_surface_tree_downward, TraversalAction};

use super::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use super::renderer::NiriRenderer;
use crate::layout::{LayoutElementRenderElement, LayoutElementSnapshotRenderElements};

/// Renders elements from a surface tree, as well as saves them as textures into `storage`.
///
/// Saved textures are based at (0, 0) to facilitate later offscreening. This is why the location
/// argument is split into `location` and `offset`: the former is ignored for saved textures, but
/// the latter isn't (for things like popups).
#[allow(clippy::too_many_arguments)]
pub fn render_and_save_from_surface_tree<R: NiriRenderer>(
    renderer: &mut R,
    surface: &WlSurface,
    location: Point<f64, Physical>,
    offset: Point<f64, Physical>,
    scale: Scale<f64>,
    alpha: f32,
    kind: Kind,
    elements: &mut Vec<LayoutElementRenderElement<R>>,
    storage: &mut Option<&mut Vec<LayoutElementSnapshotRenderElements>>,
) {
    let _span = tracy_client::span!("render_and_save_from_surface_tree");

    let base_pos = location;

    with_surface_tree_downward(
        surface,
        location + offset,
        |_, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                let data = &*data.borrow();

                if let Some(view) = data.view() {
                    location += view.offset.to_f64().to_physical(scale);
                    TraversalAction::DoChildren(location)
                } else {
                    TraversalAction::SkipChildren
                }
            } else {
                TraversalAction::SkipChildren
            }
        },
        |surface, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                if let Some(view) = data.borrow().view() {
                    location += view.offset.to_f64().to_physical(scale);
                } else {
                    return;
                }

                let elem = match WaylandSurfaceRenderElement::from_surface(
                    renderer, surface, states, location, alpha, kind,
                ) {
                    Ok(elem) => elem,
                    Err(err) => {
                        warn!("failed to import surface: {err:?}");
                        return;
                    }
                };

                elements.push(elem.into());

                if let Some(storage) = storage {
                    let renderer = renderer.as_gles_renderer();
                    // FIXME (possibly in Smithay): this causes a re-upload for shm textures.
                    if let Err(err) = import_surface(renderer, states) {
                        warn!("failed to import surface: {err:?}");
                        return;
                    }

                    let data = data.borrow();
                    let view = data.view().unwrap();
                    let Some(texture) = data.texture::<GlesRenderer>(renderer.id()) else {
                        return;
                    };

                    let buffer = TextureBuffer::from_texture(
                        renderer,
                        texture.clone(),
                        data.buffer_scale(),
                        data.buffer_transform(),
                        None,
                    );

                    let elem = TextureRenderElement::from_texture_buffer(
                        location - base_pos,
                        &buffer,
                        Some(alpha),
                        Some(view.src),
                        Some(view.dst),
                        kind,
                    );

                    storage.push(PrimaryGpuTextureRenderElement(elem).into());
                }
            }
        },
        |_, _, _| true,
    );
}
