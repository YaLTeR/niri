/*
Todo:

- Add test cases
- Animations
  x navigation scrolling
  x thumbnails appearing/disappearing
  x reorganization on scope/filter change
  x animate transition from selecting a thumbnail to the focused window
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
x how to handle overview mode? Inhibit open?
x add config item to disable
x make modifier key configurable

*/
use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::{iter, mem};

use niri_config::{
    Action, Bind, Key, Match, ModKey, Modifiers, MruDirection, MruFilter, MruScope, RegexEq,
    Trigger,
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
use crate::layout::monitor::Monitor;
use crate::layout::tile::Tile;
use crate::layout::{LayoutElement, Options};
use crate::niri::Niri;
use crate::niri_render_elements;
use crate::render_helpers::offscreen::OffscreenBuffer;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::surface::render_snapshot_from_surface_tree;
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::render_helpers::{render_to_texture, RenderTarget, ToRenderElement};
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

// Minimum duration the MRU UI needs to have stayed opened for the Thumbnail
// selection animation to be triggered
pub const THUMBNAIL_SELECT_ANIMATION_THRESHOLD: Duration = Duration::from_millis(250);

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
            PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
                thumb_texture,
                location,
                thumb_alpha,
                None,
                None,
                Kind::Unspecified,
            ))
            .into()
        };

        let fr_elem = focus_ring
            .map(|fr| fr.render(renderer, location).map(Into::into))
            .into_iter()
            .flatten();

        let title_elem = title_texture
            .map(|t| {
                let location = location
                    + Point::from((
                        thumb_size.w.saturating_sub(t.logical_size().w) / 2.,
                        SPACING / 2. + thumb_size.h,
                    ));
                PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
                    t,
                    location,
                    thumb_alpha,
                    None,
                    None,
                    Kind::Unspecified,
                ))
            })
            .map(Into::into);

        Some(thumb_elem)
            .into_iter()
            .chain(fr_elem)
            .chain(title_elem)
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
}

