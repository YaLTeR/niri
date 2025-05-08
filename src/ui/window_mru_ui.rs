/*
Todo:

- Mru list should contain an Option<BakedBuffer> to cache the texture
  once rendered and then reused as needed.
- Animation. Likely to need a position cache as for Tiles
- Keybindings in the MruUi, e.g. Close window, Quit, Focus selected, prev, next
- Unfocus the current Tile while the MruUi is up and refocus as necessary when
  the UI is closed.

*/
use std::cell::RefCell;
use std::cmp::Ordering;
use std::iter;
use std::ops::ControlFlow;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::output::Output;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use crate::layout::focus_ring::{FocusRing, FocusRingRenderElement};
use crate::layout::LayoutElement;
use crate::niri::{Niri, WindowMRU};
use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::surface::render_snapshot_from_surface_tree;
use crate::render_helpers::texture::TextureBuffer;
use crate::render_helpers::{render_to_texture, BakedBuffer, RenderTarget, ToRenderElement};
use crate::utils::output_size;
use crate::window::mapped::MappedId;

// Space to keep between sides of the output and first thumbnail, or between thumbnails
const SPACING: f64 = 50.;

// Corner radius on focus ring
const RADIUS: f32 = 6.;

// Alpha value for the focus ring
const FOCUS_RING_ALPHA: f32 = 0.9;

pub enum WindowMruUi {
    Closed {},
    Open {
        wmru: WindowMRU,
        focus_ring: RefCell<FocusRing>,
    },
}

niri_render_elements! {
    WindowMruUiRenderElement => {
        SolidColor = SolidColorRenderElement,
        TextureElement = PrimaryGpuTextureRenderElement,
        FocusRing = FocusRingRenderElement,
        // Texture = TextureRenderElement<>,
    }
}

impl WindowMruUi {
    pub fn new() -> Self {
        Self::Closed {}
    }

    pub fn is_open(&self) -> bool {
        matches!(self, WindowMruUi::Open { .. })
    }

    pub fn open(&mut self, wmru: WindowMRU, config: niri_config::FocusRing) {
        let Self::Closed {} = self else { return };

        *self = Self::Open {
            wmru,
            focus_ring: RefCell::new(FocusRing::new(config)),
        };
    }

    pub fn close(&mut self) {
        let Self::Open { .. } = self else { return };
        *self = Self::Closed {};
    }

