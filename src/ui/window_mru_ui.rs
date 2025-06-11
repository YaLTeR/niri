/*
Todo:

- Add test cases
- Animation. Likely to need a position cache as is done for Tiles
- Transition when wrapping around during Mru navigation(?)
- support clicking on the target thumbnail
x add title of the current Mru selection under the thumbnail
x change BakedBuffers to TextureBuffers
x add bindings in the UI to switch to Output or Workspace modes
- add a help panel in the UI listing key bindings (e.g. screenshot UI)
x in UI, left/right should not change the current mode
x "advance" bindings for the MruUI should be copied over from the general
  bindings for the same action.
x support only considering windows from current output/workspace
x support only considering windows from the currently selected application
x support switching navigation modes while the Mru UI is open
x Unfocus the current Tile while the MruUi is up and refocus as necessary when
  the UI is closed.
x Keybindings in the MruUi, e.g. Close window, Quit, Focus selected, prev, next
x Mru list should contain an Option<BakedBuffer> to cache the texture
  once rendered and then reused as needed.
x Transition when opening/closing MruUI

*/
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::str::FromStr;
use std::time::Instant;
use std::{cmp, iter, mem};

use niri_config::{
    Action, Bind, Key, Match, Modifiers, MruDirection, MruFilter, MruScope, RegexEq, Trigger,
};
use pango::{Alignment, EllipsizeMode, FontDescription};
use pangocairo::cairo::{self, ImageSurface};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::Color32F;
use smithay::input::keyboard::Keysym;
use smithay::output::Output;
use smithay::utils::{Coordinate, Logical, Point, Rectangle, Scale, Size, Transform};

use crate::layout::focus_ring::{FocusRing, FocusRingRenderElement};
use crate::layout::LayoutElement;
use crate::niri::Niri;
use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::surface::render_snapshot_from_surface_tree;
use crate::render_helpers::texture::TextureBuffer;
use crate::render_helpers::{render_to_texture, BakedBuffer, RenderTarget, ToRenderElement};
use crate::utils::{output_size, to_physical_precise_round, with_toplevel_role};
use crate::window::mapped::MappedId;
use crate::window::{window_matches, Mapped, WindowRef};

// Factor by which to scale window thumbnails
const THUMBNAIL_SCALE: f64 = 2.;

// Space to keep between sides of the output and first/last thumbnail, or between thumbnails
const SPACING: f64 = 50.;

// Corner radius on focus ring
const RADIUS: f32 = 6.;
// Alpha value for the focus ring
const FOCUS_RING_ALPHA: f32 = 0.9;

// Background color for the UI
const BACKGROUND: Color32F = Color32F::new(0., 0., 0., 0.7);

// Transition delay in ms for MRU UI open/close and wrap-arounds
pub const MRU_UI_TRANSITION_DELAY: u16 = 20;

// Font used to render window titles
const FONT: &str = "sans 14px";

/// Window MRU traversal context.
#[derive(Debug)]
pub struct WindowMru {
    /// List of window ids to be traversed in MRU order.
    ids: Vec<(MappedId, Option<Instant>)>,

    /// Current index in the MRU traversal.
    current: usize,

    /// Scope used to generate the window list.
    scope: MruScope,

    /// Filter used to generte the window list.
    filter: MruFilter,

    /// Traversal direction when the WindowMru was created.
    direction: MruDirection,
}

impl WindowMru {
    pub fn new(
        niri: &Niri,
        direction: MruDirection,
        scope: Option<MruScope>,
        filter: Option<MruFilter>,
    ) -> Self {
        let scope = scope.unwrap_or_default();
        let filter = filter.unwrap_or_default();

        // todo: maybe using a `Match` is overkill here and a plain app_id
        // compare would suffice
        let window_match = filter.to_match(niri);

        // Build a list of MappedId from the requested scope sorted by timestamp
        let mut ids: Vec<(MappedId, Option<Instant>)> = scope
            .windows(niri)
            .filter(|w| {
                window_match.as_ref().is_none_or(|m| {
                    with_toplevel_role(w.toplevel(), |r| window_matches(WindowRef::Mapped(w), r, m))
                })
            })
            .map(|w| (w.id(), w.get_focus_timestamp()))
            .collect();
        ids.sort_by(|(_, t1), (_, t2)| match (t1, t2) {
            (None, None) => cmp::Ordering::Equal,
            (Some(_), None) => cmp::Ordering::Less,
            (None, Some(_)) => cmp::Ordering::Greater,
            (Some(t1), Some(t2)) => t1.cmp(t2).reverse(),
        });

        if direction == MruDirection::Backward && !ids.is_empty() {
            // If moving backwards through the list, the first element is moved to the end of the
            // list
            let first = ids.remove(0);
            ids.push(first);
        }

        match direction {
            MruDirection::Forward => {
                let mut res = Self {
                    ids,
                    current: 0,
                    scope,
                    filter,
                    direction,
                };
                res.forward();
                res
            }
            MruDirection::Backward => {
                let current = ids.len().saturating_sub(1);
                let mut res = Self {
                    ids,
                    current,
                    scope,
                    filter,
                    direction,
                };
                res.backward();
                res
            }
        }
    }