impl WindowMru {
    pub fn new(
        niri: &Niri,
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
        thumbnails
            .sort_by(|Thumbnail { timestamp: t1, .. }, Thumbnail { timestamp: t2, .. }| t2.cmp(t1));

        let mut offset = SPACING;
        thumbnails.iter_mut().for_each(|t| {
            t.offset = offset;
            offset += t.size.w + SPACING
        });

        Self {
            thumbnails,
            current: 0,
            scope,
            filter,
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

    fn get_id(&self, index: usize) -> Option<MappedId> {
        Some(self.thumbnails.get(index)?.id)
    }

    fn current(&self) -> Option<&Thumbnail> {
        self.thumbnails.get(self.current)
    }

    /// Returns the total width of all the thumbnails with leading and trailing margins included.
    fn strip_width(&self) -> f64 {
        self.thumbnails
            .last()
            .map(|t| t.offset + t.size.w + SPACING)
            .unwrap_or(0.)
    }

    fn first(&mut self) {
        self.current = 0;
    }

    fn last(&mut self) {
        self.current = self.thumbnails.len().saturating_sub(1);
    }
}

type MruTexture = TextureBuffer<GlesTexture>;

pub struct WindowMruUi {
    state: WindowMruUiState,
    mod_key: ModKey,
    cached_bindings: Option<Vec<Bind>>,
    cached_opened_bindings: Option<Vec<Bind>>,
}

pub enum WindowMruUiState {
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

    /// Timestamp when the UI was opened
    open_timestamp: Instant,
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
                    app_id: Some(RegexEq::from_str(&format!("^{app_id}$")).ok()?),
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
    pub fn new(config: &niri_config::RecentWindows) -> Self {
        Self {
            mod_key: config.mod_key,
            cached_bindings: None,
            cached_opened_bindings: None,
            state: WindowMruUiState::Closed {
                close_animation: None,
            },
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.state, WindowMruUiState::Open { .. })
    }

    pub fn open(
        &mut self,
        options: Rc<Options>,
        clock: Clock,
        mut wmru: WindowMru,
        dir: MruDirection,
    ) {
        if self.is_open() {
            return;
        }

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
            open_timestamp: Instant::now(),
        };

        self.state = WindowMruUiState::Open(Box::new(inner));
        self.advance(dir);
    }

    pub fn close(&mut self) -> Option<(SelectedThumbnail, Instant)> {
        let WindowMruUiState::Open(ref inner) = self.state else {
            return None;
        };
        let thumb = inner.select_thumbnail();
        let clock = inner.clock.clone();
        let config = inner.options.animations.window_mru_ui_open_close.0;
        let open_ts = inner.open_timestamp;

        self.state = WindowMruUiState::Closed {
            close_animation: Some(Animation::new(clock, 1., 0., 0., config)),
        };

        thumb.map(|t| (t, open_ts))
    }

    pub fn update_config(&mut self, config: &niri_config::Config) {
        // invalidate cached key bindings
        self.mod_key = config.recent_windows.mod_key;
        self.cached_bindings = None;
        self.cached_opened_bindings = None;
    }

    pub fn advance(&mut self, dir: MruDirection) {
        let WindowMruUiState::Open(ref mut inner) = self.state else {
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
        let WindowMruUiState::Open(ref inner) = self.state else {
            return None;
        };

        if scope.is_some_and(|s| s != inner.wmru.scope)
            || filter.is_some_and(|f| f != inner.wmru.filter)
        {
            Some(WindowMru::new(
                niri,
                scope.or(Some(inner.wmru.scope)),
                filter.or(Some(inner.wmru.filter)),
                inner.clock.clone(),
            ))
        } else {
            None
        }
    }

    /// Replace the current MRU list.
    pub fn update_mru_list(&mut self, dir: Option<MruDirection>, mut wmru: WindowMru) {
        let WindowMruUiState::Open(ref mut inner) = self.state else {
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

            // Index in the previous Mru list at which to start looking
            // for thumbnail Ids to match with those from the new Mru list.
            // This just avoids having to go through the entire list each
            // time.
            let mut start_idx = 0;

            // View offset after the update.
            // It is calculated:
            // - when `dir` is None and the `should_advance` is true, i.e. the current thumbnail is
            //   present in both Mru lists, then the new view_offset is chosen so as to keep that
            //   thumbnail in the same position in the view.
            // - otherwise, the view_offset is chosen to make the first common thumbnail retain its
            //   position
            // - if there are no common thumbnails the view_offset eventually defaults to 0.
            let mut view_offset = {
                if let Some(vo) = inner.view_offset {
                    if let Some((pt, t)) = if should_advance && dir.is_none() {
                        prev_wmru
                            .current()
                            .and_then(|pt| wmru.current().map(|t| (pt, t)))
                    } else {
                        // look for the first visible thumbnail present in both lists
                        prev_wmru
                            .thumbnails
                            .iter()
                            .filter(|pt| pt.offset + pt.size.w >= vo)
                            .filter_map(|pt| {
                                wmru.thumbnails
                                    .iter()
                                    .find(|t| t.id == pt.id)
                                    .map(|t| (pt, t))
                            })
                            .next()
                    } {
                        Some(t.offset - pt.offset + vo)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            wmru.thumbnails.iter_mut().enumerate().for_each(|(idx, t)| {
                match prev_wmru
                    .thumbnails
                    .iter()
                    .enumerate()
                    .skip(start_idx)
                    .try_for_each(|(pidx, pt)| {
                        if pt.timestamp < t.timestamp {
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
                        let pt = &prev_wmru.thumbnails[pidx];

                        // If the view_offset hasn't yet been determined, derive
                        // it by matching the thumbnail's position in the previous
                        // view and the new one.
                        if view_offset.is_none() && inner.view_offset.is_some() {
                            view_offset.replace(t.offset - pt.offset + inner.view_offset.unwrap());
                        };

                        // Animate the new thumbnail so that it appears to move
                        // from the corresponding one's former position.
                        // The previous position needs to be projected into the
                        // updated view's referential.
                        if let Some(view_offset) = view_offset {
                            if let Some(prev_view_offset) = inner.view_offset {
                                t.animate_move_from_with_config(
                                    (pt.offset - prev_view_offset) - (t.offset - view_offset),
                                    inner.options.animations.window_movement.0,
                                );
                            }
                        }

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
            if let Some(prev_view_offset) = inner.view_offset {
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
                            let offset = thumb.offset - prev_view_offset;
                            let closing = ClosingThumbnail::new(thumb, texture, offset, anim);
                            inner.closing_thumbnails.push(closing);
                        }
                    });
            }

            inner.view_offset = view_offset;
        }

        // And (possibly) advance in the requested direction.
        if should_advance {
            if let Some(dir) = dir {
                self.advance(dir);
            }
        }
    }

    pub fn first(&mut self) {
        let WindowMruUiState::Open(ref mut inner) = self.state else {
            return;
        };
        inner.wmru.first();
    }

    pub fn last(&mut self) {
        let WindowMruUiState::Open(ref mut inner) = self.state else {
            return;
        };
        inner.wmru.last();
    }

    pub fn current_window_id(&self) -> Option<MappedId> {
        let WindowMruUiState::Open(ref inner) = self.state else {
            return None;
        };
        inner.current_window_id()
    }

    pub fn remove_window(&mut self, id: MappedId) {
        let WindowMruUiState::Open(ref mut inner) = self.state else {
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
                if let Some(view_offset) = inner.view_offset {
                    let offset = thumb.offset - view_offset;
                    let closing = ClosingThumbnail::new(thumb, texture, offset, anim);
                    inner.closing_thumbnails.push(closing);
                }
            }
        }
    }

    pub fn update_render_elements(&mut self, output: &Output) {
        let WindowMruUiState::Open(ref mut inner) = self.state else {
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

        let progress = match self.state {
            WindowMruUiState::Closed {
                close_animation: None,
            } => return vec![],
            WindowMruUiState::Closed {
                close_animation: Some(ref close_animation),
            } => close_animation.clamped_value(),
            WindowMruUiState::Open(ref inner) => {
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
                Point::default(),
                progress,
                Kind::Unspecified,
            )
            .into(),
        );

        rv
    }

    pub fn are_animations_ongoing(&self) -> bool {
        match self.state {
            WindowMruUiState::Open(ref inner) => inner.are_animations_ongoing(),
            WindowMruUiState::Closed {
                ref close_animation,
            } => close_animation.is_some(),
        }
    }

    pub fn advance_animations(&mut self) {
        match self.state {
            WindowMruUiState::Open(ref mut inner) => inner.advance_animations(),
            WindowMruUiState::Closed {
                ref mut close_animation,
            } => {
                close_animation.take_if(|a| a.is_done());
            }
        }
    }

    pub fn bindings(&mut self) -> impl Iterator<Item = &Bind> {
        let modifiers = self.mod_key.to_modifiers();
        let apply_modkey = move |mut bind: Bind| {
            bind.key.modifiers |= modifiers;
            bind
        };

        let is_open = self.is_open();

        let bindings = self
            .cached_bindings
            .get_or_insert(MRU_UI_BINDINGS.iter().cloned().map(apply_modkey).collect());

        let opened_bindings = self.cached_opened_bindings.get_or_insert(
            MRU_UI_OPENED_BINDINGS
                .iter()
                .cloned()
                .map(apply_modkey)
                .collect(),
        );

        bindings.iter().chain(
            is_open
                .then_some(opened_bindings.iter())
                .into_iter()
                .flatten(),
        )
    }
}

impl Inner {
    fn current_window_id(&self) -> Option<MappedId> {
        let wmru = &self.wmru;
        if wmru.thumbnails.is_empty() {
            None
        } else {
            wmru.thumbnails.get(wmru.current).map(|t| t.id)
        }
    }

    /// Return the window Id and screen position of the selected thumbnail.
    /// The thumbnail texture is _taken_ from the texture cache.
    fn select_thumbnail(&self) -> Option<SelectedThumbnail> {
        let id = self.current_window_id()?;
        let thumbnail = self.wmru.current()?;
        let texture = self
            .textures
            .borrow_mut()
            .get_mut(self.wmru.current)?
            .thumbnail
            .take()?;
        let view_offset = self.view_offset?
            + self
                .move_animation
                .as_ref()
                .map(|ma| ma.from * ma.anim.value())
                .unwrap_or(0.);
        Some(SelectedThumbnail {
            id,
            offset: -view_offset + thumbnail.offset + thumbnail.render_offset(),
            scale: THUMBNAIL_SCALE,
            texture,
        })
    }

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
            if closing.offset < output_size.w && closing.offset + closing.size.w > 0. {
                let loc = Point::from((
                    closing.offset,
                    (output_size.h - closing.texture.logical_size().h) / 2.,
                ));
                let elem = closing.render(loc);
                rv.push(elem.into());
            }
        }

        // Add all visible thumbnails
        let wmru = &self.wmru;
        for (i, t) in wmru.thumbnails.iter().enumerate() {
            // The next check is somewhat inaccurate because it doesn't factor in the fact that the
            // thumbnail could be in motion, and instead only considers tiles that have
            // their final position in the view. In practice this looks ok.
            if t.offset + t.size.w >= view_offset {
                if t.offset <= view_offset + output_size.w {
                    let mut tcache = self.textures.borrow_mut();
                    let textures = tcache.get_mut(i).unwrap();
                    if let Some(id) = wmru.get_id(i) {
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
                    }
                } else {
                    break;
                }
            }
        }
        rv.into_iter()
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
                render_mapped_to_texture(renderer, mapped, THUMBNAIL_SCALE)
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

fn render_mapped_to_texture(
    renderer: &mut GlesRenderer,
    mapped: &Mapped,
    scale: impl Into<Scale<f64>>,
) -> Option<MruTexture> {
    let surface = mapped.toplevel().wl_surface();

    // collect contents for the toplevel surface
    let mut contents = vec![];
    render_snapshot_from_surface_tree(renderer, surface, Point::from((0., 0.)), &mut contents);

    // render to a new texture
    let wsz = mapped.size().to_physical(1);
    render_to_texture(
        renderer,
        wsz,
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
        TextureBuffer::from_texture(renderer, texture, scale, Transform::Normal, vec![])
    })
}

pub struct TextureCache(Vec<MruUiTileTextures>);

impl TextureCache {
    fn with_capacity(size: usize) -> Self {
        let mut textures = Vec::with_capacity(size);
        textures.resize_with(size, Default::default);
        Self(textures)
    }

    /// Returns the texture at given cache index
    /// Panics if the index points beyond the end of the cache.
    fn get_mut(&mut self, index: usize) -> Option<&mut MruUiTileTextures> {
        self.0.get_mut(index)
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
    /// Offset relative to output (and not the view).
    offset: f64,
    anim: Animation,
}

impl ClosingThumbnail {
    fn new(thumb: Thumbnail, texture: MruTexture, offset: f64, anim: Animation) -> Self {
        Self {
            texture,
            offset,
            size: thumb.size,
            anim,
        }
    }

    pub fn render(&self, location: Point<f64, Logical>) -> PrimaryGpuTextureRenderElement {
        PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
            self.texture.clone(),
            location,
            (1. - self.anim.value()) as f32,
            None,
            None,
            Kind::Unspecified,
        ))
    }

    fn are_animations_ongoing(&self) -> bool {
        !self.anim.is_done()
    }
}

pub struct SelectedThumbnail {
    /// Id of the window the thumbnail corresponds to.
    pub id: MappedId,

    /// Most recent view offset of the thumbnail (in Monitor coordinate space).
    pub offset: f64,

    /// Scale used to render the thumbnail relative to its corresponding window.
    pub scale: f64,

    /// Texture used to render the thumbnail.
    texture: MruTexture,
}

pub struct ThumbnailSelectionAnimation {
    pub id: MappedId,
    pub anim: Animation,
    /// Original position of the thumbnail in global coordinate space.
    from: Point<f64, Logical>,
    /// Original scale applied to the thumbnail.
    from_scale: f64,
    /// Buffer into which the animated tile is rendered.
    offscreen: OffscreenBuffer,
}

impl ThumbnailSelectionAnimation {
    pub fn new(
        thumb: SelectedThumbnail,
        mon: &Monitor<Mapped>,
        clock: Clock,
        config: niri_config::Animation,
    ) -> Self {
        let mon_view = Rectangle::new(mon.output().current_location().to_f64(), mon.view_size());
        let texture_size = thumb.texture.logical_size();
        ThumbnailSelectionAnimation {
            id: thumb.id,
            anim: Animation::new(clock, 1., 0., 0., config),
            from: mon_view.loc
                + Point::<f64, Logical>::from((
                    thumb.offset,
                    (mon_view.size.h - texture_size.h) / 2.,
                )),
            from_scale: thumb.scale,
            offscreen: OffscreenBuffer::default(),
        }
    }

    fn render_params<'n>(&self, niri: &'n Niri) -> Option<ThumbnailAnimationRenderParameters<'n>> {
        let (monitor, ws_idx, tile, to) = niri
            .layout
            .workspaces()
            .filter_map(|(mon, idx, ws)| {
                let mon = mon?;
                let out = mon.output();

                ws.tiles_with_render_positions()
                    .filter_map(|(tile, pos, _)| {
                        (tile.window().id() == self.id).then_some((
                            mon,
                            idx,
                            tile,
                            out.current_location().to_f64() + pos,
                        ))
                    })
                    .next()
            })
            .next()?;

        // Adjust location to accomodate a possible workpace switch animation.
        let ws_adjust = monitor
            .workspaces_with_render_geo_idx()
            .filter_map(|((idx, _), geo)| (idx == ws_idx).then_some(geo.loc))
            .next()?;

        let to = to + ws_adjust;
        let destination_view = Rectangle::new(to, tile.tile_size());

        let scale = Scale::from(
            (1. + (self.from_scale - 1.) * self.anim.value()).clamp(1., self.from_scale),
        );
        let loc = to + (self.from - to).upscale(self.anim.clamped_value());
        let current_view = Rectangle::new(loc, tile.animated_tile_size().to_f64().downscale(scale));

        Some(ThumbnailAnimationRenderParameters {
            monitor,
            tile,
            destination_view,
            current_view,
            scale,
        })
    }

    pub fn render_output(
        &self,
        niri: &Niri,
        renderer: &mut GlesRenderer,
        output: &Output,
    ) -> Vec<WindowMruUiRenderElement> {
        let mut rv = Vec::new();

        // The thumbnail is rendered if its view_rect overlaps the monitor's
        // view_rect. However a tile may have a final position within the global
        // coordinate system that places its on a different monitor than the
        // one associated with the tile (e.g. after a workspace switch that
        // is triggered while the thumbnail selection animation is already in
        // progress). This will look really confusing, so instead the thumbnail
        // is rendered:
        // - on a monitor if it is "moving through" that monitor's view_rect, i.e. the final
        //   destination is **not** on that monitor
        // - on the final destination monitor so long as its view_rect overlaps the tile's
        //   view_rect.

        if let Some(mon) = niri.layout.monitor_for_output(output) {
            let output_view_rect =
                Rectangle::new(mon.output().current_location().to_f64(), mon.view_size());

            if let Some(trp) = self.render_params(niri) {
                if output_view_rect.overlaps(trp.current_view)
                    && (output == trp.monitor.output()
                        || !output_view_rect.overlaps(trp.destination_view))
                {
                    let focus_ring = niri
                        .layout
                        .focus()
                        .map(|m| m.id())
                        .is_some_and(|id| id == self.id);

                    let rve: Vec<_> = trp
                        .tile
                        .render(
                            renderer,
                            Point::default(),
                            focus_ring,
                            RenderTarget::Offscreen,
                        )
                        .collect();

                    match self.offscreen.render(renderer, Scale::from(1.), &rve) {
                        Ok((ore, _, _)) => {
                            let buffer = TextureBuffer::from_texture(
                                renderer,
                                ore.texture().clone(),
                                trp.scale,
                                Transform::Normal,
                                vec![],
                            );
                            let tre = TextureRenderElement::from_texture_buffer(
                                buffer,
                                trp.current_view.loc - output_view_rect.loc + ore.offset(),
                                1.,
                                None,
                                None,
                                Kind::Unspecified,
                            );
                            rv.push(PrimaryGpuTextureRenderElement(tre).into());
                        }
                        Err(err) => warn!(
                            "Couldn't render tile into offscreen for thumbnail animation: {err:?}"
                        ),
                    }
                }
            }
        }
        rv
    }
}

struct ThumbnailAnimationRenderParameters<'n> {
    monitor: &'n Monitor<Mapped>,
    tile: &'n Tile<Mapped>,
    destination_view: Rectangle<f64, Logical>,
    current_view: Rectangle<f64, Logical>,
    scale: Scale<f64>,
}

/// Key bindings available when the MRU UI is open.
/// Because the UI is closed when the Alt key is released, all bindings
/// have the ALT modifier.
static MRU_UI_OPENED_BINDINGS: &[Bind] = &[
    // Escape just closes the MRU UI
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Escape),
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
        },
        action: Action::MruAdvance(MruDirection::Backward, None, None),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    // and so can h and l can be used as well
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::l),
            modifiers: Modifiers::empty(),
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
            trigger: Trigger::Keysym(Keysym::h),
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::SHIFT,
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
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
            modifiers: Modifiers::empty(),
        },
        action: Action::MruChangeScope(MruScope::Output),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
];

