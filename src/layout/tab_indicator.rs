use std::iter::zip;

use niri_config::{CornerRadius, Gradient, GradientRelativeTo};
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::tile::Tile;
use super::LayoutElement;
use crate::niri_render_elements;
use crate::render_helpers::border::BorderRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::utils::{floor_logical_in_physical_max1, round_logical_in_physical};

#[derive(Debug)]
pub struct TabIndicator {
    shader_locs: Vec<Point<f64, Logical>>,
    shaders: Vec<BorderRenderElement>,
    config: niri_config::TabIndicator,
}

#[derive(Debug)]
pub struct TabInfo {
    pub gradient: Gradient,
}

niri_render_elements! {
    TabIndicatorRenderElement => {
        Gradient = BorderRenderElement,
    }
}

impl TabIndicator {
    pub fn new(config: niri_config::TabIndicator) -> Self {
        Self {
            shader_locs: Vec::new(),
            shaders: Vec::new(),
            config,
        }
    }

    pub fn update_config(&mut self, config: niri_config::TabIndicator) {
        self.config = config;
    }

    pub fn update_shaders(&mut self) {
        for elem in &mut self.shaders {
            elem.damage_all();
        }
    }

    pub fn update_render_elements(
        &mut self,
        enabled: bool,
        tile_size: Size<f64, Logical>,
        tile_view_rect: Rectangle<f64, Logical>,
        tabs: impl Iterator<Item = TabInfo> + Clone,
        // TODO: do we indicate inactive-but-selected somehow?
        _is_active: bool,
        scale: f64,
    ) {
        if !enabled || self.config.off {
            self.shader_locs.clear();
            self.shaders.clear();
            return;
        }

        // Tab indicators are rendered relative to the tile geometry.
        let tile_geo = Rectangle::new(Point::from((0., 0.)), tile_size);

        let round = |logical: f64| round_logical_in_physical(scale, logical);

        let width = round(self.config.width.0);
        let gap = round(self.config.gap.0);

        let total_prop = self.config.length.total_proportion.unwrap_or(0.5);
        let min_length = round(tile_size.h * total_prop.clamp(0., 2.));

        let count = tabs.clone().count();
        self.shaders.resize_with(count, Default::default);
        self.shader_locs.resize_with(count, Default::default);

        let pixel = 1. / scale;
        let shortest_length = count as f64 * pixel;
        let length = f64::max(min_length, shortest_length);
        let px_per_tab = length / count as f64;
        let px_per_tab = floor_logical_in_physical_max1(scale, px_per_tab);
        let floored_length = count as f64 * px_per_tab;
        let mut ones_left = ((length - floored_length) / pixel).max(0.).round() as usize;

        let mut shader_loc = Point::from((-gap - width, round((tile_size.h - length) / 2.)));

        for ((shader, loc), tab) in zip(&mut self.shaders, &mut self.shader_locs).zip(tabs) {
            *loc = shader_loc;

            let mut px_per_tab = px_per_tab;
            if ones_left > 0 {
                ones_left -= 1;
                px_per_tab += pixel;
            }
            shader_loc.y += px_per_tab;

            let shader_size = Size::from((width, px_per_tab));

            let mut gradient_area = match tab.gradient.relative_to {
                GradientRelativeTo::Window => tile_geo,
                GradientRelativeTo::WorkspaceView => tile_view_rect,
            };
            gradient_area.loc -= *loc;

            shader.update(
                shader_size,
                gradient_area,
                tab.gradient.in_,
                tab.gradient.from,
                tab.gradient.to,
                ((tab.gradient.angle as f32) - 90.).to_radians(),
                Rectangle::from_size(shader_size),
                0.,
                CornerRadius::default(),
                scale as f32,
                1.,
            );
        }
    }

    pub fn render(
        &self,
        renderer: &mut impl NiriRenderer,
        tile_pos: Point<f64, Logical>,
    ) -> impl Iterator<Item = TabIndicatorRenderElement> + '_ {
        let has_border_shader = BorderRenderElement::has_shader(renderer);
        if !has_border_shader {
            return None.into_iter().flatten();
        }

        let rv = zip(&self.shaders, &self.shader_locs)
            .map(move |(shader, loc)| shader.clone().with_location(tile_pos + *loc))
            .map(TabIndicatorRenderElement::from);

        Some(rv).into_iter().flatten()
    }

    pub fn config(&self) -> niri_config::TabIndicator {
        self.config
    }
}

impl TabInfo {
    pub fn from_tile<W: LayoutElement>(
        tile: &Tile<W>,
        is_active: bool,
        config: &niri_config::TabIndicator,
    ) -> Self {
        let rules = tile.window().rules();
        let rule = rules.tab_indicator;

        let gradient_from_rule = || {
            let (color, gradient) = if is_active {
                (rule.active_color, rule.active_gradient)
            } else {
                (rule.inactive_color, rule.inactive_gradient)
            };
            let color = color.map(Gradient::from);
            gradient.or(color)
        };

        let gradient_from_config = || {
            let (color, gradient) = if is_active {
                (config.active_color, config.active_gradient)
            } else {
                (config.inactive_color, config.inactive_gradient)
            };
            let color = color.map(Gradient::from);
            gradient.or(color)
        };

        let gradient_from_border = || {
            // Come up with tab indicator gradient matching the focus ring or the border, whichever
            // one is enabled.
            let focus_ring_config = tile.focus_ring().config();
            let border_config = tile.border().config();
            let config = if focus_ring_config.off {
                border_config
            } else {
                focus_ring_config
            };

            let (color, gradient) = if is_active {
                (config.active_color, config.active_gradient)
            } else {
                (config.inactive_color, config.inactive_gradient)
            };
            gradient.unwrap_or_else(|| Gradient::from(color))
        };

        let gradient = gradient_from_rule()
            .or_else(gradient_from_config)
            .unwrap_or_else(gradient_from_border);

        TabInfo { gradient }
    }
}