    pub fn forward(&mut self) {
        self.current = if self.ids.is_empty() {
            0
        } else {
            (self.current + 1) % self.ids.len()
        }
    }

    pub fn backward(&mut self) {
        self.current = self.current.checked_sub(1).unwrap_or(self.ids.len() - 1)
    }

    pub fn get_id(&self, index: usize) -> MappedId {
        self.ids[index].0
    }
}

type MruTexture = TextureBuffer<GlesTexture>;

pub enum WindowMruUi {
    Closed {},
    Open {
        wmru: WindowMru,
        textures: RefCell<TextureCache>,
        focus_ring: Box<RefCell<FocusRing>>,
    },
}

pub trait ToMatch {
    fn to_match(&self, niri: &Niri) -> Option<Match>;
}

impl ToMatch for MruFilter {
    fn to_match(&self, niri: &Niri) -> Option<Match> {
        match self {
            MruFilter::None => None,
            MruFilter::AppId => {
                let app_id = {
                    if niri.window_mru_ui.is_open() {
                        // When the MRU UI is already open, use the currently
                        // selected MRU thumbnail's app_id for the match
                        let id = niri.window_mru_ui.current_window_id()?;
                        let w = niri.find_window_by_id(id)?;
                        with_toplevel_role(w.toplevel()?, |r| r.app_id.clone())
                    } else {
                        let toplevel = niri.layout.active_workspace()?.active_window()?.toplevel();
                        with_toplevel_role(toplevel, |r| r.app_id.clone())
                    }?
                };

                Some(Match {
                    app_id: Some(RegexEq::from_str(&format!("^{}$", app_id)).ok()?),
                    ..Default::default()
                })
            }
        }
    }
}

pub trait ToWindowIterator {
    fn windows<'a>(&self, niri: &'a Niri) -> impl Iterator<Item = &'a Mapped>;
}

