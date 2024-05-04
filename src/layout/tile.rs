use std::cell::RefCell;
use std::cmp::max;
use std::rc::Rc;
use std::time::Duration;

use niri_config::CornerRadius;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::utils::RescaleRenderElement;
use smithay::backend::renderer::element::{Element, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size, Transform};

use super::focus_ring::{FocusRing, FocusRingRenderElement};
use super::{
    LayoutElement, LayoutElementRenderElement, LayoutElementRenderSnapshot, Options,
    RESIZE_ANIMATION_THRESHOLD,
};
use crate::animation::Animation;
use crate::niri_render_elements;
use crate::render_helpers::border::BorderRenderElement;
use crate::render_helpers::clipped_surface::{ClippedSurfaceRenderElement, RoundedCornerDamage};
use crate::render_helpers::damage::ExtraDamage;
use crate::render_helpers::offscreen::OffscreenRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::resize::ResizeRenderElement;
use crate::render_helpers::snapshot::RenderSnapshot;
use crate::render_helpers::{render_to_encompassing_texture, RenderTarget};

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

    /// The animation of the window resizing.
    resize_animation: Option<ResizeAnimation>,

    /// The animation of a tile visually moving horizontally.
    move_x_animation: Option<MoveAnimation>,

    /// The animation of a tile visually moving vertically.
    move_y_animation: Option<MoveAnimation>,

    /// Snapshot of the last render for use in the close animation.
    unmap_snapshot: RefCell<Option<TileRenderSnapshot>>,

    /// Extra damage for clipped surface corner radius changes.
    rounded_corner_damage: RoundedCornerDamage,

    /// Configurable properties of the layout.
    pub options: Rc<Options>,
}

niri_render_elements! {
    TileRenderElement<R> => {
        LayoutElement = LayoutElementRenderElement<R>,
        FocusRing = FocusRingRenderElement,
        SolidColor = SolidColorRenderElement,
        Offscreen = RescaleRenderElement<OffscreenRenderElement>,
        Resize = ResizeRenderElement,
        Border = BorderRenderElement,
        ClippedSurface = ClippedSurfaceRenderElement<R>,
        ExtraDamage = ExtraDamage,
    }
}

type TileRenderSnapshot =
    RenderSnapshot<TileRenderElement<GlesRenderer>, TileRenderElement<GlesRenderer>>;

#[derive(Debug)]
struct ResizeAnimation {
    anim: Animation,
    size_from: Size<i32, Logical>,
    snapshot: LayoutElementRenderSnapshot,
}

#[derive(Debug)]
struct MoveAnimation {
    anim: Animation,
    from: i32,
}

impl<W: LayoutElement> Tile<W> {
    pub fn new(window: W, options: Rc<Options>) -> Self {
        let rules = window.rules();
        let border_config = rules.border.resolve_against(options.border);
        let focus_ring_config = rules.focus_ring.resolve_against(options.focus_ring.into());

        Self {
            window,
            border: FocusRing::new(border_config.into()),
            focus_ring: FocusRing::new(focus_ring_config.into()),
            is_fullscreen: false, // FIXME: up-to-date fullscreen right away, but we need size.
            fullscreen_backdrop: SolidColorBuffer::new((0, 0), [0., 0., 0., 1.]),
            fullscreen_size: Default::default(),
            open_animation: None,
            resize_animation: None,
            move_x_animation: None,
            move_y_animation: None,
            unmap_snapshot: RefCell::new(None),
            rounded_corner_damage: Default::default(),
            options,
        }
    }

    pub fn update_config(&mut self, options: Rc<Options>) {
        self.options = options;

        let rules = self.window.rules();

        let border_config = rules.border.resolve_against(self.options.border);
        self.border.update_config(border_config.into());

        let focus_ring_config = rules
            .focus_ring
            .resolve_against(self.options.focus_ring.into());
        self.focus_ring.update_config(focus_ring_config.into());
    }

    pub fn update_shaders(&mut self) {
        self.border.update_shaders();
        self.focus_ring.update_shaders();
    }