/// Key bindings that are available both when the MRU UI is opened or closed
static MRU_UI_BINDINGS: &[Bind] = &[
    // The following two bindings cover MRU window navigation. They are
    // preset because the `Alt` key is treated specially in `on_keyboard`.
    // When it is released the active MRU traversal is considered to have
    // completed. If the user were allowed to change the MRU bindings
    // below, the navigation mechanism would no longer work as intended.
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Tab),
            modifiers: Modifiers::empty(),
        },
        action: Action::MruAdvance(MruDirection::Forward, None, Some(MruFilter::None)),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Tab),
            modifiers: Modifiers::SHIFT,
        },
        action: Action::MruAdvance(MruDirection::Backward, None, Some(MruFilter::None)),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    // forward/backward bind actions for AppId navigation
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::grave),
            modifiers: Modifiers::empty(),
        },
        action: Action::MruAdvance(MruDirection::Forward, None, Some(MruFilter::AppId)),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::grave),
            modifiers: Modifiers::SHIFT,
        },
        action: Action::MruAdvance(MruDirection::Backward, None, Some(MruFilter::AppId)),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn new_wmru(size: usize, scope: Option<MruScope>, filter: Option<MruFilter>) -> WindowMru {
        let clock = Clock::with_time(Duration::ZERO);
        let thumbnails = (0..size)
            .map(|_| Thumbnail {
                id: MappedId::next(),
                timestamp: None,
                size: Size::from((0., 0.)),
                offset: 0.,
                clock: clock.clone(),
                open_animation: None,
                move_animation: None,
            })
            .collect::<Vec<_>>();
        WindowMru {
            thumbnails,
            current: 0,
            scope: scope.unwrap_or(MruScope::All),
            filter: filter.unwrap_or(MruFilter::None),
        }
    }

    #[track_caller]
    fn check_base_mru_behavior(wmru: &mut WindowMru) {
        wmru.last();
        wmru.forward();
        assert_eq!(0, wmru.current);

        wmru.first();
        wmru.backward();
        assert_eq!(wmru.thumbnails.len().saturating_sub(1), wmru.current)
    }

    #[test]
    fn wrap_around() {
        for l in 0..3 {
            let mut wmru = new_wmru(l, None, None);
            check_base_mru_behavior(&mut wmru);
        }
    }
}