    pub fn render_output(
        &self,
        niri: &Niri,
        output: &Output,
        _target: RenderTarget,
        renderer: &mut GlesRenderer,
    ) -> Vec<WindowMruUiRenderElement> {
        let _span = tracy_client::span!("WindowMruUi::render_output");

        let Self::Open {
            ref wmru,
            ref focus_ring,
        } = self
        else {
            panic!("render_output on a non-open WindowMruUi");
        };

        let transform = output.current_transform();
        let output_mode = output.current_mode().unwrap();
        let size = transform.transform_size(output_mode.size);
        let scale = output.current_scale().fractional_scale();
        let mut elements = Vec::new();

        let output_size = output_size(output);

        let allowance = output_size.w - 2. * SPACING;

        let (before, after) = wmru.ids.split_at(wmru.current);
        let (current, after) = after.split_at(1);
        let current = current[0];

        let Some(current_texture) = get_texture(niri, renderer, current) else {
            return vec![];
        };
        let mut total_width = current_texture.buffer.logical_size().w;

        // textures that come after/before the "current" id in the Mru list
        let (mut after_textures, mut before_textures) = (vec![], vec![]);

        // define iterators over the mru list in both directions that move away from the current id
        let after_it = after.iter().map(Some).chain(iter::repeat(None));
        let before_it = before.iter().rev().map(Some).chain(iter::repeat(None));

        after_it.zip(before_it).try_for_each(|(a, b)| {
            match (a, b) {
                (None, None) => {
                    // all textures fit in the allowance
                    return ControlFlow::Break(());
                }
                (Some(i), None) | (None, Some(i)) => {
                    if let Some(t) = get_texture(niri, renderer, *i) {
                        total_width += t.buffer.logical_size().w + SPACING;
                        after_textures.push(t);
                    }
                }
                (Some(a), Some(b)) => {
                    if let Some(t) = get_texture(niri, renderer, *a) {
                        total_width += t.buffer.logical_size().w + SPACING;
                        after_textures.push(t);
                    }
                    if let Some(t) = get_texture(niri, renderer, *b) {
                        total_width += t.buffer.logical_size().w + SPACING;
                        before_textures.push(t);
                    }
                }
            }
            if total_width >= allowance {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });

        if total_width <= allowance {
            // all thumbnails fit in the allowance
            let mut location: Point<f64, Logical> =
                Point::from(((output_size.w - total_width) / 2., output_size.h / 2.));
            for t in before_textures.iter().rev() {
                render_elements_for_thumbnail(
                    t,
                    &mut location,
                    true,
                    renderer,
                    None,
                    &mut elements,
                );
            }
            render_elements_for_thumbnail(
                &current_texture,
                &mut location,
                true,
                renderer,
                Some(focus_ring),
                &mut elements,
            );

            for t in after_textures.iter() {
                render_elements_for_thumbnail(
                    t,
                    &mut location,
                    true,
                    renderer,
                    None,
                    &mut elements,
                );
            }
        } else {
            match after_textures.len().cmp(&before_textures.len()) {
                Ordering::Equal => {
                    // fill from the center
                    let center = Point::from((output_size.w / 2., output_size.h / 2.));
                    let mut location =
                        center - Point::from((current_texture.buffer.logical_size().w / 2., 0.));

                    render_elements_for_thumbnail(
                        &current_texture,
                        &mut location,
                        true,
                        renderer,
                        Some(focus_ring),
                        &mut elements,
                    );

                    for t in after_textures {
                        render_elements_for_thumbnail(
                            &t,
                            &mut location,
                            true,
                            renderer,
                            None,
                            &mut elements,
                        );
                    }

                    let mut location = center
                        - Point::from((current_texture.buffer.logical_size().w / 2. + SPACING, 0.));
                    for t in before_textures {
                        render_elements_for_thumbnail(
                            &t,
                            &mut location,
                            false,
                            renderer,
                            None,
                            &mut elements,
                        );
                    }
                }
                Ordering::Less => {
                    // fill from the right
                    let mut location = Point::from((output_size.w - SPACING, output_size.h / 2.));
                    for t in after_textures.iter().rev() {
                        render_elements_for_thumbnail(
                            t,
                            &mut location,
                            false,
                            renderer,
                            None,
                            &mut elements,
                        );
                    }
                    render_elements_for_thumbnail(
                        &current_texture,
                        &mut location,
                        false,
                        renderer,
                        Some(focus_ring),
                        &mut elements,
                    );

                    for t in before_textures {
                        render_elements_for_thumbnail(
                            &t,
                            &mut location,
                            false,
                            renderer,
                            None,
                            &mut elements,
                        );
                    }
                }
                Ordering::Greater => {
                    // fill from the left
                    let mut location = Point::from((SPACING, output_size.h / 2.));
                    for t in before_textures.iter().rev() {
                        render_elements_for_thumbnail(
                            t,
                            &mut location,
                            true,
                            renderer,
                            None,
                            &mut elements,
                        );
                    }
                    render_elements_for_thumbnail(
                        &current_texture,
                        &mut location,
                        true,
                        renderer,
                        Some(focus_ring),
                        &mut elements,
                    );
                    for t in after_textures {
                        render_elements_for_thumbnail(
                            &t,
                            &mut location,
                            true,
                            renderer,
                            None,
                            &mut elements,
                        );
                    }
                }
            }
        }

        // Put a panel above the current View to contrast the thumbnails
        let loc: Point<_, Physical> = Point::from((0, size.h / 16));

        let size: Size<_, Physical> = Size::from((size.w, size.h / 16 * 14));
        let buffer = SolidColorBuffer::new(size.to_f64().to_logical(scale), [0., 0., 0., 0.7]);

        elements.push(
            SolidColorRenderElement::from_buffer(
                &buffer,
                loc.to_f64().to_logical(scale),
                1.0,
                Kind::Unspecified,
            )
            .into(),
        );

        elements
    }
}

fn get_texture(
    niri: &Niri,
    renderer: &mut GlesRenderer,
    id: MappedId,
) -> Option<BakedBuffer<TextureBuffer<GlesTexture>>> {
    niri.layout.windows().find_map(|(_, mapped)| {
        if mapped.id() != id {
            return None;
        }

        let surface = mapped.toplevel().wl_surface();

        // collect contents for the toplevel surface
        let mut contents = vec![];
        render_snapshot_from_surface_tree(renderer, surface, Point::from((0., 0.)), &mut contents);

        // render to a new texture
        let wsz = mapped.window.geometry().to_physical_precise_up(1.);

        render_to_texture(
            renderer,
            wsz.size,
            Scale::from(1.),
            Transform::Normal,
            Fourcc::Abgr8888,
            contents.iter().map(|e| {
                e.to_render_element(
                    mapped.buf_loc().to_f64(),
                    Scale::from(1.0),
                    1.0,
                    Kind::Unspecified,
                )
            }),
        )
        .ok()
        .map(|(texture, _)| {
            let tb = TextureBuffer::from_texture(renderer, texture, 2.0, Transform::Normal, vec![]);

            // wrap the texture into a BakedBuffer
            BakedBuffer {
                buffer: tb,
                location: Point::default(),
                src: None,
                dst: None,
            }
        })
    })
}

fn render_elements_for_thumbnail(
    bb: &BakedBuffer<TextureBuffer<GlesTexture>>,
    location: &mut Point<f64, Logical>,
    forward: bool,
    renderer: &mut GlesRenderer,
    focus_ring: Option<&RefCell<FocusRing>>,
    elements: &mut Vec<WindowMruUiRenderElement>,
) {
    let bb_size = bb.buffer.logical_size();
    if !forward {
        *location -= Point::from((bb_size.w, 0.));
    }

    let render_location = *location - Point::from((0., bb_size.h / 2.));

    elements.push(
        bb.to_render_element(render_location, Scale::from(1.0), 1.0, Kind::Unspecified)
            .into(),
    );
    if let Some(focus_ring) = focus_ring {
        let mut focus_ring = focus_ring.borrow_mut();
        focus_ring.update_render_elements(
            bb_size,
            true,
            true,
            Rectangle::default(), // no effect
            niri_config::CornerRadius {
                top_left: RADIUS,
                top_right: RADIUS,
                bottom_right: RADIUS,
                bottom_left: RADIUS,
            },
            1.0,
            FOCUS_RING_ALPHA,
        );
        elements.extend(focus_ring.render(renderer, render_location).map(Into::into));
    }
    if forward {
        *location += Point::from((SPACING + bb_size.w, 0.));
    } else {
        *location -= Point::from((SPACING, 0.));
    }
}

impl Default for WindowMruUi {
    fn default() -> Self {
        Self::new()
    }
}