    pub fn update_window(&mut self) {
        // FIXME: remove when we can get a fullscreen size right away.
        if self.fullscreen_size != Size::from((0, 0)) {
            self.is_fullscreen = self.window.is_fullscreen();
        }

        if let Some(animate_from) = self.window.take_animation_snapshot() {
            let size_from = if let Some(resize) = self.resize_animation.take() {
                // Compute like in animated_window_size(), but using the snapshot geometry (since
                // the current one is already overwritten).
                let mut size = animate_from.size;

                let val = resize.anim.value();
                let size_from = resize.size_from;

                size.w = (size_from.w as f64 + (size.w - size_from.w) as f64 * val).round() as i32;
                size.h = (size_from.h as f64 + (size.h - size_from.h) as f64 * val).round() as i32;

                size
            } else {
                animate_from.size
            };

            let change = self.window.size().to_point() - size_from.to_point();
            let change = max(change.x.abs(), change.y.abs());
            if change > RESIZE_ANIMATION_THRESHOLD {
                let anim = Animation::new(0., 1., 0., self.options.animations.window_resize.anim);
                self.resize_animation = Some(ResizeAnimation {
                    anim,
                    size_from,
                    snapshot: animate_from,
                });
            } else {
                self.resize_animation = None;
            }
        }

        let rules = self.window.rules();
        let border_config = rules.border.resolve_against(self.options.border);
        self.border.update_config(border_config.into());
        let focus_ring_config = rules
            .focus_ring
            .resolve_against(self.options.focus_ring.into());
        self.focus_ring.update_config(focus_ring_config.into());

        let window_size = self.window_size();
        let radius = rules
            .geometry_corner_radius
            .unwrap_or_default()
            .fit_to(window_size.w as f32, window_size.h as f32);
        self.rounded_corner_damage.set_corner_radius(radius);
        self.rounded_corner_damage.set_size(window_size);
    }

