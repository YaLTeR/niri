use core::f64;
use std::rc::Rc;

use niri_config::{Color, CornerRadius, GradientInterpolation};
use smithay::backend::renderer::element::{Element, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use super::focus_ring::{FocusRing, FocusRingRenderElement};
use super::opening_window::{OpenAnimation, OpeningWindowRenderElement};
use super::shadow::Shadow;
use super::{
    HitType, LayoutElement, LayoutElementRenderElement, LayoutElementRenderSnapshot, Options,
    SizeFrac, RESIZE_ANIMATION_THRESHOLD,
};
use crate::animation::{Animation, Clock};
use crate::niri_render_elements;
use crate::render_helpers::border::BorderRenderElement;
use crate::render_helpers::clipped_surface::{ClippedSurfaceRenderElement, RoundedCornerDamage};
use crate::render_helpers::damage::ExtraDamage;
use crate::render_helpers::offscreen::{OffscreenBuffer, OffscreenRenderElement};
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::resize::ResizeRenderElement;
use crate::render_helpers::shadow::ShadowRenderElement;
use crate::render_helpers::snapshot::RenderSnapshot;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::RenderTarget;
use crate::utils::transaction::Transaction;
use crate::utils::{round_logical_in_physical, round_logical_in_physical_max1};

/// Toplevel window with decorations.
#[derive(Debug)]
pub struct Tile<W: LayoutElement> {
    /// The toplevel window itself.
    window: W,

    /// The border around the window.
    border: FocusRing,

    /// The focus ring around the window.
    focus_ring: FocusRing,

    /// The shadow around the window.
    shadow: Shadow,

    /// Whether this tile is fullscreen.
    ///
    /// This will update only when the `window` actually goes fullscreen, rather than right away,
    /// to avoid black backdrop flicker before the window has had a chance to resize.
    is_fullscreen: bool,

    /// The black backdrop for fullscreen windows.
    fullscreen_backdrop: SolidColorBuffer,

    /// Whether the tile should float upon unfullscreening.
    pub(super) unfullscreen_to_floating: bool,

    /// The size that the window should assume when going floating.
    ///
    /// This is generally the last size the window had when it was floating. It can be unknown if
    /// the window starts out in the tiling layout or fullscreen.
    pub(super) floating_window_size: Option<Size<i32, Logical>>,

    /// The position that the tile should assume when going floating, relative to the floating
    /// space working area.
    ///
    /// This is generally the last position the tile had when it was floating. It can be unknown if
    /// the window starts out in the tiling layout.
    pub(super) floating_pos: Option<Point<f64, SizeFrac>>,

    /// Currently selected preset width index when this tile is floating.
    pub(super) floating_preset_width_idx: Option<usize>,

    /// Currently selected preset height index when this tile is floating.
    pub(super) floating_preset_height_idx: Option<usize>,

    /// The animation upon opening a window.
    open_animation: Option<OpenAnimation>,

    /// The animation of the window resizing.
    resize_animation: Option<ResizeAnimation>,

    /// The animation of a tile visually moving horizontally.
    move_x_animation: Option<MoveAnimation>,

    /// The animation of a tile visually moving vertically.
    move_y_animation: Option<MoveAnimation>,

    /// The animation of the tile's opacity.
    pub(super) alpha_animation: Option<AlphaAnimation>,

    /// Offset during the initial interactive move rubberband.
    pub(super) interactive_move_offset: Point<f64, Logical>,

    /// Snapshot of the last render for use in the close animation.
    unmap_snapshot: Option<TileRenderSnapshot>,

    /// Extra damage for clipped surface corner radius changes.
    rounded_corner_damage: RoundedCornerDamage,

    /// The view size for the tile's workspace.
    ///
    /// Used as the fullscreen target size.
    view_size: Size<f64, Logical>,

    /// Scale of the output the tile is on (and rounds its sizes to).
    scale: f64,

    /// Extra scale used for rendering.
    ///
    /// Applied on top of `scale` and used for visuals only (does not affect the layout).
    extra_overview_scale: f64,

    /// Clock for driving animations.
    pub(super) clock: Clock,

    /// Configurable properties of the layout.
    pub(super) options: Rc<Options>,
}

niri_render_elements! {
    TileRenderElement<R> => {
        LayoutElement = LayoutElementRenderElement<R>,
        FocusRing = FocusRingRenderElement,
        SolidColor = SolidColorRenderElement,
        Opening = OpeningWindowRenderElement,
        Resize = ResizeRenderElement,
        Border = BorderRenderElement,
        Shadow = ShadowRenderElement,
        ClippedSurface = ClippedSurfaceRenderElement<R>,
        Offscreen = OffscreenRenderElement,
        ExtraDamage = ExtraDamage,
    }
}

pub type TileRenderSnapshot =
    RenderSnapshot<TileRenderElement<GlesRenderer>, TileRenderElement<GlesRenderer>>;

#[derive(Debug)]
struct ResizeAnimation {
    anim: Animation,
    size_from: Size<f64, Logical>,
    snapshot: LayoutElementRenderSnapshot,
    offscreen: OffscreenBuffer,
}

#[derive(Debug)]
struct MoveAnimation {
    anim: Animation,
    from: f64,
}

#[derive(Debug)]
pub(super) struct AlphaAnimation {
    pub(super) anim: Animation,
    /// Whether the animation should persist after it's done.
    ///
    /// This is used by things like interactive move which need to animate alpha to
    /// semitransparent, then hold it at semitransparent for a while, until the operation
    /// completes.
    pub(super) hold_after_done: bool,
    offscreen: OffscreenBuffer,
}

impl<W: LayoutElement> Tile<W> {
    pub fn new(
        window: W,
        view_size: Size<f64, Logical>,
        scale: f64,
        clock: Clock,
        options: Rc<Options>,
    ) -> Self {
        let rules = window.rules();
        let border_config = rules.border.resolve_against(options.border);
        let focus_ring_config = rules.focus_ring.resolve_against(options.focus_ring.into());
        let shadow_config = rules.shadow.resolve_against(options.shadow);
        let is_fullscreen = window.is_fullscreen();

        Self {
            window,
            border: FocusRing::new(border_config.into()),
            focus_ring: FocusRing::new(focus_ring_config.into()),
            shadow: Shadow::new(shadow_config),
            is_fullscreen,
            fullscreen_backdrop: SolidColorBuffer::new(view_size, [0., 0., 0., 1.]),
            unfullscreen_to_floating: false,
            floating_window_size: None,
            floating_pos: None,
            floating_preset_width_idx: None,
            floating_preset_height_idx: None,
            open_animation: None,
            resize_animation: None,
            move_x_animation: None,
            move_y_animation: None,
            alpha_animation: None,
            interactive_move_offset: Point::from((0., 0.)),
            unmap_snapshot: None,
            rounded_corner_damage: Default::default(),
            view_size,
            scale,
            extra_overview_scale: 1.,
            clock,
            options,
        }
    }

    pub fn update_config(
        &mut self,
        view_size: Size<f64, Logical>,
        scale: f64,
        options: Rc<Options>,
    ) {
        // If preset widths or heights changed, clear our stored preset index.
        if self.options.preset_column_widths != options.preset_column_widths {
            self.floating_preset_width_idx = None;
        }
        if self.options.preset_window_heights != options.preset_window_heights {
            self.floating_preset_height_idx = None;
        }

        self.view_size = view_size;
        self.scale = scale;
        self.options = options;

        let rules = self.window.rules();

        let border_config = rules.border.resolve_against(self.options.border);
        self.border.update_config(border_config.into());

        let focus_ring_config = rules
            .focus_ring
            .resolve_against(self.options.focus_ring.into());
        self.focus_ring.update_config(focus_ring_config.into());

        let shadow_config = rules.shadow.resolve_against(self.options.shadow);
        self.shadow.update_config(shadow_config);

        self.fullscreen_backdrop.resize(view_size);
    }

    pub fn update_shaders(&mut self) {
        self.border.update_shaders();
        self.focus_ring.update_shaders();
        self.shadow.update_shaders();
    }

    pub fn update_window(&mut self) {
        self.is_fullscreen = self.window.is_fullscreen();

        if let Some(animate_from) = self.window.take_animation_snapshot() {
            let (size_from, offscreen) = if let Some(resize) = self.resize_animation.take() {
                // Compute like in animated_window_size(), but using the snapshot geometry (since
                // the current one is already overwritten).
                let mut size = animate_from.size;

                let val = resize.anim.value();
                let size_from = resize.size_from;

                size.w = size_from.w + (size.w - size_from.w) * val;
                size.h = size_from.h + (size.h - size_from.h) * val;

                // Also try to reuse the existing offscreen buffer if we have one.
                (size, resize.offscreen)
            } else {
                (animate_from.size, OffscreenBuffer::default())
            };

            let change = self.window.size().to_f64().to_point() - size_from.to_point();
            let change = f64::max(change.x.abs(), change.y.abs());
            if change > RESIZE_ANIMATION_THRESHOLD {
                let anim = Animation::new(
                    self.clock.clone(),
                    0.,
                    1.,
                    0.,
                    self.options.animations.window_resize.anim,
                );
                self.resize_animation = Some(ResizeAnimation {
                    anim,
                    size_from,
                    snapshot: animate_from,
                    offscreen,
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

        let shadow_config = rules.shadow.resolve_against(self.options.shadow);
        self.shadow.update_config(shadow_config);

        let window_size = self.window_size();
        let radius = rules
            .geometry_corner_radius
            .unwrap_or_default()
            .fit_to(window_size.w as f32, window_size.h as f32);
        self.rounded_corner_damage.set_corner_radius(radius);
        self.rounded_corner_damage.set_size(window_size);
    }

    pub fn advance_animations(&mut self) {
        if let Some(open) = &mut self.open_animation {
            if open.is_done() {
                self.open_animation = None;
            }
        }

        if let Some(resize) = &mut self.resize_animation {
            if resize.anim.is_done() {
                self.resize_animation = None;
            }
        }

        if let Some(move_) = &mut self.move_x_animation {
            if move_.anim.is_done() {
                self.move_x_animation = None;
            }
        }
        if let Some(move_) = &mut self.move_y_animation {
            if move_.anim.is_done() {
                self.move_y_animation = None;
            }
        }

        if let Some(alpha) = &mut self.alpha_animation {
            if !alpha.hold_after_done && alpha.anim.is_done() {
                self.alpha_animation = None;
            }
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.are_transitions_ongoing() || self.window.rules().baba_is_float == Some(true)
    }

    pub fn are_transitions_ongoing(&self) -> bool {
        self.open_animation.is_some()
            || self.resize_animation.is_some()
            || self.move_x_animation.is_some()
            || self.move_y_animation.is_some()
            || self
                .alpha_animation
                .as_ref()
                .is_some_and(|alpha| !alpha.anim.is_done())
    }

    pub fn update_render_elements(
        &mut self,
        is_active: bool,
        view_rect: Rectangle<f64, Logical>,
        extra_overview_scale: f64,
    ) {
        let rules = self.window.rules();

        self.extra_overview_scale = extra_overview_scale;
        let visual_scale = self.scale * extra_overview_scale;

        let draw_border_with_background = rules
            .draw_border_with_background
            .unwrap_or_else(|| !self.window.has_ssd());
        let border_width = self.effective_border_width().unwrap_or(0.);
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
            Rectangle::new(
                view_rect.loc - Point::from((border_width, border_width)),
                view_rect.size,
            ),
            radius,
            visual_scale,
            1.,
        );

        let radius = if self.is_fullscreen {
            CornerRadius::default()
        } else if self.effective_border_width().is_some() {
            radius
        } else {
            rules.geometry_corner_radius.unwrap_or_default()
        };
        self.shadow.update_render_elements(
            self.animated_tile_size(),
            is_active,
            radius,
            visual_scale,
            1.,
        );

        let draw_focus_ring_with_background = if self.effective_border_width().is_some() {
            false
        } else {
            draw_border_with_background
        };
        let radius = radius.expanded_by(self.focus_ring.width() as f32);
        self.focus_ring.update_render_elements(
            self.animated_tile_size(),
            is_active,
            !draw_focus_ring_with_background,
            view_rect,
            radius,
            visual_scale,
            1.,
        );
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn render_offset(&self) -> Point<f64, Logical> {
        let mut offset = Point::from((0., 0.));

        if let Some(move_) = &self.move_x_animation {
            offset.x += move_.from * move_.anim.value();
        }
        if let Some(move_) = &self.move_y_animation {
            offset.y += move_.from * move_.anim.value();
        }

        offset += self.interactive_move_offset;

        offset
    }

    pub fn start_open_animation(&mut self) {
        self.open_animation = Some(OpenAnimation::new(Animation::new(
            self.clock.clone(),
            0.,
            1.,
            0.,
            self.options.animations.window_open.anim,
        )));
    }

    pub fn resize_animation(&self) -> Option<&Animation> {
        self.resize_animation.as_ref().map(|resize| &resize.anim)
    }

    pub fn animate_move_from(&mut self, from: Point<f64, Logical>) {
        self.animate_move_x_from(from.x);
        self.animate_move_y_from(from.y);
    }

    pub fn animate_move_x_from(&mut self, from: f64) {
        self.animate_move_x_from_with_config(from, self.options.animations.window_movement.0);
    }

    pub fn animate_move_x_from_with_config(&mut self, from: f64, config: niri_config::Animation) {
        let current_offset = self.render_offset().x;

        // Preserve the previous config if ongoing.
        let anim = self.move_x_animation.take().map(|move_| move_.anim);
        let anim = anim
            .map(|anim| anim.restarted(1., 0., 0.))
            .unwrap_or_else(|| Animation::new(self.clock.clone(), 1., 0., 0., config));

        self.move_x_animation = Some(MoveAnimation {
            anim,
            from: from + current_offset,
        });
    }

    pub fn animate_move_y_from(&mut self, from: f64) {
        self.animate_move_y_from_with_config(from, self.options.animations.window_movement.0);
    }

    pub fn animate_move_y_from_with_config(&mut self, from: f64, config: niri_config::Animation) {
        let current_offset = self.render_offset().y;

        // Preserve the previous config if ongoing.
        let anim = self.move_y_animation.take().map(|move_| move_.anim);
        let anim = anim
            .map(|anim| anim.restarted(1., 0., 0.))
            .unwrap_or_else(|| Animation::new(self.clock.clone(), 1., 0., 0., config));

        self.move_y_animation = Some(MoveAnimation {
            anim,
            from: from + current_offset,
        });
    }

    pub fn stop_move_animations(&mut self) {
        self.move_x_animation = None;
        self.move_y_animation = None;
    }

    pub fn animate_alpha(&mut self, from: f64, to: f64, config: niri_config::Animation) {
        let from = from.clamp(0., 1.);
        let to = to.clamp(0., 1.);

        let (current, offscreen) = if let Some(alpha) = self.alpha_animation.take() {
            (alpha.anim.clamped_value(), alpha.offscreen)
        } else {
            (from, OffscreenBuffer::default())
        };

        self.alpha_animation = Some(AlphaAnimation {
            anim: Animation::new(self.clock.clone(), current, to, 0., config),
            hold_after_done: false,
            offscreen,
        });
    }

    pub fn ensure_alpha_animates_to_1(&mut self) {
        if let Some(alpha) = &self.alpha_animation {
            if alpha.anim.to() != 1. {
                // Cancel animation instead of starting a new one because the user likely wants to
                // see the tile right away.
                self.alpha_animation = None;
            }
        }
    }

    pub fn hold_alpha_animation_after_done(&mut self) {
        if let Some(alpha) = &mut self.alpha_animation {
            alpha.hold_after_done = true;
        }
    }

    pub fn window(&self) -> &W {
        &self.window
    }

    pub fn window_mut(&mut self) -> &mut W {
        &mut self.window
    }

    pub fn is_fullscreen(&self) -> bool {
        self.is_fullscreen
    }

    /// Returns `None` if the border is hidden and `Some(width)` if it should be shown.
    pub fn effective_border_width(&self) -> Option<f64> {
        if self.is_fullscreen {
            return None;
        }

        if self.border.is_off() {
            return None;
        }

        Some(self.border.width())
    }

    pub fn visual_effective_border_width(&self) -> Option<f64> {
        let visual_scale = self.scale * self.extra_overview_scale;
        self.effective_border_width()
            .map(move |w| round_logical_in_physical_max1(visual_scale, w))
    }

    /// Returns the location of the window's visual geometry within this Tile.
    pub fn window_loc(&self) -> Point<f64, Logical> {
        let mut loc = Point::from((0., 0.));

        // In fullscreen, center the window in the given size.
        if self.is_fullscreen {
            let window_size = self.window_size();
            let target_size = self.view_size;

            // Windows aren't supposed to be larger than the fullscreen size, but in case we get
            // one, leave it at the top-left as usual.
            if window_size.w < target_size.w {
                loc.x += (target_size.w - window_size.w) / 2.;
            }
            if window_size.h < target_size.h {
                loc.y += (target_size.h - window_size.h) / 2.;
            }

            // Round to physical pixels.
            loc = loc
                .to_physical_precise_round(self.scale)
                .to_logical(self.scale);
        }

        if let Some(width) = self.effective_border_width() {
            loc += (width, width).into();
        }

        loc
    }

    pub fn visual_window_loc(&self) -> Point<f64, Logical> {
        let mut loc = Point::from((0., 0.));

        let visual_scale = self.scale * self.extra_overview_scale;

        // In fullscreen, center the window in the given size.
        if self.is_fullscreen {
            let window_size = self.window_size();
            let target_size = self.view_size;

            // Windows aren't supposed to be larger than the fullscreen size, but in case we get
            // one, leave it at the top-left as usual.
            if window_size.w < target_size.w {
                loc.x += (target_size.w - window_size.w) / 2.;
            }
            if window_size.h < target_size.h {
                loc.y += (target_size.h - window_size.h) / 2.;
            }

            // Round to physical pixels.
            loc = loc
                .to_physical_precise_round(visual_scale)
                .to_logical(visual_scale);
        }

        if let Some(width) = self.visual_effective_border_width() {
            loc += (width, width).into();
        }

        loc
    }

    pub fn tile_size(&self) -> Size<f64, Logical> {
        let mut size = self.window_size();

        if self.is_fullscreen {
            // Normally we'd just return the fullscreen size here, but this makes things a bit
            // nicer if a fullscreen window is bigger than the fullscreen size for some reason.
            size.w = f64::max(size.w, self.view_size.w);
            size.h = f64::max(size.h, self.view_size.h);
            return size;
        }

        if let Some(width) = self.effective_border_width() {
            size.w += width * 2.;
            size.h += width * 2.;
        }

        size
    }

    pub fn tile_expected_or_current_size(&self) -> Size<f64, Logical> {
        let mut size = self.window_expected_or_current_size();

        if self.is_fullscreen {
            // Normally we'd just return the fullscreen size here, but this makes things a bit
            // nicer if a fullscreen window is bigger than the fullscreen size for some reason.
            size.w = f64::max(size.w, self.view_size.w);
            size.h = f64::max(size.h, self.view_size.h);
            return size;
        }

        if let Some(width) = self.effective_border_width() {
            size.w += width * 2.;
            size.h += width * 2.;
        }

        size
    }

    pub fn window_size(&self) -> Size<f64, Logical> {
        let mut size = self.window.size().to_f64();
        size = size
            .to_physical_precise_round(self.scale)
            .to_logical(self.scale);
        size
    }

    pub fn window_expected_or_current_size(&self) -> Size<f64, Logical> {
        let size = self.window.expected_size();
        let mut size = size.unwrap_or_else(|| self.window.size()).to_f64();
        size = size
            .to_physical_precise_round(self.scale)
            .to_logical(self.scale);
        size
    }

    pub fn animated_window_size(&self) -> Size<f64, Logical> {
        let mut size = self.window_size();

        let visual_scale = self.scale * self.extra_overview_scale;

        if let Some(resize) = &self.resize_animation {
            let val = resize.anim.value();
            let size_from = resize.size_from.to_f64();

            size.w = f64::max(1., size_from.w + (size.w - size_from.w) * val);
            size.h = f64::max(1., size_from.h + (size.h - size_from.h) * val);
            size = size
                .to_physical_precise_round(visual_scale)
                .to_logical(visual_scale);
        }

        size
    }

    pub fn animated_tile_size(&self) -> Size<f64, Logical> {
        let mut size = self.animated_window_size();

        let visual_scale = self.scale * self.extra_overview_scale;

        if self.is_fullscreen {
            // Normally we'd just return the fullscreen size here, but this makes things a bit
            // nicer if a fullscreen window is bigger than the fullscreen size for some reason.
            size.w = f64::max(size.w, self.view_size.w);
            size.h = f64::max(size.h, self.view_size.h);
            size = size
                .to_physical_precise_round(visual_scale)
                .to_logical(visual_scale);
            return size;
        }

        if let Some(width) = self.visual_effective_border_width() {
            size.w += width * 2.;
            size.h += width * 2.;
        }

        size
    }

    pub fn buf_loc(&self) -> Point<f64, Logical> {
        let mut loc = Point::from((0., 0.));
        loc += self.window_loc();
        loc += self.window.buf_loc().to_f64();
        loc
    }

    fn is_in_input_region(&self, mut point: Point<f64, Logical>) -> bool {
        point -= self.window_loc().to_f64();
        self.window.is_in_input_region(point)
    }

    fn is_in_activation_region(&self, point: Point<f64, Logical>) -> bool {
        let activation_region = Rectangle::from_size(self.tile_size());
        activation_region.contains(point)
    }

    pub fn hit(&self, point: Point<f64, Logical>) -> Option<HitType> {
        let offset = self.bob_offset();
        let point = point - offset;

        if self.is_in_input_region(point) {
            let win_pos = self.buf_loc() + offset;
            Some(HitType::Input { win_pos })
        } else if self.is_in_activation_region(point) {
            Some(HitType::Activate {
                is_tab_indicator: false,
            })
        } else {
            None
        }
    }

    pub fn request_tile_size(
        &mut self,
        mut size: Size<f64, Logical>,
        animate: bool,
        transaction: Option<Transaction>,
    ) {
        // Can't go through effective_border_width() because we might be fullscreen.
        if !self.border.is_off() {
            let width = self.border.width();
            size.w = f64::max(1., size.w - width * 2.);
            size.h = f64::max(1., size.h - width * 2.);
        }

        // The size request has to be i32 unfortunately, due to Wayland. We floor here instead of
        // round to avoid situations where proportionally-sized columns don't fit on the screen
        // exactly.
        self.window
            .request_size(size.to_i32_floor(), false, animate, transaction);
    }

    pub fn tile_width_for_window_width(&self, size: f64) -> f64 {
        if self.border.is_off() {
            size
        } else {
            size + self.border.width() * 2.
        }
    }

    pub fn tile_height_for_window_height(&self, size: f64) -> f64 {
        if self.border.is_off() {
            size
        } else {
            size + self.border.width() * 2.
        }
    }

    pub fn window_width_for_tile_width(&self, size: f64) -> f64 {
        if self.border.is_off() {
            size
        } else {
            size - self.border.width() * 2.
        }
    }

    pub fn window_height_for_tile_height(&self, size: f64) -> f64 {
        if self.border.is_off() {
            size
        } else {
            size - self.border.width() * 2.
        }
    }

    pub fn request_fullscreen(&mut self, animate: bool, transaction: Option<Transaction>) {
        self.window
            .request_size(self.view_size.to_i32_round(), true, animate, transaction);
    }

    pub fn min_size_nonfullscreen(&self) -> Size<f64, Logical> {
        let mut size = self.window.min_size().to_f64();

        // Can't go through effective_border_width() because we might be fullscreen.
        if !self.border.is_off() {
            let width = self.border.width();

            size.w = f64::max(1., size.w);
            size.h = f64::max(1., size.h);

            size.w += width * 2.;
            size.h += width * 2.;
        }

        size
    }

    pub fn max_size_nonfullscreen(&self) -> Size<f64, Logical> {
        let mut size = self.window.max_size().to_f64();

        // Can't go through effective_border_width() because we might be fullscreen.
        if !self.border.is_off() {
            let width = self.border.width();

            if size.w > 0. {
                size.w += width * 2.;
            }
            if size.h > 0. {
                size.h += width * 2.;
            }
        }

        size
    }

    pub fn bob_offset(&self) -> Point<f64, Logical> {
        if self.window.rules().baba_is_float != Some(true) {
            return Point::from((0., 0.));
        }

        let visual_scale = self.scale * self.extra_overview_scale;

        let now = self.clock.now().as_secs_f64();
        let amplitude = self.view_size.h / 96.;
        let y = amplitude * ((f64::consts::TAU * now / 3.6).sin() - 1.);
        let y = round_logical_in_physical(visual_scale, y);
        Point::from((0., y))
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

    fn render_inner<'a, R: NiriRenderer + 'a>(
        &'a self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        focus_ring: bool,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> + 'a {
        let _span = tracy_client::span!("Tile::render_inner");

        let scale = Scale::from(self.scale);
        let visual_scale = scale * self.extra_overview_scale;

        let win_alpha = if self.is_fullscreen || self.window.is_ignoring_opacity_window_rule() {
            1.
        } else {
            self.window.rules().opacity.unwrap_or(1.).clamp(0., 1.)
        };

        // This is here rather than in render_offset() because render_offset() is currently assumed
        // by the code to be temporary. So, for example, interactive move will try to "grab" the
        // tile at its current render offset and reset the render offset to zero by cancelling the
        // tile move animations. On the other hand, bob_offset() is not resettable, so adding it in
        // render_offset() would cause obvious animation glitches.
        //
        // This isn't to say that adding it here is perfect; indeed, it kind of breaks view_rect
        // passed to update_render_elements(). But, it works well enough for what it is.
        let location = location + self.bob_offset();

        let window_loc = self.visual_window_loc();
        let window_size = self.window_size().to_f64();
        let animated_window_size = self.animated_window_size();
        let window_render_loc = location + window_loc;
        let area = Rectangle::new(window_render_loc, animated_window_size);

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
                    .render_popups(renderer, window_render_loc, scale, win_alpha, target)
                    .into_iter()
                    .map(Into::into),
            );

            if ResizeRenderElement::has_shader(renderer) {
                let gles_renderer = renderer.as_gles_renderer();

                if let Some(texture_from) = resize.snapshot.texture(gles_renderer, scale, target) {
                    let window_elements = self.window.render_normal(
                        gles_renderer,
                        Point::from((0., 0.)),
                        scale,
                        1.,
                        target,
                    );

                    let current = resize
                        .offscreen
                        .render(gles_renderer, scale, &window_elements)
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

                    if let Some((elem_current, _sync_point, mut data)) = current {
                        let texture_current = elem_current.texture().clone();
                        // The offset and size are computed in physical pixels and converted to
                        // logical with the same `scale`, so converting them back with rounding
                        // inside the geometry() call gives us the same physical result back.
                        let texture_current_geo = elem_current.geometry(scale);

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
                            win_alpha,
                        );

                        // We're drawing the resize shader, not the offscreen directly.
                        data.id = elem.id().clone();

                        // This is not a problem for split popups as the code will look for them by
                        // original id when it doesn't find them on the offscreen.
                        self.window.set_offscreen_data(Some(data));
                        resize_shader = Some(elem.into());
                    }
                }
            }

            if resize_shader.is_none() {
                let fallback_buffer = SolidColorBuffer::new(area.size, [1., 0., 0., 1.]);
                resize_fallback = Some(
                    SolidColorRenderElement::from_buffer(
                        &fallback_buffer,
                        area.loc,
                        win_alpha,
                        Kind::Unspecified,
                    )
                    .into(),
                );
            }
        }

        // If we're not resizing, render the window itself.
        let mut window_surface = None;
        let mut window_popups = None;
        let mut rounded_corner_damage = None;
        if resize_shader.is_none() && resize_fallback.is_none() {
            let window = self
                .window
                .render(renderer, window_render_loc, scale, win_alpha, target);

            let geo = Rectangle::new(window_render_loc, window_size);
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
                            Rectangle::from_size(geo.size),
                            GradientInterpolation::default(),
                            Color::from_color32f(elem.color()),
                            Color::from_color32f(elem.color()),
                            0.,
                            Rectangle::from_size(geo.size),
                            0.,
                            radius,
                            scale.x as f32,
                            1.,
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
                location,
                1.,
                Kind::Unspecified,
            )
            .into()
        });
        let rv = rv.chain(elem);

        let elem = self.visual_effective_border_width().map(|width| {
            self.border
                .render(renderer, location + Point::from((width, width)))
                .map(Into::into)
        });
        let rv = rv.chain(elem.into_iter().flatten());

        let elem = focus_ring.then(|| self.focus_ring.render(renderer, location).map(Into::into));
        let rv = rv.chain(elem.into_iter().flatten());

        rv.chain(self.shadow.render(renderer, location).map(Into::into))
    }

    pub fn render<'a, R: NiriRenderer + 'a>(
        &'a self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        focus_ring: bool,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> + 'a {
        let _span = tracy_client::span!("Tile::render");

        let scale = Scale::from(self.scale);

        let tile_alpha = self
            .alpha_animation
            .as_ref()
            .map_or(1., |alpha| alpha.anim.clamped_value()) as f32;

        let mut open_anim_elem = None;
        let mut alpha_anim_elem = None;
        let mut window_elems = None;

        self.window().set_offscreen_data(None);

        if let Some(open) = &self.open_animation {
            let renderer = renderer.as_gles_renderer();
            let elements = self.render_inner(renderer, Point::from((0., 0.)), focus_ring, target);
            let elements = elements.collect::<Vec<TileRenderElement<_>>>();
            match open.render(
                renderer,
                &elements,
                self.animated_tile_size(),
                location,
                scale,
                tile_alpha,
            ) {
                Ok((elem, data)) => {
                    self.window().set_offscreen_data(Some(data));
                    open_anim_elem = Some(elem.into());
                }
                Err(err) => {
                    warn!("error rendering window opening animation: {err:?}");
                }
            }
        } else if let Some(alpha) = &self.alpha_animation {
            let renderer = renderer.as_gles_renderer();
            let elements = self.render_inner(renderer, Point::from((0., 0.)), focus_ring, target);
            let elements = elements.collect::<Vec<TileRenderElement<_>>>();
            match alpha.offscreen.render(renderer, scale, &elements) {
                Ok((elem, _sync, data)) => {
                    let offset = elem.offset();
                    let elem = elem.with_alpha(tile_alpha).with_offset(location + offset);

                    self.window().set_offscreen_data(Some(data));
                    alpha_anim_elem = Some(elem.into());
                }
                Err(err) => {
                    warn!("error rendering tile to offscreen for alpha animation: {err:?}");
                }
            }
        }

        if open_anim_elem.is_none() && alpha_anim_elem.is_none() {
            window_elems = Some(self.render_inner(renderer, location, focus_ring, target));
        }

        open_anim_elem
            .into_iter()
            .chain(alpha_anim_elem)
            .chain(window_elems.into_iter().flatten())
    }

    pub fn store_unmap_snapshot_if_empty(&mut self, renderer: &mut GlesRenderer) {
        if self.unmap_snapshot.is_some() {
            return;
        }

        self.unmap_snapshot = Some(self.render_snapshot(renderer));
    }

    fn render_snapshot(&self, renderer: &mut GlesRenderer) -> TileRenderSnapshot {
        let _span = tracy_client::span!("Tile::render_snapshot");

        let contents = self.render(renderer, Point::from((0., 0.)), false, RenderTarget::Output);

        // A bit of a hack to render blocked out as for screencast, but I think it's fine here.
        let blocked_out_contents = self.render(
            renderer,
            Point::from((0., 0.)),
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

    pub fn take_unmap_snapshot(&mut self) -> Option<TileRenderSnapshot> {
        self.unmap_snapshot.take()
    }

    pub fn border(&self) -> &FocusRing {
        &self.border
    }

    pub fn focus_ring(&self) -> &FocusRing {
        &self.focus_ring
    }

    pub fn options(&self) -> &Rc<Options> {
        &self.options
    }

    #[cfg(test)]
    pub fn view_size(&self) -> Size<f64, Logical> {
        self.view_size
    }

    #[cfg(test)]
    pub fn verify_invariants(&self) {
        use approx::assert_abs_diff_eq;

        assert_eq!(self.is_fullscreen, self.window.is_fullscreen());
        assert_eq!(self.fullscreen_backdrop.size(), self.view_size);

        let scale = self.scale;
        let size = self.tile_size();
        let rounded = size.to_physical_precise_round(scale).to_logical(scale);
        assert_abs_diff_eq!(size.w, rounded.w, epsilon = 1e-5);
        assert_abs_diff_eq!(size.h, rounded.h, epsilon = 1e-5);
    }
}
