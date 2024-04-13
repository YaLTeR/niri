use std::cell::OnceCell;
use std::cmp::max;
use std::rc::Rc;
use std::time::Duration;

use niri_config::BlockOutFrom;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::utils::RescaleRenderElement;
use smithay::backend::renderer::element::{Element, Kind};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use super::focus_ring::{FocusRing, FocusRingRenderElement};
use super::{
    AnimationSnapshot, LayoutElement, LayoutElementRenderElement, Options,
    RESIZE_ANIMATION_THRESHOLD,
};
use crate::animation::Animation;
use crate::niri_render_elements;
use crate::render_helpers::crossfade::CrossfadeRenderElement;
use crate::render_helpers::offscreen::OffscreenRenderElement;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::shaders::Shaders;
use crate::render_helpers::{
    render_to_encompassing_texture, RenderSnapshot, RenderTarget, ToRenderElement,
};

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

    /// The animation of a tile visually moving.
    move_animation: Option<MoveAnimation>,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

niri_render_elements! {
    TileRenderElement<R> => {
        LayoutElement = LayoutElementRenderElement<R>,
        FocusRing = FocusRingRenderElement,
        SolidColor = SolidColorRenderElement,
        Offscreen = RescaleRenderElement<OffscreenRenderElement>,
        Crossfade = CrossfadeRenderElement,
    }
}

niri_render_elements! {
    TileSnapshotContentsRenderElement => {
        Texture = PrimaryGpuTextureRenderElement,
        SolidColor = SolidColorRenderElement,
    }
}

niri_render_elements! {
    TileSnapshotRenderElement => {
        Contents = RescaleRenderElement<TileSnapshotContentsRenderElement>,
        FocusRing = FocusRingRenderElement,
        SolidColor = SolidColorRenderElement,
    }
}

#[derive(Debug)]
struct ResizeAnimation {
    anim: Animation,
    size_from: Size<i32, Logical>,
    snapshot: AnimationSnapshot,
    /// Snapshot rendered into a texture (happens lazily).
    snapshot_texture: OnceCell<Option<(GlesTexture, Rectangle<i32, Physical>)>>,
    snapshot_blocked_out_texture: OnceCell<Option<(GlesTexture, Rectangle<i32, Physical>)>>,
}

