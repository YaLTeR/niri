/*
Todo:

- Add test cases
- Animation. Likely to need a position cache as for Tiles
- Transition when wrapping around during Mru navigation
- support clicking on the target thumbnail
- add title of the current Mru selection under the thumbnail
x support only considering windows from current output/workspace
x support only considering windows from the currently selected application
- support switching navigation modes while the Mru UI is open
x Unfocus the current Tile while the MruUi is up and refocus as necessary when
  the UI is closed.
x Keybindings in the MruUi, e.g. Close window, Quit, Focus selected, prev, next
x Mru list should contain an Option<BakedBuffer> to cache the texture
  once rendered and then reused as needed.
x Transition when opening/closing MruUI

*/
use std::cell::RefCell;
use std::cmp::{self};
use std::iter;
use std::ops::ControlFlow;
use std::str::FromStr;
use std::time::Instant;

use niri_config::{
    Action, Bind, Key, Match, Modifiers, MruDirection, MruFilter, MruScope, RegexEq, Trigger,
};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::Color32F;
use smithay::input::keyboard::Keysym;
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size, Transform};

use crate::layout::focus_ring::{FocusRing, FocusRingRenderElement};
use crate::layout::LayoutElement;
use crate::niri::Niri;
use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::surface::render_snapshot_from_surface_tree;
use crate::render_helpers::texture::TextureBuffer;
use crate::render_helpers::{render_to_texture, BakedBuffer, RenderTarget, ToRenderElement};
use crate::utils::{output_size, with_toplevel_role};
use crate::window::mapped::MappedId;
use crate::window::{window_matches, Mapped, WindowRef};

// Factor by which to scale original window for its thumbnail
const THUMBNAIL_SCALE: f64 = 2.;

// Space to keep between sides of the output and first thumbnail, or between thumbnails
const SPACING: f64 = 50.;

// Corner radius on focus ring
const RADIUS: f32 = 6.;
// Alpha value for the focus ring
const FOCUS_RING_ALPHA: f32 = 0.9;

// Background color for the UI
const BACKGROUND: Color32F = Color32F::new(0., 0., 0., 0.7);

// Transition delay in ms for MRU UI open/close and wrap-arounds
pub const MRU_UI_TRANSITION_DELAY: u16 = 20;

/// Window MRU traversal context.
#[derive(Debug)]
pub struct WindowMru {
    /// List of window ids to be traversed in MRU order.
    ids: Vec<MappedId>,

    /// Current index in the MRU traversal.
    current: usize,
}