impl ToWindowIterator for MruScope {
    fn windows<'a>(&self, niri: &'a Niri) -> impl Iterator<Item = &'a Mapped> {
        // gather windows based on the requested scope
        match self {
            MruScope::All => Box::new(niri.layout.windows().map(|(_, w)| w)),
            MruScope::Output => {
                if let Some(active_output) = niri.layout.active_output() {
                    Box::new(niri.layout.windows().filter_map(move |(m, w)| {
                        if let Some(monitor) = m {
                            if monitor.output() == active_output {
                                return Some(w);
                            }
                        }
                        None
                    }))
                } else {
                    Box::new(iter::empty()) as Box<dyn Iterator<Item = &Mapped>>
                }
            }
            MruScope::Workspace => niri
                .layout
                .active_workspace()
                .map(|wkspc| Box::new(wkspc.windows()) as Box<dyn Iterator<Item = &Mapped>>)
                .unwrap_or(Box::new(iter::empty())),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum MruAlign {
    Center,
    Right,
    Left,
}

niri_render_elements! {
    WindowMruUiRenderElement => {
        SolidColor = SolidColorRenderElement,
        TextureElement = PrimaryGpuTextureRenderElement,
        FocusRing = FocusRingRenderElement,
    }
}

impl WindowMruUi {
    pub fn new() -> Self {
        Self::Closed {}
    }

    pub fn is_open(&self) -> bool {
        matches!(self, WindowMruUi::Open { .. })
    }

    pub fn open(&mut self, config: &niri_config::Config, wmru: WindowMru) {
        let Self::Closed {} = self else { return };
        let nids = wmru.ids.len();
        *self = Self::Open {
            wmru,
            textures: RefCell::new(TextureCache::with_capacity(nids)),
            focus_ring: Box::new(RefCell::new(FocusRing::new(config.layout.focus_ring))),
        };
    }

    pub fn close(&mut self) {
        let Self::Open { .. } = self else { return };
        *self = Self::Closed {};
    }

    pub fn advance(&mut self, dir: MruDirection) {
        let Self::Open { wmru, .. } = self else {
            return;
        };
        match dir {
            MruDirection::Forward => wmru.forward(),
            MruDirection::Backward => wmru.backward(),
        }
    }

    pub fn derive_new_mru_list(
        &self,
        niri: &Niri,
        scope: Option<MruScope>,
        filter: Option<MruFilter>,
    ) -> Option<WindowMru> {
        let Self::Open { wmru, .. } = self else {
            return None;
        };

        if scope.is_some_and(|s| s != wmru.scope) || filter.is_some_and(|f| f != wmru.filter) {
            Some(WindowMru::new(niri, wmru.direction, scope, filter))
        } else {
            None
        }
    }

    pub fn update_mru_list(&mut self, dir: Option<MruDirection>, mut wmru: WindowMru) {
        let Self::Open {
            wmru: ref mut prev_wmru,
            textures,
            ..
        } = self
        else {
            return;
        };
        // Try to set the `current` field in the new wmru to match the one
        // from the previous mru.
        if let Some(current_selection) = wmru.ids.get(wmru.current) {
            if let Some(current_in_new) = wmru
                .ids
                .iter()
                .position(|(i, t)| *i == current_selection.0 || *t < current_selection.1)
            {
                wmru.current = current_in_new
            }
        }

        // If the current Mru selection is present in both the previous Mru list
        // and in the replacement list, then we should advance in the requested
        // direction to avoid current staying unchanged after a user action.
        let should_advance = wmru.ids.get(wmru.current) == prev_wmru.ids.get(prev_wmru.current);

        // Retain textures from the TextureCache that match window Ids from
        // the updated MruList.
        textures.replace_with(|v| {
            let mut start_pos = 0;
            let textures = wmru
                .ids
                .iter()
                .map(|(id, t)| {
                    let mut tile_texture = Default::default();
                    if let Some(index) = prev_wmru
                        .ids
                        .iter()
                        .skip(start_pos)
                        .take_while(|(_, pt)| t >= pt)
                        .position(|(pid, _)| id == pid)
                    {
                        let adjusted_idx = index + start_pos;
                        start_pos = adjusted_idx;
                        mem::swap(&mut tile_texture, &mut v.0[adjusted_idx]);
                    }
                    tile_texture
                })
                .collect();
            TextureCache(textures)
        });

        // Replace the UI's WindowMru.
        std::mem::swap(&mut wmru, prev_wmru);

        // And (possibly) advance in the requested direction.
        if should_advance {
            if let Some(dir) = dir {
                self.advance(dir);
            }
        }
    }

    pub fn first(&mut self) {
        let Self::Open { wmru, .. } = self else {
            return;
        };
        wmru.current = 0;
    }

    pub fn last(&mut self) {
        let Self::Open { wmru, .. } = self else {
            return;
        };
        wmru.current = wmru.ids.len().saturating_sub(1);
    }

    pub fn current_window_id(&self) -> Option<MappedId> {
        let Self::Open { wmru, .. } = self else {
            return None;
        };
        if wmru.ids.is_empty() {
            None
        } else {
            wmru.ids.get(wmru.current).map(|(m, _)| m).copied()
        }
    }

    pub fn remove_window(&mut self, id: MappedId) {
        let Self::Open { wmru, textures, .. } = self else {
            return;
        };
        if let Some(idx) = wmru.ids.iter().position(|v| v.0 == id) {
            wmru.ids.remove(idx);
            if wmru.current >= wmru.ids.len() {
                wmru.current = wmru.current.saturating_sub(1);
            }
            textures.borrow_mut().get_mut().0.remove(idx);
        }
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
            ref textures,
            ref focus_ring,
            ..
        } = self
        else {
            panic!("render_output on a non-open WindowMruUi");
        };

        let mut elements = Vec::new();
        let output_size = output_size(output);

        if !wmru.ids.is_empty() {
            let allowance = output_size.w - 2. * SPACING;

            let current = wmru.current;
            let mut textures = textures.borrow_mut();
            let current_texture_width = {
                if let Some(t) =
                    textures
                        .get_mut(current)
                        .get_thumbnail(niri, renderer, wmru.get_id(current))
                {
                    t.logical_size().w
                } else {
                    return vec![];
                }
            };
            let mut total_width = current_texture_width;

            // define iterators over the mru list that move away from the "current" element in the
            // MRU list
            let after_it = (current + 1..wmru.ids.len())
                .map(Some)
                .chain(iter::repeat(None));
            let before_it = (0..current).rev().map(Some).chain(iter::repeat(None));

            // the texture cache gets updated for all textures that fit within the allowance
            let (align, left, right) = match after_it.zip(before_it).try_fold(
                (MruAlign::Center, current, current),
                |(align, l, r), (a, b)| {
                    let (align, l, r) = match (a, b) {
                        (None, None) => {
                            // all textures fit in the allowance
                            return ControlFlow::Break((MruAlign::Left, l, r));
                        }
                        (Some(a), None) => {
                            if let Some(t) = textures.borrow_mut().get_mut(a).get_thumbnail(
                                niri,
                                renderer,
                                wmru.get_id(a),
                            ) {
                                total_width += t.logical_size().w + SPACING;
                            }
                            (MruAlign::Left, l, a)
                        }
                        (None, Some(b)) => {
                            if let Some(t) = textures.borrow_mut().get_mut(b).get_thumbnail(
                                niri,
                                renderer,
                                wmru.get_id(b),
                            ) {
                                total_width += t.logical_size().w + SPACING;
                            }
                            (MruAlign::Right, b, r)
                        }
                        (Some(a), Some(b)) => {
                            if let Some(t) = textures.borrow_mut().get_mut(a).get_thumbnail(
                                niri,
                                renderer,
                                wmru.get_id(a),
                            ) {
                                total_width += t.logical_size().w + SPACING;
                            }
                            if let Some(t) = textures.borrow_mut().get_mut(b).get_thumbnail(
                                niri,
                                renderer,
                                wmru.get_id(b),
                            ) {
                                total_width += t.logical_size().w + SPACING;
                            }
                            (align, b, a)
                        }
                    };
                    if total_width >= allowance {
                        ControlFlow::Break((align, l, r))
                    } else {
                        ControlFlow::Continue((align, l, r))
                    }
                },
            ) {
                c @ ControlFlow::Continue(_) => c.continue_value().unwrap(),
                b @ ControlFlow::Break(_) => b.break_value().unwrap(),
            };

            match align {
                MruAlign::Left => {
                    let mut location: Point<f64, Logical> = if total_width <= allowance {
                        Point::from(((output_size.w - total_width) / 2., output_size.h / 2.))
                    } else {
                        Point::from((SPACING, output_size.h / 2.))
                    };
                    for idx in left..=right {
                        let tile_textures = textures.get_mut(idx);
                        if let Some(t) =
                            tile_textures.get_thumbnail(niri, renderer, wmru.get_id(idx))
                        {
                            let title_texture = (idx == current)
                                .then(|| {
                                    tile_textures.get_title(
                                        niri,
                                        renderer,
                                        wmru.get_id(idx),
                                        t.logical_size().to_physical(1.).to_i32_round().w,
                                    )
                                })
                                .flatten();

                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                true,
                                renderer,
                                (idx == current).then_some(focus_ring),
                                title_texture,
                                &mut elements,
                            );
                        }
                    }
                }
                MruAlign::Center => {
                    // fill from the center
                    let center = Point::from((output_size.w / 2., output_size.h / 2.));
                    let mut location = center - Point::from((current_texture_width / 2., 0.));

                    for idx in current..=right {
                        let tile_textures = textures.get_mut(idx);
                        if let Some(t) =
                            tile_textures.get_thumbnail(niri, renderer, wmru.get_id(idx))
                        {
                            let title_texture = (idx == current)
                                .then(|| {
                                    tile_textures.get_title(
                                        niri,
                                        renderer,
                                        wmru.get_id(idx),
                                        t.logical_size().to_physical(1.).to_i32_round().w,
                                    )
                                })
                                .flatten();
                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                true,
                                renderer,
                                (idx == current).then_some(focus_ring),
                                title_texture,
                                &mut elements,
                            );
                        }
                    }

                    let mut location =
                        center - Point::from((current_texture_width / 2. + SPACING, 0.));
                    for idx in (left..current).rev() {
                        let tile_textures = textures.get_mut(idx);
                        if let Some(t) =
                            tile_textures.get_thumbnail(niri, renderer, wmru.get_id(idx))
                        {
                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                false,
                                renderer,
                                None,
                                None,
                                &mut elements,
                            );
                        }
                    }
                }
                MruAlign::Right => {
                    // fill from the right
                    let mut location = Point::from((output_size.w - SPACING, output_size.h / 2.));

                    for idx in (left..=right).rev() {
                        let tile_textures = textures.get_mut(idx);
                        if let Some(t) =
                            tile_textures.get_thumbnail(niri, renderer, wmru.get_id(idx))
                        {
                            let title_texture = (idx == current)
                                .then(|| {
                                    tile_textures.get_title(
                                        niri,
                                        renderer,
                                        wmru.get_id(idx),
                                        t.logical_size().to_physical(1.).to_i32_round().w,
                                    )
                                })
                                .flatten();
                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                false,
                                renderer,
                                (idx == current).then_some(focus_ring),
                                title_texture,
                                &mut elements,
                            );
                        }
                    }
                }
            }
        }
        // Put a panel above the current View to contrast the thumbnails
        let size = Size::from((output_size.w, output_size.h / 16. * 14.));
        let buffer = SolidColorBuffer::new(size, BACKGROUND);

        elements.push(
            SolidColorRenderElement::from_buffer(
                &buffer,
                Point::from((0., output_size.h / 16.)),
                1.0,
                Kind::Unspecified,
            )
            .into(),
        );

        elements
    }
}

