use niri_config::layer_rule::LayerRule;
use niri_config::Config;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::Kind;
use smithay::desktop::{LayerSurface, PopupManager};
use smithay::utils::{Logical, Point, Scale, Size};
use smithay::wayland::shell::wlr_layer::{ExclusiveZone, Layer};

use super::ResolvedLayerRules;
use crate::layout::shadow::Shadow;
use crate::niri_render_elements;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::shadow::ShadowRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::{RenderTarget, SplitElements};

#[derive(Debug)]
pub struct MappedLayer {
    /// The surface itself.
    surface: LayerSurface,

    /// Up-to-date rules.
    rules: ResolvedLayerRules,

    /// Buffer to draw instead of the surface when it should be blocked out.
    block_out_buffer: SolidColorBuffer,

    /// The shadow around the surface.
    shadow: Shadow,
}

niri_render_elements! {
    LayerSurfaceRenderElement<R> => {
        Wayland = WaylandSurfaceRenderElement<R>,
        SolidColor = SolidColorRenderElement,
        Shadow = ShadowRenderElement,
    }
}

impl MappedLayer {
    pub fn new(surface: LayerSurface, rules: ResolvedLayerRules, config: &Config) -> Self {
        let mut shadow_config = config.layout.shadow;
        // Shadows for layer surfaces need to be explicitly enabled.
        shadow_config.on = false;
        let shadow_config = rules.shadow.resolve_against(shadow_config);

        Self {
            surface,
            rules,
            block_out_buffer: SolidColorBuffer::new((0., 0.), [0., 0., 0., 1.]),
            shadow: Shadow::new(shadow_config),
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        let mut shadow_config = config.layout.shadow;
        // Shadows for layer surfaces need to be explicitly enabled.
        shadow_config.on = false;
        let shadow_config = self.rules.shadow.resolve_against(shadow_config);
        self.shadow.update_config(shadow_config);
    }

    pub fn update_shaders(&mut self) {
        self.shadow.update_shaders();
    }

    pub fn update_render_elements(&mut self, size: Size<f64, Logical>, scale: Scale<f64>) {
        // Round to physical pixels.
        let size = size.to_physical_precise_round(scale).to_logical(scale);

        self.block_out_buffer.resize(size);

        let radius = self.rules.geometry_corner_radius.unwrap_or_default();
        // FIXME: is_active based on keyboard focus?
        self.shadow
            .update_render_elements(size, true, radius, scale.x, 1.);
    }

    pub fn surface(&self) -> &LayerSurface {
        &self.surface
    }

    pub fn rules(&self) -> &ResolvedLayerRules {
        &self.rules
    }

    /// Recomputes the resolved layer rules and returns whether they changed.
    pub fn recompute_layer_rules(&mut self, rules: &[LayerRule], is_at_startup: bool) -> bool {
        let new_rules = ResolvedLayerRules::compute(rules, &self.surface, is_at_startup);
        if new_rules == self.rules {
            return false;
        }

        self.rules = new_rules;
        true
    }

    pub fn place_within_backdrop(&self) -> bool {
        if !self.rules.place_within_backdrop {
            return false;
        }

        if self.surface.layer() != Layer::Background {
            return false;
        }

        let state = self.surface.cached_state();
        if state.exclusive_zone != ExclusiveZone::DontCare {
            return false;
        }

        true
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        target: RenderTarget,
    ) -> SplitElements<LayerSurfaceRenderElement<R>> {
        let mut rv = SplitElements::default();

        let alpha = self.rules.opacity.unwrap_or(1.).clamp(0., 1.);

        if target.should_block_out(self.rules.block_out_from) {
            // Round to physical pixels.
            let location = location.to_physical_precise_round(scale).to_logical(scale);

            // FIXME: take geometry-corner-radius into account.
            let elem = SolidColorRenderElement::from_buffer(
                &self.block_out_buffer,
                location,
                alpha,
                Kind::Unspecified,
            );
            rv.normal.push(elem.into());
        } else {
            // Layer surfaces don't have extra geometry like windows.
            let buf_pos = location;

            let surface = self.surface.wl_surface();
            for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
                // Layer surfaces don't have extra geometry like windows.
                let offset = popup_offset - popup.geometry().loc;

                rv.popups.extend(render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    (buf_pos + offset.to_f64()).to_physical_precise_round(scale),
                    scale,
                    alpha,
                    Kind::Unspecified,
                ));
            }

            rv.normal = render_elements_from_surface_tree(
                renderer,
                surface,
                buf_pos.to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            );
        }

        let location = location.to_physical_precise_round(scale).to_logical(scale);
        rv.normal
            .extend(self.shadow.render(renderer, location).map(Into::into));

        rv
    }
}
