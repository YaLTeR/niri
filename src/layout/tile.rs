use std::cmp::max;
use std::rc::Rc;
use std::time::Duration;

use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::utils::{
    Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::backend::renderer::element::{Element, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use super::focus_ring::{FocusRing, FocusRingRenderElement};
use super::{
    LayoutElement, LayoutElementRenderElement, LayoutElementSnapshotRenderElements, Options,
};
use crate::animation::Animation;
use crate::niri_render_elements;
use crate::render_helpers::offscreen::OffscreenRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::{RenderSnapshot, RenderTarget};

/// Toplevel window with decorations.
#[derive(Debug)]
pub struct Tile<W: LayoutElement> {
    /// The toplevel window itself.
    window: W,

    /// The border around the window.
    border: FocusRing,

    /// The focus ring around the window.
    ///
    /// It's supposed to be on the Workspace, but for the sake of a nicer open animation it's
    /// currently here.
    focus_ring: FocusRing,

    /// Whether this tile is fullscreen.
    ///
    /// This will update only when the `window` actually goes fullscreen, rather than right away,
    /// to avoid black backdrop flicker before the window has had a chance to resize.
    is_fullscreen: bool,

    /// The black backdrop for fullscreen windows.
    fullscreen_backdrop: SolidColorBuffer,

    /// The size we were requested to fullscreen into.
    fullscreen_size: Size<i32, Logical>,

    /// The animation upon opening a window.
    open_animation: Option<Animation>,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

niri_render_elements! {
    TileRenderElement<R> => {
        LayoutElement = LayoutElementRenderElement<R>,
        FocusRing = FocusRingRenderElement,
        SolidColor = SolidColorRenderElement,
        Offscreen = RescaleRenderElement<OffscreenRenderElement>,
    }
}

niri_render_elements! {
    TileSnapshotRenderElement => {
        LayoutElement = RelocateRenderElement<LayoutElementSnapshotRenderElements>,
        FocusRing = FocusRingRenderElement,
        SolidColor = SolidColorRenderElement,
    }
}

impl<W: LayoutElement> Tile<W> {
    pub fn new(window: W, options: Rc<Options>) -> Self {
        Self {
            window,
            border: FocusRing::new(options.border.into()),
            focus_ring: FocusRing::new(options.focus_ring),
            is_fullscreen: false, // FIXME: up-to-date fullscreen right away, but we need size.
            fullscreen_backdrop: SolidColorBuffer::new((0, 0), [0., 0., 0., 1.]),
            fullscreen_size: Default::default(),
            open_animation: None,
            options,
        }
    }

    pub fn update_config(&mut self, options: Rc<Options>) {
        self.border.update_config(options.border.into());
        self.focus_ring.update_config(options.focus_ring);
        self.options = options;
    }

    pub fn update_window(&mut self) {
        // FIXME: remove when we can get a fullscreen size right away.
        if self.fullscreen_size != Size::from((0, 0)) {
            self.is_fullscreen = self.window.is_fullscreen();
        }
    }

    pub fn advance_animations(&mut self, current_time: Duration, is_active: bool) {
        let draw_border_with_background = self
            .window
            .rules()
            .draw_border_with_background
            .unwrap_or_else(|| !self.window.has_ssd());
        self.border
            .update(self.window.size(), !draw_border_with_background);
        self.border.set_active(is_active);

        let draw_focus_ring_with_background = if self.effective_border_width().is_some() {
            false
        } else {
            draw_border_with_background
        };
        self.focus_ring
            .update(self.tile_size(), !draw_focus_ring_with_background);
        self.focus_ring.set_active(is_active);

        match &mut self.open_animation {
            Some(anim) => {
                anim.set_current_time(current_time);
                if anim.is_done() {
                    self.open_animation = None;
                }
            }
            None => (),
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.open_animation.is_some()
    }

    pub fn start_open_animation(&mut self) {
        self.open_animation = Some(Animation::new(
            0.,
            1.,
            0.,
            self.options.animations.window_open,
            niri_config::Animation::default_window_open(),
        ));
    }

    pub fn open_animation(&self) -> &Option<Animation> {
        &self.open_animation
    }

    pub fn window(&self) -> &W {
        &self.window
    }

    pub fn window_mut(&mut self) -> &mut W {
        &mut self.window
    }

    pub fn into_window(self) -> W {
        self.window
    }

    /// Returns `None` if the border is hidden and `Some(width)` if it should be shown.
    fn effective_border_width(&self) -> Option<i32> {
        if self.is_fullscreen {
            return None;
        }

        if self.border.is_off() {
            return None;
        }

        Some(self.border.width())
    }

    /// Returns the location of the window's visual geometry within this Tile.
    pub fn window_loc(&self) -> Point<i32, Logical> {
        let mut loc = Point::from((0, 0));

        // In fullscreen, center the window in the given size.
        if self.is_fullscreen {
            let window_size = self.window.size();
            let target_size = self.fullscreen_size;

            // Windows aren't supposed to be larger than the fullscreen size, but in case we get
            // one, leave it at the top-left as usual.
            if window_size.w < target_size.w {
                loc.x += (target_size.w - window_size.w) / 2;
            }
            if window_size.h < target_size.h {
                loc.y += (target_size.h - window_size.h) / 2;
            }
        }

        if let Some(width) = self.effective_border_width() {
            loc += (width, width).into();
        }

        loc
    }

    pub fn tile_size(&self) -> Size<i32, Logical> {
        let mut size = self.window.size();

        if self.is_fullscreen {
            // Normally we'd just return the fullscreen size here, but this makes things a bit
            // nicer if a fullscreen window is bigger than the fullscreen size for some reason.
            size.w = max(size.w, self.fullscreen_size.w);
            size.h = max(size.h, self.fullscreen_size.h);
            return size;
        }

        if let Some(width) = self.effective_border_width() {
            size.w = size.w.saturating_add(width * 2);
            size.h = size.h.saturating_add(width * 2);
        }

        size
    }

    pub fn window_size(&self) -> Size<i32, Logical> {
        self.window.size()
    }

    pub fn buf_loc(&self) -> Point<i32, Logical> {
        let mut loc = Point::from((0, 0));
        loc += self.window_loc();
        loc += self.window.buf_loc();
        loc
    }

    pub fn is_in_input_region(&self, mut point: Point<f64, Logical>) -> bool {
        point -= self.window_loc().to_f64();
        self.window.is_in_input_region(point)
    }

    pub fn is_in_activation_region(&self, point: Point<f64, Logical>) -> bool {
        let activation_region = Rectangle::from_loc_and_size((0, 0), self.tile_size());
        activation_region.to_f64().contains(point)
    }

    pub fn request_tile_size(&mut self, mut size: Size<i32, Logical>) {
        // Can't go through effective_border_width() because we might be fullscreen.
        if !self.border.is_off() {
            let width = self.border.width();
            size.w = max(1, size.w - width * 2);
            size.h = max(1, size.h - width * 2);
        }

        self.window.request_size(size);
    }

    pub fn tile_width_for_window_width(&self, size: i32) -> i32 {
        if self.border.is_off() {
            size
        } else {
            size.saturating_add(self.border.width() * 2)
        }
    }

    pub fn tile_height_for_window_height(&self, size: i32) -> i32 {
        if self.border.is_off() {
            size
        } else {
            size.saturating_add(self.border.width() * 2)
        }
    }

    pub fn window_height_for_tile_height(&self, size: i32) -> i32 {
        if self.border.is_off() {
            size
        } else {
            size.saturating_sub(self.border.width() * 2)
        }
    }

    pub fn request_fullscreen(&mut self, size: Size<i32, Logical>) {
        self.fullscreen_backdrop.resize(size);
        self.fullscreen_size = size;
        self.window.request_fullscreen(size);
    }

    pub fn min_size(&self) -> Size<i32, Logical> {
        let mut size = self.window.min_size();

        if let Some(width) = self.effective_border_width() {
            size.w = max(1, size.w);
            size.h = max(1, size.h);

            size.w = size.w.saturating_add(width * 2);
            size.h = size.h.saturating_add(width * 2);
        }

        size
    }

    pub fn max_size(&self) -> Size<i32, Logical> {
        let mut size = self.window.max_size();

        if let Some(width) = self.effective_border_width() {
            if size.w > 0 {
                size.w = size.w.saturating_add(width * 2);
            }
            if size.h > 0 {
                size.h = size.h.saturating_add(width * 2);
            }
        }

        size
    }

    pub fn draw_border_with_background(&self) -> bool {
        if self.effective_border_width().is_some() {
            return false;
        }

        self.window
            .rules()
            .draw_border_with_background
            .unwrap_or_else(|| !self.window.has_ssd())
    }

    fn render_inner<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        view_size: Size<i32, Logical>,
        focus_ring: bool,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> {
        let alpha = if self.is_fullscreen {
            1.
        } else {
            self.window.rules().opacity.unwrap_or(1.).clamp(0., 1.)
        };

        let rv = self
            .window
            .render(renderer, location + self.window_loc(), scale, alpha, target)
            .into_iter()
            .map(Into::into);

        let elem = self.effective_border_width().map(|width| {
            self.border
                .render(
                    renderer,
                    location + Point::from((width, width)),
                    scale,
                    view_size,
                )
                .map(Into::into)
        });
        let rv = rv.chain(elem.into_iter().flatten());

        let elem = focus_ring.then(|| {
            self.focus_ring
                .render(renderer, location, scale, view_size)
                .map(Into::into)
        });
        let rv = rv.chain(elem.into_iter().flatten());

        let elem = self.is_fullscreen.then(|| {
            SolidColorRenderElement::from_buffer(
                &self.fullscreen_backdrop,
                location.to_physical_precise_round(scale),
                scale,
                1.,
                Kind::Unspecified,
            )
            .into()
        });
        rv.chain(elem)
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        view_size: Size<i32, Logical>,
        focus_ring: bool,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> {
        if let Some(anim) = &self.open_animation {
            let renderer = renderer.as_gles_renderer();
            let elements =
                self.render_inner(renderer, location, scale, view_size, focus_ring, target);
            let elements = elements.collect::<Vec<TileRenderElement<_>>>();

            let elem = OffscreenRenderElement::new(
                renderer,
                scale.x as i32,
                &elements,
                anim.value().clamp(0., 1.) as f32,
            );
            self.window()
                .set_offscreen_element_id(Some(elem.id().clone()));

            let mut center = location;
            center.x += self.tile_size().w / 2;
            center.y += self.tile_size().h / 2;

            Some(TileRenderElement::Offscreen(
                RescaleRenderElement::from_element(
                    elem,
                    center.to_physical_precise_round(scale),
                    (anim.value() / 2. + 0.5).max(0.),
                ),
            ))
            .into_iter()
            .chain(None.into_iter().flatten())
        } else {
            self.window().set_offscreen_element_id(None);

            let elements =
                self.render_inner(renderer, location, scale, view_size, focus_ring, target);
            None.into_iter().chain(Some(elements).into_iter().flatten())
        }
    }

    pub fn take_snapshot_for_close_anim(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
        view_size: Size<i32, Logical>,
    ) -> RenderSnapshot<TileSnapshotRenderElement> {
        let snapshot = self.window.take_last_render();
        if snapshot.contents.is_empty() {
            return RenderSnapshot::default();
        }

        let mut process = |contents| {
            let mut rv = vec![];

            let buf_pos =
                (self.window_loc() + self.window.buf_loc()).to_physical_precise_round(scale);
            for elem in contents {
                let elem = RelocateRenderElement::from_element(elem, buf_pos, Relocate::Relative);
                rv.push(elem.into());
            }

            if let Some(width) = self.effective_border_width() {
                rv.extend(
                    self.border
                        .render(renderer, Point::from((width, width)), scale, view_size)
                        .map(Into::into),
                );
            }

            if self.is_fullscreen {
                let elem = SolidColorRenderElement::from_buffer(
                    &self.fullscreen_backdrop,
                    Point::from((0, 0)),
                    scale,
                    1.,
                    Kind::Unspecified,
                );
                rv.push(elem.into());
            }

            rv
        };

        RenderSnapshot {
            contents: process(snapshot.contents),
            blocked_out_contents: process(snapshot.blocked_out_contents),
            block_out_from: snapshot.block_out_from,
        }
    }
}