impl Default for WindowMruUi {
    fn default() -> Self {
        Self::new()
    }
}

fn render_elements_for_thumbnail(
    thumbnail_texture: MruTexture,
    location: &mut Point<f64, Logical>,
    forward: bool,
    renderer: &mut GlesRenderer,
    focus_ring: Option<&RefCell<FocusRing>>,
    title_texture: Option<MruTexture>,
    elements: &mut Vec<WindowMruUiRenderElement>,
) {
    let texture_size = thumbnail_texture.logical_size();
    if !forward {
        *location -= Point::from((texture_size.w, 0.));
    }

    let render_location = *location - Point::from((0., texture_size.h / 2.));

    elements.push({
        let bb = BakedBuffer {
            buffer: thumbnail_texture,
            location: Point::default(),
            src: None,
            dst: None,
        };

        bb.to_render_element(render_location, Scale::from(1.0), 1.0, Kind::Unspecified)
            .into()
    });
    if let Some(focus_ring) = focus_ring {
        let mut focus_ring = focus_ring.borrow_mut();
        focus_ring.update_render_elements(
            texture_size,
            true,
            true,
            false,
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

    if let Some(title_texture) = title_texture {
        let location = *location
            + Point::from((
                texture_size
                    .w
                    .saturating_sub(title_texture.logical_size().w)
                    / 2.,
                (SPACING + texture_size.h) / 2.,
            ));
        let bb = BakedBuffer {
            buffer: title_texture,
            location: Point::default(),
            src: None,
            dst: None,
        };
        elements.push(
            bb.to_render_element(location, 1.0.into(), 1.0, Kind::Unspecified)
                .into(),
        );
    }

    if forward {
        *location += Point::from((SPACING + texture_size.w, 0.));
    } else {
        *location -= Point::from((SPACING, 0.));
    }
}

#[derive(Default)]
struct MruUiTileTextures {
    thumbnail: Option<MruTexture>,
    title: Option<MruTexture>,
}

impl MruUiTileTextures {
    fn get_thumbnail(
        &mut self,
        niri: &Niri,
        renderer: &mut GlesRenderer,
        mid: MappedId,
    ) -> Option<MruTexture> {
        if self.thumbnail.is_none() {
            self.thumbnail = niri.layout.windows().find_map(|(_, mapped)| {
                if mapped.id() != mid {
                    return None;
                }

                let surface = mapped.toplevel().wl_surface();

                // collect contents for the toplevel surface
                let mut contents = vec![];
                render_snapshot_from_surface_tree(
                    renderer,
                    surface,
                    Point::from((0., 0.)),
                    &mut contents,
                );

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
                    TextureBuffer::from_texture(
                        renderer,
                        texture,
                        THUMBNAIL_SCALE,
                        Transform::Normal,
                        vec![],
                    )
                })
            });
        }
        // TextureBuffer is an Arc, so cloning is cheap
        self.thumbnail.clone()
    }

    fn get_title(
        &mut self,
        niri: &Niri,
        renderer: &mut GlesRenderer,
        mid: MappedId,
        width: i32,
    ) -> Option<MruTexture> {
        if self.title.is_none() {
            self.title = get_window_title_by_id(niri, mid)
                .and_then(|title| generate_title_texture(&title, renderer, width).ok());
        }
        // TextureBuffer is an Arc, so cloning is cheap
        self.title.clone()
    }
}