#[derive(Debug)]
struct MoveAnimation {
    anim: Animation,
    from: Point<i32, Logical>,
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
            resize_animation: None,
            move_animation: None,
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
                let anim = Animation::new(
                    0.,
                    1.,
                    0.,
                    self.options.animations.window_resize,
                    niri_config::Animation::default_window_resize(),
                );
                self.resize_animation = Some(ResizeAnimation {
                    anim,
                    size_from,
                    snapshot: animate_from,
                    snapshot_texture: OnceCell::new(),
                    snapshot_blocked_out_texture: OnceCell::new(),
                });
            } else {
                self.resize_animation = None;
            }
        }
    }

    pub fn advance_animations(&mut self, current_time: Duration, is_active: bool) {
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

        if let Some(move_) = &mut self.move_animation {
            move_.anim.set_current_time(current_time);
            if move_.anim.is_done() {
                self.move_animation = None;
            }
        }

        let draw_border_with_background = self
            .window
            .rules()
            .draw_border_with_background
            .unwrap_or_else(|| !self.window.has_ssd());
        self.border
            .update(self.animated_window_size(), !draw_border_with_background);
        self.border.set_active(is_active);

        let draw_focus_ring_with_background = if self.effective_border_width().is_some() {
            false
        } else {
            draw_border_with_background
        };
        self.focus_ring
            .update(self.animated_tile_size(), !draw_focus_ring_with_background);
        self.focus_ring.set_active(is_active);
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.open_animation.is_some()
            || self.resize_animation.is_some()
            || self.move_animation.is_some()
    }

    pub fn render_offset(&self) -> Point<i32, Logical> {
        let mut offset = Point::from((0., 0.));

        if let Some(move_) = &self.move_animation {
            offset += move_.from.to_f64().upscale(move_.anim.value());
        }

        offset.to_i32_round()
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

    pub fn resize_animation(&self) -> Option<&Animation> {
        self.resize_animation.as_ref().map(|resize| &resize.anim)
    }

    pub fn animate_move_from_with_config(
        &mut self,
        from: Point<i32, Logical>,
        config: niri_config::Animation,
        default: niri_config::Animation,
    ) {
        let current_offset = self.render_offset();

        self.move_animation = Some(MoveAnimation {
            anim: Animation::new(1., 0., 0., config, default),
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
            size.h = (size_from.h as f64 + (size.h - size_from.h) as f64 * val).round() as i32;
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
        view_size: Size<i32, Logical>,
        focus_ring: bool,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> {
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

        let gles_renderer = renderer.as_gles_renderer();

        // If we're resizing, try to render a crossfade, or a fallback.
        let mut crossfade = None;
        let mut crossfade_fallback = None;

        if let Some(resize) = &self.resize_animation {
            if Shaders::get(gles_renderer).crossfade.is_some() {
                if let Some(texture_from) = resize.rendered_texture(gles_renderer, scale, target) {
                    let window_elements =
                        self.window
                            .render(gles_renderer, Point::from((0, 0)), scale, 1., target);
                    let current = render_to_encompassing_texture(
                        gles_renderer,
                        scale,
                        Transform::Normal,
                        Fourcc::Abgr8888,
                        &window_elements,
                    )
                    .map_err(|err| warn!("error rendering window to texture: {err:?}"))
                    .ok();

                    if let Some((texture_current, _sync_point, texture_current_geo)) = current {
                        let elem = CrossfadeRenderElement::new(
                            gles_renderer,
                            area,
                            scale,
                            texture_from.clone(),
                            resize.snapshot.size,
                            (texture_current, texture_current_geo),
                            window_size,
                            resize.anim.clamped_value().clamp(0., 1.) as f32,
                            alpha,
                        )
                        .expect("we checked the crossfade shader above");
                        self.window
                            .set_offscreen_element_id(Some(elem.id().clone()));
                        crossfade = Some(elem.into());
                    }
                }
            }

            if crossfade.is_none() {
                let fallback_buffer = SolidColorBuffer::new(area.size, [1., 0., 0., 1.]);
                crossfade_fallback = Some(
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
        let mut window = None;
        if crossfade.is_none() && crossfade_fallback.is_none() {
            window = Some(
                self.window
                    .render(renderer, window_render_loc, scale, alpha, target)
                    .into_iter()
                    .map(Into::into),
            );
        }

        let rv = crossfade
            .into_iter()
            .chain(crossfade_fallback)
            .chain(window.into_iter().flatten());

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

            let elements =
                self.render_inner(renderer, location, scale, view_size, focus_ring, target);
            None.into_iter().chain(Some(elements).into_iter().flatten())
        }
    }

    fn render_snapshot<E, C>(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
        view_size: Size<i32, Logical>,
        contents: Vec<C>,
    ) -> Vec<TileSnapshotRenderElement>
    where
        E: Into<TileSnapshotContentsRenderElement>,
        C: ToRenderElement<RenderElement = E>,
    {
        let alpha = if self.is_fullscreen {
            1.
        } else {
            self.window.rules().opacity.unwrap_or(1.).clamp(0., 1.)
        };

        let window_size = self.window_size();
        let animated_window_size = self.animated_window_size();
        let animated_scale = animated_window_size.to_f64() / window_size.to_f64();

        let mut rv = vec![];

        for baked in contents {
            let elem = baked.to_render_element(self.window_loc(), scale, alpha, Kind::Unspecified);
            let elem: TileSnapshotContentsRenderElement = elem.into();

            let origin = self.window_loc().to_physical_precise_round(scale);
            let elem = RescaleRenderElement::from_element(elem, origin, animated_scale);
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
    }

    pub fn take_snapshot_for_close_anim(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
        view_size: Size<i32, Logical>,
    ) -> RenderSnapshot<TileSnapshotRenderElement, TileSnapshotRenderElement> {
        let snapshot = self.window.take_last_render();
        if snapshot.contents.is_empty() {
            return RenderSnapshot::default();
        }

        RenderSnapshot {
            contents: self.render_snapshot(renderer, scale, view_size, snapshot.contents),
            blocked_out_contents: self.render_snapshot(
                renderer,
                scale,
                view_size,
                snapshot.blocked_out_contents,
            ),
            block_out_from: snapshot.block_out_from,
        }
    }
}

impl ResizeAnimation {
    fn rendered_texture(
        &self,
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
        target: RenderTarget,
    ) -> &Option<(GlesTexture, Rectangle<i32, Physical>)> {
        let block_out = match self.snapshot.render.block_out_from {
            None => false,
            Some(BlockOutFrom::Screencast) => target == RenderTarget::Screencast,
            Some(BlockOutFrom::ScreenCapture) => target != RenderTarget::Output,
        };

        if block_out {
            self.snapshot_blocked_out_texture.get_or_init(|| {
                let _span = tracy_client::span!("ResizeAnimation::rendered_texture");

                let elements: Vec<_> = self
                    .snapshot
                    .render
                    .blocked_out_contents
                    .iter()
                    .map(|baked| {
                        baked.to_render_element(Point::from((0, 0)), scale, 1., Kind::Unspecified)
                    })
                    .collect();

                match render_to_encompassing_texture(
                    renderer,
                    scale,
                    Transform::Normal,
                    Fourcc::Abgr8888,
                    &elements,
                ) {
                    Ok((texture, _sync_point, geo)) => Some((texture, geo)),
                    Err(err) => {
                        warn!("error rendering snapshot to texture: {err:?}");
                        None
                    }
                }
            })
        } else {
            self.snapshot_texture.get_or_init(|| {
                let _span = tracy_client::span!("ResizeAnimation::rendered_texture");

                let elements: Vec<_> = self
                    .snapshot
                    .render
                    .contents
                    .iter()
                    .map(|baked| {
                        baked.to_render_element(Point::from((0, 0)), scale, 1., Kind::Unspecified)
                    })
                    .collect();

                match render_to_encompassing_texture(
                    renderer,
                    scale,
                    Transform::Normal,
                    Fourcc::Abgr8888,
                    &elements,
                ) {
                    Ok((texture, _sync_point, geo)) => Some((texture, geo)),
                    Err(err) => {
                        warn!("error rendering snapshot to texture: {err:?}");
                        None
                    }
                }
            })
        }
    }
}