    pub fn advance_animations(&mut self, current_time: Duration) {
        if let Some(anim) = &mut self.open_animation {
            anim.set_current_time(current_time);
            if anim.is_done() {
                self.open_animation = None;
            }
        }

        if let Some(resize) = &mut self.resize_animation {
            resize.anim.set_current_time(current_time);
            if resize.anim.is_done() {
                self.resize_animation = None;
            }
        }

        if let Some(move_) = &mut self.move_x_animation {
            move_.anim.set_current_time(current_time);
            if move_.anim.is_done() {
                self.move_x_animation = None;
            }
        }
        if let Some(move_) = &mut self.move_y_animation {
            move_.anim.set_current_time(current_time);
            if move_.anim.is_done() {
                self.move_y_animation = None;
            }
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.open_animation.is_some()
            || self.resize_animation.is_some()
            || self.move_x_animation.is_some()
            || self.move_y_animation.is_some()
    }

    pub fn update(&mut self, is_active: bool, mut view_rect: Rectangle<i32, Logical>) {
        view_rect.loc -= self.render_offset();

        let rules = self.window.rules();

        let draw_border_with_background = rules
            .draw_border_with_background
            .unwrap_or_else(|| !self.window.has_ssd());
        let border_width = self.effective_border_width().unwrap_or(0);
        let radius = if self.is_fullscreen {
            CornerRadius::default()
        } else {
            rules
                .geometry_corner_radius
                .map_or(CornerRadius::default(), |radius| {
                    radius.expanded_by(border_width as f32)
                })
        };
        self.border.update_render_elements(
            self.animated_window_size(),
            is_active,
            !draw_border_with_background,
            Rectangle::from_loc_and_size(
                view_rect.loc - Point::from((border_width, border_width)),
                view_rect.size,
            ),
            radius,
        );

        let draw_focus_ring_with_background = if self.effective_border_width().is_some() {
            false
        } else {
            draw_border_with_background
        };
        let radius = if self.is_fullscreen {
            CornerRadius::default()
        } else if self.effective_border_width().is_some() {
            radius
        } else {
            rules.geometry_corner_radius.unwrap_or_default()
        }
        .expanded_by(self.focus_ring.width() as f32);
        self.focus_ring.update_render_elements(
            self.animated_tile_size(),
            is_active,
            !draw_focus_ring_with_background,
            view_rect,
            radius,
        );
    }

    pub fn render_offset(&self) -> Point<i32, Logical> {
        let mut offset = Point::from((0., 0.));

        if let Some(move_) = &self.move_x_animation {
            offset.x += f64::from(move_.from) * move_.anim.value();
        }
        if let Some(move_) = &self.move_y_animation {
            offset.y += f64::from(move_.from) * move_.anim.value();
        }

        offset.to_i32_round()
    }

    pub fn start_open_animation(&mut self) {
        self.open_animation = Some(Animation::new(
            0.,
            1.,
            0.,
            self.options.animations.window_open.0,
        ));
    }

    pub fn open_animation(&self) -> &Option<Animation> {
        &self.open_animation
    }

    pub fn resize_animation(&self) -> Option<&Animation> {
        self.resize_animation.as_ref().map(|resize| &resize.anim)
    }

    pub fn animate_move_from(&mut self, from: Point<i32, Logical>) {
        self.animate_move_x_from(from.x);
        self.animate_move_y_from(from.y);
    }

    pub fn animate_move_x_from(&mut self, from: i32) {
        self.animate_move_x_from_with_config(from, self.options.animations.window_movement.0);
    }

    pub fn animate_move_x_from_with_config(&mut self, from: i32, config: niri_config::Animation) {
        let current_offset = self.render_offset().x;

        // Preserve the previous config if ongoing.
        let anim = self.move_x_animation.take().map(|move_| move_.anim);
        let anim = anim
            .map(|anim| anim.restarted(1., 0., 0.))
            .unwrap_or_else(|| Animation::new(1., 0., 0., config));

        self.move_x_animation = Some(MoveAnimation {
            anim,
            from: from + current_offset,
        });
    }

    pub fn animate_move_y_from(&mut self, from: i32) {
        self.animate_move_y_from_with_config(from, self.options.animations.window_movement.0);
    }

    pub fn animate_move_y_from_with_config(&mut self, from: i32, config: niri_config::Animation) {
        let current_offset = self.render_offset().y;

        // Preserve the previous config if ongoing.
        let anim = self.move_y_animation.take().map(|move_| move_.anim);
        let anim = anim
            .map(|anim| anim.restarted(1., 0., 0.))
            .unwrap_or_else(|| Animation::new(1., 0., 0., config));

        self.move_y_animation = Some(MoveAnimation {
            anim,
            from: from + current_offset,
        });
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

    pub fn is_fullscreen(&self) -> bool {
        self.is_fullscreen
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

    fn animated_window_size(&self) -> Size<i32, Logical> {
        let mut size = self.window.size();

        if let Some(resize) = &self.resize_animation {
            let val = resize.anim.value();
            let size_from = resize.size_from;

            size.w = (size_from.w as f64 + (size.w - size_from.w) as f64 * val).round() as i32;
            size.w = max(1, size.w);
            size.h = (size_from.h as f64 + (size.h - size_from.h) as f64 * val).round() as i32;
            size.h = max(1, size.h);
        }

        size
    }

    fn animated_tile_size(&self) -> Size<i32, Logical> {
        let mut size = self.animated_window_size();

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

    pub fn request_tile_size(&mut self, mut size: Size<i32, Logical>, animate: bool) {
        // Can't go through effective_border_width() because we might be fullscreen.
        if !self.border.is_off() {
            let width = self.border.width();
            size.w = max(1, size.w - width * 2);
            size.h = max(1, size.h - width * 2);
        }

        self.window.request_size(size, animate);
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
        focus_ring: bool,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> {
        let _span = tracy_client::span!("Tile::render_inner");

        let alpha = if self.is_fullscreen {
            1.
        } else {
            self.window.rules().opacity.unwrap_or(1.).clamp(0., 1.)
        };

        let window_loc = self.window_loc();
        let window_size = self.window_size();
        let animated_window_size = self.animated_window_size();
        let window_render_loc = location + window_loc;
        let area = Rectangle::from_loc_and_size(window_render_loc, animated_window_size);

        let rules = self.window.rules();
        let clip_to_geometry = !self.is_fullscreen && rules.clip_to_geometry == Some(true);
        let radius = rules.geometry_corner_radius.unwrap_or_default();

        // If we're resizing, try to render a shader, or a fallback.
        let mut resize_shader = None;
        let mut resize_popups = None;
        let mut resize_fallback = None;

        if let Some(resize) = &self.resize_animation {
            resize_popups = Some(
                self.window
                    .render_popups(renderer, window_render_loc, scale, alpha, target)
                    .into_iter()
                    .map(Into::into),
            );

            if ResizeRenderElement::has_shader(renderer) {
                let gles_renderer = renderer.as_gles_renderer();

                if let Some(texture_from) = resize.snapshot.texture(gles_renderer, scale, target) {
                    let window_elements = self.window.render_normal(
                        gles_renderer,
                        Point::from((0, 0)),
                        scale,
                        1.,
                        target,
                    );

                    let current = render_to_encompassing_texture(
                        gles_renderer,
                        scale,
                        Transform::Normal,
                        Fourcc::Abgr8888,
                        &window_elements,
                    )
                    .map_err(|err| warn!("error rendering window to texture: {err:?}"))
                    .ok();

                    // Clip blocked-out resizes unconditionally because they use solid color render
                    // elements.
                    let clip_to_geometry = if target
                        .should_block_out(resize.snapshot.block_out_from)
                        && target.should_block_out(rules.block_out_from)
                    {
                        true
                    } else {
                        clip_to_geometry
                    };

                    if let Some((texture_current, _sync_point, texture_current_geo)) = current {
                        let elem = ResizeRenderElement::new(
                            area,
                            scale,
                            texture_from.clone(),
                            resize.snapshot.size,
                            (texture_current, texture_current_geo),
                            window_size,
                            resize.anim.value() as f32,
                            resize.anim.clamped_value().clamp(0., 1.) as f32,
                            radius,
                            clip_to_geometry,
                            alpha,
                        );
                        // FIXME: with split popups, this will use the resize element ID for
                        // popups, but we want the real IDs.
                        self.window
                            .set_offscreen_element_id(Some(elem.id().clone()));
                        resize_shader = Some(elem.into());
                    }
                }
            }

            if resize_shader.is_none() {
                let fallback_buffer = SolidColorBuffer::new(area.size, [1., 0., 0., 1.]);
                resize_fallback = Some(
                    SolidColorRenderElement::from_buffer(
                        &fallback_buffer,
                        area.loc.to_physical_precise_round(scale),
                        scale,
                        alpha,
                        Kind::Unspecified,
                    )
                    .into(),
                );
                self.window.set_offscreen_element_id(None);
            }
        }

        // If we're not resizing, render the window itself.
        let mut window_surface = None;
        let mut window_popups = None;
        let mut rounded_corner_damage = None;
        if resize_shader.is_none() && resize_fallback.is_none() {
            let window = self
                .window
                .render(renderer, window_render_loc, scale, alpha, target);

            let geo = Rectangle::from_loc_and_size(window_render_loc, window_size);
            let radius = radius.fit_to(window_size.w as f32, window_size.h as f32);

            let clip_shader = ClippedSurfaceRenderElement::shader(renderer).cloned();
            let has_border_shader = BorderRenderElement::has_shader(renderer);

            if clip_to_geometry && clip_shader.is_some() {
                let damage = self.rounded_corner_damage.element();
                rounded_corner_damage = Some(damage.with_location(window_render_loc).into());
            }

            window_surface = Some(window.normal.into_iter().map(move |elem| match elem {
                LayoutElementRenderElement::Wayland(elem) => {
                    // If we should clip to geometry, render a clipped window.
                    if clip_to_geometry {
                        if let Some(shader) = clip_shader.clone() {
                            if ClippedSurfaceRenderElement::will_clip(&elem, scale, geo, radius) {
                                return ClippedSurfaceRenderElement::new(
                                    elem,
                                    scale,
                                    geo,
                                    shader.clone(),
                                    radius,
                                )
                                .into();
                            }
                        }
                    }

                    // Otherwise, render it normally.
                    LayoutElementRenderElement::Wayland(elem).into()
                }
                LayoutElementRenderElement::SolidColor(elem) => {
                    // In this branch we're rendering a blocked-out window with a solid
                    // color. We need to render it with a rounded corner shader even if
                    // clip_to_geometry is false, because in this case we're assuming that
                    // the unclipped window CSD already has corners rounded to the
                    // user-provided radius, so our blocked-out rendering should match that
                    // radius.
                    if radius != CornerRadius::default() && has_border_shader {
                        return BorderRenderElement::new(
                            geo.size,
                            Rectangle::from_loc_and_size((0, 0), geo.size),
                            elem.color(),
                            elem.color(),
                            0.,
                            Rectangle::from_loc_and_size((0, 0), geo.size),
                            0.,
                            radius,
                        )
                        .with_location(geo.loc)
                        .into();
                    }

                    // Otherwise, render the solid color as is.
                    LayoutElementRenderElement::SolidColor(elem).into()
                }
            }));

            window_popups = Some(window.popups.into_iter().map(Into::into));
        }

        let rv = resize_popups
            .into_iter()
            .flatten()
            .chain(resize_shader)
            .chain(resize_fallback)
            .chain(window_popups.into_iter().flatten())
            .chain(rounded_corner_damage)
            .chain(window_surface.into_iter().flatten());

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
        let rv = rv.chain(elem);

        let elem = self.effective_border_width().map(|width| {
            self.border
                .render(renderer, location + Point::from((width, width)), scale)
                .map(Into::into)
        });
        let rv = rv.chain(elem.into_iter().flatten());

        let elem = focus_ring.then(|| {
            self.focus_ring
                .render(renderer, location, scale)
                .map(Into::into)
        });
        rv.chain(elem.into_iter().flatten())
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        focus_ring: bool,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> {
        let _span = tracy_client::span!("Tile::render");

        if let Some(anim) = &self.open_animation {
            let renderer = renderer.as_gles_renderer();
            let elements = self.render_inner(renderer, location, scale, focus_ring, target);
            let elements = elements.collect::<Vec<TileRenderElement<_>>>();

            let elem = OffscreenRenderElement::new(
                renderer,
                scale.x as i32,
                &elements,
                anim.clamped_value().clamp(0., 1.) as f32,
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

            let elements = self.render_inner(renderer, location, scale, focus_ring, target);
            None.into_iter().chain(Some(elements).into_iter().flatten())
        }
    }

    pub fn store_unmap_snapshot_if_empty(
        &mut self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
    ) {
        if self.unmap_snapshot.get_mut().is_some() {
            return;
        }

        *self.unmap_snapshot.get_mut() = Some(self.render_snapshot(renderer, scale));
    }

    fn render_snapshot(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
    ) -> TileRenderSnapshot {
        let _span = tracy_client::span!("Tile::render_snapshot");

        let contents = self.render_inner(
            renderer,
            Point::from((0, 0)),
            scale,
            false,
            RenderTarget::Output,
        );

        // A bit of a hack to render blocked out as for screencast, but I think it's fine here.
        let blocked_out_contents = self.render_inner(
            renderer,
            Point::from((0, 0)),
            scale,
            false,
            RenderTarget::Screencast,
        );

        RenderSnapshot {
            contents: contents.collect(),
            blocked_out_contents: blocked_out_contents.collect(),
            block_out_from: self.window.rules().block_out_from,
            size: self.animated_tile_size(),
            texture: Default::default(),
            blocked_out_texture: Default::default(),
        }
    }

    pub fn take_unmap_snapshot(&self) -> Option<TileRenderSnapshot> {
        self.unmap_snapshot.take()
    }
}
