use std::cell::RefCell;

use niri_config::layer_rule::LayerRule;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::Kind;
use smithay::desktop::{LayerSurface, PopupManager};
use smithay::utils::{Logical, Rectangle, Scale};

use super::ResolvedLayerRules;
use crate::niri_render_elements;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::{RenderTarget, SplitElements};

#[derive(Debug)]
pub struct MappedLayer {
    /// The surface itself.
    surface: LayerSurface,

    /// Up-to-date rules.
    rules: ResolvedLayerRules,

    /// Buffer to draw instead of the surface when it should be blocked out.
    block_out_buffer: RefCell<SolidColorBuffer>,
}

niri_render_elements! {
    LayerSurfaceRenderElement<R> => {
        Wayland = WaylandSurfaceRenderElement<R>,
        SolidColor = SolidColorRenderElement,
    }
}

impl MappedLayer {
    pub fn new(surface: LayerSurface, rules: ResolvedLayerRules) -> Self {
        Self {
            surface,
            rules,
            block_out_buffer: RefCell::new(SolidColorBuffer::new((0., 0.), [0., 0., 0., 1.])),
        }
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

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        geometry: Rectangle<i32, Logical>,
        scale: Scale<f64>,
        target: RenderTarget,
    ) -> SplitElements<LayerSurfaceRenderElement<R>> {
        let mut rv = SplitElements::default();

        let alpha = self.rules.opacity.unwrap_or(1.).clamp(0., 1.);

        if target.should_block_out(self.rules.block_out_from) {
            // Round to physical pixels.
            let geometry = geometry
                .to_f64()
                .to_physical_precise_round(scale)
                .to_logical(scale);

            let mut buffer = self.block_out_buffer.borrow_mut();
            buffer.resize(geometry.size.to_f64());
            let elem = SolidColorRenderElement::from_buffer(
                &buffer,
                geometry.loc,
                alpha,
                Kind::Unspecified,
            );
            rv.normal.push(elem.into());
        } else {
            // Layer surfaces don't have extra geometry like windows.
            let buf_pos = geometry.loc;

            let surface = self.surface.wl_surface();
            for (popup, popup_offset) in PopupManager::popups_for_surface(surface) {
                // Layer surfaces don't have extra geometry like windows.
                let offset = popup_offset - popup.geometry().loc;

                rv.popups.extend(render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    (buf_pos + offset).to_physical_precise_round(scale),
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

        rv
    }
}