pub struct TextureCache(Vec<MruUiTileTextures>);

impl TextureCache {
    fn with_capacity(size: usize) -> Self {
        let mut textures = Vec::with_capacity(size);
        textures.resize_with(size, Default::default);
        Self(textures)
    }

    fn get_mut(&mut self, index: usize) -> &mut MruUiTileTextures {
        &mut self.0[index]
    }
}

fn get_window_title_by_id(niri: &Niri, id: MappedId) -> Option<String> {
    niri.layout.windows().find_map(|(_, mapped)| {
        (mapped.id() == id).then(|| with_toplevel_role(mapped.toplevel(), |r| r.title.clone()))?
    })
}

fn generate_title_texture(
    title: &str,
    renderer: &mut GlesRenderer,
    width: i32,
) -> anyhow::Result<MruTexture> {
    let mut font = FontDescription::from_string(FONT);
    let font_size = to_physical_precise_round(1.0, font.size());
    font.set_absolute_size(font_size);

    // Create an initial surface to determine the font height
    let surface = ImageSurface::create(cairo::Format::ARgb32, 0, 0)?;
    let cr = cairo::Context::new(&surface)?;
    let layout = pangocairo::functions::create_layout(&cr);
    layout.set_font_description(Some(&font));
    layout.set_alignment(Alignment::Left);
    layout.set_markup(title);

    // Use the initial surface to determine the height of the final surface
    let (text_width, height) = layout.pixel_size();

    // apply a gradient to the end of the text to avoid a weird cut-off
    let (width, apply_gradient) = if text_width > width {
        (width, true)
    } else {
        (text_width, false)
    };

    // Create a second surface with the final dimensions
    let surface = ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    let cr = cairo::Context::new(&surface)?;
    let layout = pangocairo::functions::create_layout(&cr);
    layout.set_font_description(Some(&font));
    layout.set_ellipsize(EllipsizeMode::End);
    layout.set_alignment(Alignment::Center);
    layout.set_text(title);

    // set a transparent background
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    cr.paint()?;

    // render the title
    cr.set_source_rgb(1., 1., 1.);
    pangocairo::functions::show_layout(&cr, &layout);

    // apply overflow gradient if needed
    if apply_gradient {
        let gradient = cairo::LinearGradient::new(0., 0., width as f64, 0.);
        gradient.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 1.0); // fully opaque
        gradient.add_color_stop_rgba(0.9, 1.0, 1.0, 1.0, 1.0); // fully opaque
        gradient.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0); // fade to transparent

        // Use destination-in to mask the content with the gradient
        cr.set_operator(cairo::Operator::DestIn);
        cr.rectangle(0., 0., width as f64, height as f64);
        cr.set_source(&gradient)?;
        cr.fill()?;
    }

    // release the cr so we can access the surface content
    drop(cr);

    // Convert the the pango surface to a TextureBuffer
    let data = surface.take_data()?;
    let buffer = TextureBuffer::from_memory(
        renderer,
        &data,
        Fourcc::Argb8888,
        (width, height),
        false,
        1.0,
        Transform::Normal,
        Vec::new(),
    )?;
    Ok(buffer)
}

/// Key bindings available when the MRU UI is open.
/// Because the UI is closed when the Alt key is released, all bindings
/// have the ALT modifier.
pub const MRU_UI_BINDINGS: &[Bind] = &[
    // Escape just closes the MRU UI
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Escape),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruCancel,
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    // Left and Right can also be used when the UI is open
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Right),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruAdvance(MruDirection::Forward, None, None),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Left),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruAdvance(MruDirection::Backward, None, None),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    // j and k can be used as well
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::j),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruAdvance(MruDirection::Forward, None, None),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::k),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruAdvance(MruDirection::Backward, None, None),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    // And q can be used to close windows during navigation
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::q),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruCloseCurrent,
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Return),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruClose,
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Home),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruFirst,
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::End),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruLast,
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::a),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruChangeScope(MruScope::All),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::w),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruChangeScope(MruScope::Workspace),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::o),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruChangeScope(MruScope::Output),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
];
