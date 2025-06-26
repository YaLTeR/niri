/*
Todo:

- Add test cases
- Animations
  x navigation scrolling
  x thumbnails appearing/disappearing
  x reorganization on scope/filter change
  - animate transition from selecting a thumbnail to the focused window
  - Transition when wrapping around during Mru navigation(?)
  x UI open/close animation
- shortcut to "summon" a window to the current workspace
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
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;
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

use crate::animation::{Animation, Clock};
use crate::layout::focus_ring::{FocusRing, FocusRingRenderElement};
use crate::layout::{LayoutElement, Options};
use crate::niri::Niri;
use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::surface::render_snapshot_from_surface_tree;
use crate::render_helpers::texture::TextureBuffer;
use crate::render_helpers::{render_to_texture, BakedBuffer, ToRenderElement};
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

// Font used to render window titles
const FONT: &str = "sans 14px";

#[derive(Debug)]
struct Thumbnail {
    id: MappedId,
    timestamp: Option<Instant>,
    offset: f64,
    size: Size<f64, Logical>,
    clock: Clock,
    open_animation: Option<Animation>,
    move_animation: Option<MoveAnimation>,
}

impl Thumbnail {
    fn are_animations_ongoing(&self) -> bool {
        self.open_animation.is_some() || self.move_animation.is_some()
    }

    fn advance_animations(&mut self) {
        self.open_animation.take_if(|a| a.is_done());
        self.move_animation.take_if(|a| a.anim.is_done());
    }

    /// Animate thumbnail motion from given location.
    fn animate_move_from_with_config(&mut self, from: f64, config: niri_config::Animation) {
        let current_offset = self.render_offset();

        // Preserve the previous config if ongoing.
        let anim = self.move_animation.take().map(|ma| ma.anim);
        let anim = anim
            .map(|anim| anim.restarted(1., 0., 0.))
            .unwrap_or_else(|| Animation::new(self.clock.clone(), 1., 0., 0., config));

        self.move_animation = Some(MoveAnimation {
            anim,
            from: from + current_offset,
        });
    }

    /// Thumbnail offset in the MRU UI view adjusted for animation.
    fn render_offset(&self) -> f64 {
        self.move_animation
            .as_ref()
            .map(|ma| ma.from * ma.anim.value())
            .unwrap_or_default()
    }

    fn render(
        &self,
        renderer: &mut GlesRenderer,
        location: Point<f64, Logical>,
        thumb_texture: MruTexture,
        title_texture: Option<MruTexture>,
        focus_ring: Option<&FocusRing>,
    ) -> impl Iterator<Item = WindowMruUiRenderElement> {
        let _span = tracy_client::span!("Thumbnail::render");

        let thumb_alpha = self
            .open_animation
            .as_ref()
            .map(|a| a.clamped_value() as f32)
            .unwrap_or(1.);
        let thumb_size = thumb_texture.logical_size();
        let thumb_elem: WindowMruUiRenderElement = {
            let bb = BakedBuffer {
                buffer: thumb_texture,
                location: Point::default(),
                src: None,
                dst: None,
            };

            bb.to_render_element(location, Scale::from(1.0), thumb_alpha, Kind::Unspecified)
                .into()
        };
        let mut rv: Vec<WindowMruUiRenderElement> = Vec::new();

        // if self.open_animation.is_none() {
        rv.extend(
            focus_ring
                .map(|fr| fr.render(renderer, location).map(Into::into))
                .into_iter()
                .flatten(),
        );
        // }

        rv.extend(
            title_texture
                .map(|t| {
                    let location = location
                        + Point::from((
                            thumb_size.w.saturating_sub(t.logical_size().w) / 2.,
                            SPACING / 2. + thumb_size.h,
                        ));
                    let bb = BakedBuffer {
                        buffer: t,
                        location: Point::default(),
                        src: None,
                        dst: None,
                    };
                    bb.to_render_element(location, 1.0.into(), thumb_alpha, Kind::Unspecified)
                })
                .map(Into::into),
        );
        Some(thumb_elem).into_iter().chain(rv)
    }
}

/// Window MRU traversal context.
#[derive(Debug)]
pub struct WindowMru {
    /// List of window ids to be traversed in MRU order.
    thumbnails: Vec<Thumbnail>,

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
        clock: Clock,
    ) -> Self {
        let scope = scope.unwrap_or_default();
        let filter = filter.unwrap_or_default();

        // todo: maybe using a `Match` is overkill here and a plain app_id
        // compare would suffice
        let window_match = filter.to_match(niri);

        // Build a list of MappedId from the requested scope sorted by timestamp
        let mut thumbnails: Vec<Thumbnail> = scope
            .windows(niri)
            .filter(|w| {
                window_match.as_ref().is_none_or(|m| {
                    with_toplevel_role(w.toplevel(), |r| window_matches(WindowRef::Mapped(w), r, m))
                })
            })
            .map(|w| Thumbnail {
                id: w.id(),
                timestamp: w.get_focus_timestamp(),
                offset: 0.,
                size: w.window.geometry().to_f64().downscale(THUMBNAIL_SCALE).size,
                clock: clock.clone(),
                open_animation: None,
                move_animation: None,
            })
            .collect();
        thumbnails.sort_by(
            |Thumbnail { timestamp: t1, .. }, Thumbnail { timestamp: t2, .. }| match (t1, t2) {
                (None, None) => cmp::Ordering::Equal,
                (Some(_), None) => cmp::Ordering::Less,
                (None, Some(_)) => cmp::Ordering::Greater,
                (Some(t1), Some(t2)) => t1.cmp(t2).reverse(),
            },
        );

        if direction == MruDirection::Backward && !thumbnails.is_empty() {
            // If moving backwards through the list, the first element is moved to the end of the
            // list
            let first = thumbnails.remove(0);
            thumbnails.push(first);
        }

        let mut offset = SPACING;
        thumbnails.iter_mut().for_each(|t| {
            t.offset = offset;
            offset += t.size.w + SPACING
        });

        match direction {
            MruDirection::Forward => {
                let mut res = Self {
                    thumbnails,
                    current: 0,
                    scope,
                    filter,
                    direction,
                };
                res.forward();
                res
            }
            MruDirection::Backward => {
                let current = thumbnails.len().saturating_sub(1);
                let mut res = Self {
                    thumbnails,
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

    fn forward(&mut self) {
        self.current = if self.thumbnails.is_empty() {
            0
        } else {
            (self.current + 1) % self.thumbnails.len()
        }
    }

    fn backward(&mut self) {
        self.current = self
            .current
            .checked_sub(1)
            .unwrap_or(self.thumbnails.len().saturating_sub(1))
    }

    fn get_id(&self, index: usize) -> MappedId {
        self.thumbnails[index].id
    }

    fn current(&self) -> Option<&Thumbnail> {
        self.thumbnails.get(self.current)
    }

    /// Returns the total width of all the thumbnails with leading and trailing margins.
    fn strip_width(&self) -> f64 {
        self.thumbnails
            .last()
            .map(|t| t.offset + t.size.w + SPACING)
            .unwrap_or(0.)
    }
}

type MruTexture = TextureBuffer<GlesTexture>;

pub enum WindowMruUi {
    Closed { close_animation: Option<Animation> },
    Open(Box<Inner>),
}

/// Opaque containing MRU UI state
pub struct Inner {
    /// List of Window Ids to display in the MRU UI.
    wmru: WindowMru,

    /// Texture cache for MRU UI, organized in a Vec that shares indices
    /// with the WindowMru.
    textures: RefCell<TextureCache>,

    /// FocusRing object used for the current MRU UI selection.
    focus_ring: FocusRing,

    /// Current view offset relative to the MRU list coordinate system.
    view_offset: Option<f64>,

    /// Animation clock
    clock: Clock,

    /// Opening Animation for the MruUi itself
    open_animation: Animation,

    /// Animation of the view offset while traversing the MRU list
    move_animation: Option<MoveAnimation>,

    /// Thumbnails linked to windows that were just closed, or to windows
    /// that no longer match the current MRU filter or scope.
    closing_thumbnails: Vec<ClosingThumbnail>,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

// Taken from Tile.rs,
#[derive(Debug)]
struct MoveAnimation {
    anim: Animation,
    from: f64,
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

niri_render_elements! {
    WindowMruUiRenderElement => {
        SolidColor = SolidColorRenderElement,
        TextureElement = PrimaryGpuTextureRenderElement,
        FocusRing = FocusRingRenderElement,
    }
}

impl WindowMruUi {
    pub fn new() -> Self {
        Self::Closed {
            close_animation: None,
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self, WindowMruUi::Open { .. })
    }

    pub fn open(&mut self, options: Rc<Options>, clock: Clock, mut wmru: WindowMru) {
        let Self::Closed { .. } = self else { return };

        // Each thumbnail is started with an open_animaiton
        wmru.thumbnails.iter_mut().for_each(|t| {
            t.open_animation = Some(Animation::new(
                clock.clone(),
                0.,
                1.,
                0.,
                options.animations.window_open.anim,
            ))
        });

        let nids = wmru.thumbnails.len();
        let open_anim = Animation::new(
            clock.clone(),
            0.,
            1.,
            0.,
            options.animations.window_mru_ui_open_close.0,
        );
        let inner = Inner {
            wmru,
            textures: RefCell::new(TextureCache::with_capacity(nids)),
            focus_ring: FocusRing::new(options.focus_ring),
            options,
            view_offset: None,
            closing_thumbnails: vec![],
            open_animation: open_anim,
            move_animation: None,
            clock,
        };

        *self = Self::Open(Box::new(inner));
    }

    pub fn close(&mut self) {
        let Self::Open(inner) = self else {
            return;
        };
        *self = Self::Closed {
            close_animation: Some(Animation::new(
                inner.clock.clone(),
                1.,
                0.,
                0.,
                inner.options.animations.window_mru_ui_open_close.0,
            )),
        };
    }

    pub fn advance(&mut self, dir: MruDirection) {
        let Self::Open(inner) = self else {
            return;
        };
        match dir {
            MruDirection::Forward => inner.wmru.forward(),
            MruDirection::Backward => inner.wmru.backward(),
        }
    }

    pub fn derive_new_mru_list(
        &self,
        niri: &Niri,
        scope: Option<MruScope>,
        filter: Option<MruFilter>,
    ) -> Option<WindowMru> {
        let Self::Open(inner) = self else {
            return None;
        };

        if scope.is_some_and(|s| s != inner.wmru.scope)
            || filter.is_some_and(|f| f != inner.wmru.filter)
        {
            Some(WindowMru::new(
                niri,
                inner.wmru.direction,
                scope,
                filter,
                inner.clock.clone(),
            ))
        } else {
            None
        }
    }

    /// Replace the current MRU list.
    pub fn update_mru_list(&mut self, dir: Option<MruDirection>, mut wmru: WindowMru) {
        let Self::Open(inner) = self else {
            return;
        };
        let prev_wmru = &mut inner.wmru;
        // Try to set the `current` field in the new wmru to match the one
        // from the previous mru.
        if let Some(current_selection) = prev_wmru.thumbnails.get(prev_wmru.current) {
            if let Some(current_in_new) = wmru.thumbnails.iter().position(
                |Thumbnail {
                     id: i,
                     timestamp: t,
                     ..
                 }| *i == current_selection.id || *t < current_selection.timestamp,
            ) {
                wmru.current = current_in_new
            }
        }

        // If the current Mru selection is present in both the previous Mru list
        // and in the replacement list, then we should advance in the requested
        // direction to avoid current staying unchanged despite the user
        // having performed an action.
        let should_advance = wmru.get_id(wmru.current) == prev_wmru.get_id(prev_wmru.current);

        // - Swap the MRU Ui's WindowMru with the new one,
        // - create a new texture cache initialized with textures that can be reused from the
        //   previous cache
        // - animate thumbnails:
        //   - thumbnails that were in both WindowMru (previous and replacement) change positions
        //     with a move animation
        //   - thumbnails that are no longer present in the replacement WindowMru disappear with a
        //     close animation
        //   - thumbnails that are only in the replacement WindowMru get an open animation
        {
            let len = wmru.thumbnails.len();

            // Create new empty texture cache
            let mut textures = Vec::with_capacity(len);
            textures.resize_with(len, Default::default);

            // Replace the previous texture cache
            let mut ptextures = inner.textures.replace(TextureCache(textures)).0;
            let textures = &mut inner.textures.borrow_mut().0;

            //
            let mut start_idx = 0;

            wmru.thumbnails.iter_mut().enumerate().for_each(|(idx, t)| {
                match prev_wmru
                    .thumbnails
                    .iter()
                    .enumerate()
                    .skip(start_idx)
                    .try_for_each(|(pidx, pt)| {
                        if prev_wmru.direction == MruDirection::Forward
                            && pt.timestamp < t.timestamp
                        {
                            return ControlFlow::Break(None);
                        }
                        start_idx = pidx + 1;
                        if t.id == pt.id {
                            ControlFlow::Break(Some(pidx))
                        } else {
                            ControlFlow::Continue(())
                        }
                    }) {
                    ControlFlow::Break(Some(pidx)) => {
                        // The thumbnail is present in the previous and
                        // replacement Mru list.

                        // Animate the new thumbnail so that it appears to move
                        // from the corresponding one's previous position.
                        let pt = &prev_wmru.thumbnails[pidx];
                        t.animate_move_from_with_config(
                            pt.offset - t.offset,
                            inner.options.animations.window_movement.0,
                        );

                        // Retain the previous thumbnail's textures by
                        // transfering it to the new texture cache.
                        mem::swap(&mut ptextures[pidx], &mut textures[idx]);
                    }
                    _ => {
                        // The new thumbnail wasn't in the previous Mru list.

                        // Schedule an open animation for it.
                        t.open_animation = Some(Animation::new(
                            t.clock.clone(),
                            0.,
                            1.,
                            0.,
                            inner.options.animations.window_open.anim,
                        ))
                    }
                }
            });

            // Replace the UI's WindowMru.
            let prev_wmru = std::mem::replace(prev_wmru, wmru);

            // Whatever textures remain in the previous texture cache should be
            // used to trigger close animations for the corresponding thumbnails.
            prev_wmru
                .thumbnails
                .into_iter()
                .enumerate()
                .for_each(|(idx, thumb)| {
                    if let Some(texture) = ptextures[idx].thumbnail.take() {
                        let anim = Animation::new(
                            inner.clock.clone(),
                            0.,
                            1.,
                            0.,
                            inner.options.animations.window_close.anim,
                        );
                        let closing = ClosingThumbnail::new(thumb, texture, anim);
                        inner.closing_thumbnails.push(closing);
                    }
                });
        }

        // And (possibly) advance in the requested direction.
        if should_advance {
            if let Some(dir) = dir {
                self.advance(dir);
            }
        }
    }

    pub fn first(&mut self) {
        let Self::Open(inner) = self else {
            return;
        };
        inner.wmru.current = 0;
    }

    pub fn last(&mut self) {
        let Self::Open(inner) = self else {
            return;
        };
        inner.wmru.current = inner.wmru.thumbnails.len().saturating_sub(1);
    }

    pub fn current_window_id(&self) -> Option<MappedId> {
        let Self::Open(inner) = self else {
            return None;
        };
        let wmru = &inner.wmru;
        if wmru.thumbnails.is_empty() {
            None
        } else {
            wmru.thumbnails.get(wmru.current).map(|t| t.id)
        }
    }

    pub fn remove_window(&mut self, id: MappedId) {
        let Self::Open(inner) = self else {
            return;
        };
        let wmru = &mut inner.wmru;
        if let Some(idx) = wmru.thumbnails.iter().position(|t| t.id == id) {
            // Remove the thumbnail and the cached texture.
            let thumb = wmru.thumbnails.remove(idx);
            if wmru.current >= wmru.thumbnails.len() {
                wmru.current = wmru.current.saturating_sub(1);
            }
            // Update the offset of all thumbnails that follow the removed
            // thumbnail.
            wmru.thumbnails.iter_mut().skip(idx).for_each(|t| {
                let offset_delta = thumb.size.w + SPACING;
                t.animate_move_from_with_config(
                    offset_delta,
                    inner.options.animations.window_movement.0,
                );
                t.offset -= offset_delta;
            });
            // If there is a cached texture, the thumbnail may be visible
            // so schedule a closing animation.
            if let Some(texture) = inner.textures.borrow_mut().0.remove(idx).thumbnail.take() {
                let anim = Animation::new(
                    inner.clock.clone(),
                    0.,
                    1.,
                    0.,
                    inner.options.animations.window_close.anim,
                );
                let closing = ClosingThumbnail::new(thumb, texture, anim);
                inner.closing_thumbnails.push(closing);
            }
        }
    }

    pub fn update_render_elements(&mut self, output: &Output) {
        let Self::Open(inner) = self else {
            return;
        };

        let new_view_offset = {
            let wmru = &inner.wmru;
            let strip_width = wmru.strip_width();
            let output_size = output_size(output);

            if strip_width <= output_size.w {
                // All thumbnails fit on the output, adjust the view_offset
                // to center the entire list of thumbnails.
                -(output_size.w - strip_width) / 2.
            } else {
                // The thumbnail strip is longer than what can fit on the
                // output. The view_offset is calculated so as to have the
                // current MRU selection centered, unless this leaves more than
                // `SPACING` empty space at the left or right of the screen.
                // In the latter case, the first/last thumbnail is positioned
                // `SPACING` away from the output's edge.
                let Some(current) = wmru.current() else {
                    return;
                };
                let width_before_current = current.offset + current.size.w / 2.;
                let width_after_current = strip_width - width_before_current;

                if width_before_current <= output_size.w / 2. {
                    // Align on the thumbnail strip on the left side of the screen.
                    0.
                } else if width_after_current <= output_size.w / 2. {
                    // Align on the thumbnail strip on the right side of the screen.
                    strip_width - output_size.w
                } else {
                    // center on the current MRU selection.
                    width_before_current - output_size.w / 2.
                }
            }
        };

        if let Some(prev_view_offset) = inner.view_offset {
            let pixel = 1. / output.current_scale().fractional_scale();
            if (new_view_offset - prev_view_offset).abs() > pixel {
                inner.animate_view_offset_from(new_view_offset - prev_view_offset);
            }
        }

        inner.view_offset = Some(new_view_offset);

        if let Some(current) = inner.wmru.current() {
            inner.focus_ring.update_render_elements(
                current.size,
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
            )
        }
    }

    pub fn render_output(
        &self,
        niri: &Niri,
        output: &Output,
        renderer: &mut GlesRenderer,
    ) -> Vec<WindowMruUiRenderElement> {
        let mut rv = Vec::new();
        let output_size = output_size(output);

        let progress = match self {
            Self::Closed {
                close_animation: None,
            } => return vec![],
            Self::Closed {
                close_animation: Some(close_animation),
            } => close_animation.clamped_value(),
            Self::Open(inner) => {
                rv.extend(inner.render(niri, renderer, output_size));
                inner.open_animation.clamped_value()
            }
        };

        let progress = progress.clamp(0., 1.) as f32;

        // Put a panel above the current desktop view to contrast the thumbnails
        let buffer = SolidColorBuffer::new(output_size, BACKGROUND);

        rv.push(
            SolidColorRenderElement::from_buffer(
                &buffer,
                // Point::from((0., output_size.h / 16.)),
                Point::default(),
                progress,
                Kind::Unspecified,
            )
            .into(),
        );

        rv
    }

    pub fn are_animations_ongoing(&self) -> bool {
        match self {
            Self::Open(inner) => inner.are_animations_ongoing(),
            Self::Closed { close_animation } => close_animation.is_some(),
        }
    }

    pub fn advance_animations(&mut self) {
        match *self {
            Self::Open(ref mut inner) => inner.advance_animations(),
            Self::Closed {
                ref mut close_animation,
            } => {
                close_animation.take_if(|a| a.is_done());
            }
        }
    }
}

impl Inner {
    fn are_animations_ongoing(&self) -> bool {
        (!self.open_animation.is_done())
            || self
                .wmru
                .thumbnails
                .iter()
                .any(|t| t.are_animations_ongoing())
            || self.move_animation.is_some()
            || !self.closing_thumbnails.is_empty()
    }

    fn advance_animations(&mut self) {
        self.move_animation.take_if(|ma| ma.anim.is_done());
        self.closing_thumbnails
            .retain_mut(|closing| closing.are_animations_ongoing());
        self.wmru
            .thumbnails
            .iter_mut()
            .for_each(|t| t.advance_animations());
    }

    fn animate_view_offset_from(&mut self, from: f64) {
        self.animate_view_offset_from_with_config(from, self.options.animations.window_movement.0)
    }

    fn animate_view_offset_from_with_config(&mut self, from: f64, config: niri_config::Animation) {
        let current_offset = self.render_offset().x;

        let anim = self
            .move_animation
            .take()
            .map(|ma| ma.anim)
            .map(|a| a.restarted(1., 0., 0.))
            .unwrap_or_else(|| Animation::new(self.clock.clone(), 1., 0., 0., config));

        self.move_animation = Some(MoveAnimation {
            anim,
            from: current_offset - from,
        });
    }

    // Adapted from tile.rs.
    fn render_offset(&self) -> Point<f64, Logical> {
        let mut offset = Point::default();

        if let Some(ref ma) = self.move_animation {
            offset.x += ma.from * ma.anim.value();
        }

        offset
    }

    fn render(
        &self,
        niri: &Niri,
        renderer: &mut GlesRenderer,
        output_size: Size<f64, Logical>,
    ) -> impl Iterator<Item = WindowMruUiRenderElement> {
        let mut rv = Vec::new();

        let Some(view_offset) = self.view_offset else {
            return rv.into_iter();
        };

        let view_offset = self
            .move_animation
            .as_ref()
            .map(|ma| ma.from * ma.anim.value())
            .unwrap_or(0.)
            + view_offset;

        // As with tiles, render thumbnails for closing windows on top of
        // others.
        for closing in self.closing_thumbnails.iter().rev() {
            if closing.offset + closing.size.w >= view_offset {
                if closing.offset <= view_offset + output_size.w {
                    let loc = Point::from((
                        closing.offset - view_offset,
                        (output_size.h - closing.texture.logical_size().h) / 2.,
                    ));
                    let elem = closing.render(loc);
                    rv.push(elem.into());
                } else {
                    break;
                }
            }
        }

        // Add all visible thumbnails
        let wmru = &self.wmru;
        for (i, t) in wmru.thumbnails.iter().enumerate() {
            if t.offset + t.size.w >= view_offset {
                if t.offset <= view_offset + output_size.w {
                    let mut tcache = self.textures.borrow_mut();
                    let textures = tcache.get_mut(i);
                    let id = wmru.get_id(i);
                    if let Some(thumb_texture) = textures.get_thumbnail(niri, renderer, id) {
                        let title_texture = (i == wmru.current)
                            .then(|| {
                                textures.get_title(
                                    niri,
                                    renderer,
                                    id,
                                    thumb_texture
                                        .logical_size()
                                        .to_physical(1.)
                                        .to_i32_round()
                                        .w,
                                )
                            })
                            .flatten();
                        let loc = Point::from((
                            t.offset + t.render_offset() - view_offset,
                            (output_size.h - thumb_texture.logical_size().h) / 2.,
                        ));
                        rv.extend(t.render(
                            renderer,
                            loc,
                            thumb_texture,
                            title_texture,
                            (i == wmru.current).then_some(&self.focus_ring),
                        ));
                    }
                } else {
                    break;
                }
            }
        }

        rv.into_iter()
    }
}

impl Default for WindowMruUi {
    fn default() -> Self {
        Self::new()
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

/// A visible Thumbnail that is in the process of being dismissed.
/// This can happen if the corresponding window was closed or if the
/// window ceases to match the current MRU filter or scope.
struct ClosingThumbnail {
    texture: MruTexture,
    size: Size<f64, Logical>,
    offset: f64,
    anim: Animation,
}

impl ClosingThumbnail {
    fn new(thumb: Thumbnail, texture: MruTexture, anim: Animation) -> Self {
        Self {
            texture,
            offset: thumb.offset,
            size: thumb.size,
            anim,
        }
    }

    pub fn render(&self, location: Point<f64, Logical>) -> PrimaryGpuTextureRenderElement {
        let bb = BakedBuffer {
            buffer: self.texture.clone(),
            location: Point::default(),
            src: None,
            dst: None,
        };

        bb.to_render_element(
            location,
            Scale::from(1.0),
            (1. - self.anim.value()) as f32,
            Kind::Unspecified,
        )
    }

    fn are_animations_ongoing(&self) -> bool {
        !self.anim.is_done()
    }
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