impl WindowMru {
    pub fn new(
        niri: &Niri,
        dir: MruDirection,
        scope: impl ToWindowIterator,
        filter: impl ToMatch,
    ) -> Self {
        // todo: maybe using a `Match` is overkill here and a plain app_id
        // compare would suffice
        let window_match = filter.to_match(niri);

        // Build a list of MappedId from the requested scope sorted by timestamp
        let mut ts_ids: Vec<(Option<Instant>, MappedId)> = scope
            .windows(niri)
            .filter(|w| {
                window_match.as_ref().is_none_or(|m| {
                    with_toplevel_role(w.toplevel(), |r| {
                        window_matches(WindowRef::Mapped(w), r, &m)
                    })
                })
            })
            .map(|w| (w.get_focus_timestamp(), w.id()))
            .collect();
        ts_ids.sort_by(|(t1, _), (t2, _)| match (t1, t2) {
            (None, None) => cmp::Ordering::Equal,
            (Some(_), None) => cmp::Ordering::Less,
            (None, Some(_)) => cmp::Ordering::Greater,
            (Some(t1), Some(t2)) => t1.cmp(t2).reverse(),
        });

        let mut ts_ids_it = ts_ids.into_iter().map(|(_, id)| id);

        // If moving backwards through the list, the first element is moved to the end of the list
        let first = match dir {
            MruDirection::Forward => None,
            MruDirection::Backward => ts_ids_it.next(),
        };
        let mut ids: Vec<MappedId> = ts_ids_it.collect();
        if let Some(f) = first {
            ids.push(f)
        }

        match dir {
            MruDirection::Forward => {
                let mut res = Self { ids, current: 0 };
                res.forward();
                res
            }
            MruDirection::Backward => {
                let current = ids.len().saturating_sub(1);
                let mut res = Self { ids, current };
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
}

type Thumbnail = BakedBuffer<TextureBuffer<GlesTexture>>;

pub enum WindowMruUi {
    Closed {},
    Open {
        wmru: WindowMru,
        textures: RefCell<TextureCache>,
        focus_ring: RefCell<FocusRing>,
    },
}

pub trait ToMatch {
    fn to_match(&self, niri: &Niri) -> Option<Match>;
}

impl ToMatch for MruFilter {
    fn to_match(&self, niri: &Niri) -> Option<Match> {
        let current_app_id = {
            let toplevel = niri.layout.active_workspace()?.active_window()?.toplevel();

            with_toplevel_role(toplevel, |r| r.app_id.clone())
        }?;

        Some(Match {
            app_id: Some(RegexEq::from_str(&format!("^{}$", current_app_id)).ok()?),
            ..Default::default()
        })
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

    pub fn open(&mut self, config: niri_config::FocusRing, wmru: WindowMru) {
        let Self::Closed {} = self else { return };
        let nids = wmru.ids.len();
        *self = Self::Open {
            wmru,
            textures: RefCell::new(TextureCache::with_capacity(nids)),
            focus_ring: RefCell::new(FocusRing::new(config)),
        };
    }

    pub fn close(&mut self) {
        let Self::Open { .. } = self else { return };
        *self = Self::Closed {};
    }

    pub fn advance(&mut self, dir: MruDirection, _scope: MruScope, _filter: MruFilter) {
        let Self::Open { wmru, .. } = self else {
            return;
        };
        match dir {
            MruDirection::Forward => wmru.forward(),
            MruDirection::Backward => wmru.backward(),
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
            wmru.ids.get(wmru.current).copied()
        }
    }

    pub fn remove_window(&mut self, id: MappedId) {
        let Self::Open { wmru, textures, .. } = self else {
            return;
        };
        if let Some(idx) = wmru.ids.iter().position(|v| *v == id) {
            wmru.ids.remove(idx);
            if wmru.current >= wmru.ids.len() {
                wmru.current = wmru.current.saturating_sub(1);
            }
            textures.borrow_mut().textures.remove(idx);
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
        } = self
        else {
            panic!("render_output on a non-open WindowMruUi");
        };

        let mut elements = Vec::new();
        let output_size = output_size(output);

        if !wmru.ids.is_empty() {
            let mut textures = textures.borrow_mut();

            let allowance = output_size.w - 2. * SPACING;

            let current = wmru.current;
            let current_texture_width = {
                let Some(t) = textures.get(niri, renderer, wmru, current) else {
                    return vec![];
                };
                t.buffer.logical_size().w
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
                            if let Some(t) = textures.get(niri, renderer, wmru, a) {
                                total_width += t.buffer.logical_size().w + SPACING;
                            }
                            (MruAlign::Left, l, a)
                        }
                        (None, Some(b)) => {
                            if let Some(t) = textures.get(niri, renderer, wmru, b) {
                                total_width += t.buffer.logical_size().w + SPACING;
                            }
                            (MruAlign::Right, b, r)
                        }
                        (Some(a), Some(b)) => {
                            if let Some(t) = textures.get(niri, renderer, wmru, a) {
                                total_width += t.buffer.logical_size().w + SPACING;
                            }
                            if let Some(t) = textures.get(niri, renderer, wmru, b) {
                                total_width += t.buffer.logical_size().w + SPACING;
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
                        if let Some(t) = textures.get(niri, renderer, wmru, idx) {
                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                true,
                                renderer,
                                if idx == current {
                                    Some(focus_ring)
                                } else {
                                    None
                                },
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
                        if let Some(t) = textures.get(niri, renderer, wmru, idx) {
                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                true,
                                renderer,
                                if idx == current {
                                    Some(focus_ring)
                                } else {
                                    None
                                },
                                &mut elements,
                            );
                        }
                    }

                    let mut location =
                        center - Point::from((current_texture_width / 2. + SPACING, 0.));
                    for idx in (left..current).rev() {
                        if let Some(t) = textures.get(niri, renderer, wmru, idx) {
                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                false,
                                renderer,
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
                        if let Some(t) = textures.get(niri, renderer, wmru, idx) {
                            render_elements_for_thumbnail(
                                t,
                                &mut location,
                                false,
                                renderer,
                                if idx == current {
                                    Some(focus_ring)
                                } else {
                                    None
                                },
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
    if forward {
        *location += Point::from((SPACING + bb_size.w, 0.));
    } else {
        *location -= Point::from((SPACING, 0.));
    }
}

pub struct TextureCache {
    textures: Vec<Option<Thumbnail>>,
}

impl TextureCache {
    fn with_capacity(size: usize) -> Self {
        let mut textures = Vec::with_capacity(size);
        textures.resize_with(textures.capacity(), Default::default);
        Self { textures }
    }

    fn get(
        &mut self,
        niri: &Niri,
        renderer: &mut GlesRenderer,
        wmru: &WindowMru,
        index: usize,
    ) -> Option<&Thumbnail> {
        if self.textures[index].is_none() {
            let id = wmru.ids[index];
            self.textures[index] = niri.layout.windows().find_map(|(_, mapped)| {
                if mapped.id() != id {
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
                    let tb = TextureBuffer::from_texture(
                        renderer,
                        texture,
                        THUMBNAIL_SCALE,
                        Transform::Normal,
                        vec![],
                    );

                    // wrap the texture into a BakedBuffer
                    BakedBuffer {
                        buffer: tb,
                        location: Point::default(),
                        src: None,
                        dst: None,
                    }
                })
            });
        }
        self.textures[index].as_ref()
    }
}

/// Key bindings available when the MRU UI is open.
/// Because the UI is closed when the Alt key is released, all bindings
/// have the ALT modifier.
pub const MRU_UI_BINDINGS: &[Bind] = &[
    // The first two are the same as those declared in in input::PRESET_BINDINGS
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Tab),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruAdvance(MruDirection::Forward, MruScope::All, MruFilter::None),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::Tab),
            modifiers: Modifiers::ALT.union(Modifiers::SHIFT),
        },
        action: Action::MruAdvance(MruDirection::Backward, MruScope::All, MruFilter::None),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
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
        action: Action::MruAdvance(MruDirection::Forward, MruScope::All, MruFilter::None),
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
        action: Action::MruAdvance(MruDirection::Backward, MruScope::All, MruFilter::None),
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
    // forward/backward bind actions for AppId navigation
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::grave),
            modifiers: Modifiers::ALT,
        },
        action: Action::MruAdvance(MruDirection::Forward, MruScope::All, MruFilter::AppId),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
    Bind {
        key: Key {
            trigger: Trigger::Keysym(Keysym::grave),
            modifiers: Modifiers::ALT.union(Modifiers::SHIFT),
        },
        action: Action::MruAdvance(MruDirection::Backward, MruScope::All, MruFilter::AppId),
        repeat: true,
        cooldown: None,
        allow_when_locked: false,
        allow_inhibiting: true,
        hotkey_overlay_title: None,
    },
];
