
use crate::layout::ColumnDisplay;
use smithay::utils::{Point, Rectangle, Scale};

use super::ScrollingSpace;
use super::ScrollingSpaceRenderElement;
use crate::layout::LayoutElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::RenderTarget;
use super::utils::compute_toplevel_bounds;
use crate::layout::ConfigureIntent;
use niri_config::utils::MergeWith as _;

impl<W: LayoutElement> ScrollingSpace<W> {
    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer if we're on a fullscreen window and the view is stationary.
        if self.columns.is_empty() {
            return false;
        }

        if !self.view_offset.is_static() {
            return false;
        }

        self.columns[self.active_column_idx]
            .sizing_mode()
            .is_fullscreen()
    }

    pub fn render_elements<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        target: RenderTarget,
        focus_ring: bool,
    ) -> Vec<ScrollingSpaceRenderElement<R>> {
        let mut rv = vec![];

        let scale = Scale::from(self.scale);

        // Draw the closing windows on top of the other windows.
        let view_rect = Rectangle::new(Point::from((self.view_pos(), 0.)), self.view_size);
        for closing in self.closing_windows.iter().rev() {
            let elem = closing.render(renderer.as_gles_renderer(), view_rect, scale, target);
            rv.push(elem.into());
        }

        if self.columns.is_empty() {
            return rv;
        }

        let mut first = true;

        // This matches self.tiles_in_render_order().
        let view_off = Point::from((-self.view_pos(), 0.));
        for (col, col_x) in self.columns_in_render_order() {
            let col_off = Point::from((col_x, 0.));
            let col_render_off = col.render_offset();

            // Draw the tab indicator on top.
            {
                let pos = view_off + col_off + col_render_off;
                let pos = pos.to_physical_precise_round(scale).to_logical(scale);
                rv.extend(col.tab_indicator.render(renderer, pos).map(Into::into));
            }

            for (tile, tile_off, visible) in col.tiles_in_render_order() {
                let tile_pos =
                    view_off + col_off + col_render_off + tile_off + tile.render_offset();
                // Round to physical pixels.
                let tile_pos = tile_pos.to_physical_precise_round(scale).to_logical(scale);

                // And now the drawing logic.

                // For the active tile (which comes first), draw the focus ring.
                let focus_ring = focus_ring && first;
                first = false;

                // In the scrolling layout, we currently use visible only for hidden tabs in the
                // tabbed mode. We want to animate their opacity when going in and out of tabbed
                // mode, so we don't want to apply "visible" immediately. However, "visible" is
                // also used for input handling, and there we *do* want to apply it immediately.
                // So, let's just selectively ignore "visible" here when animating alpha.
                let visible = visible || tile.alpha_animation.is_some();
                if !visible {
                    continue;
                }

                rv.extend(
                    tile.render(renderer, tile_pos, focus_ring, target)
                        .map(Into::into),
                );
            }
        }

        rv
    }

    pub fn refresh(&mut self, is_active: bool, is_focused: bool) {
        for (col_idx, col) in self.columns.iter_mut().enumerate() {
            let mut col_resize_data = None;
            if let Some(resize) = &self.interactive_resize {
                if col.contains(&resize.window) {
                    col_resize_data = Some(resize.data);
                }
            }

            let is_tabbed = col.display_mode == ColumnDisplay::Tabbed;
            let extra_size = col.extra_size();

            // If transactions are disabled, also disable combined throttling, for more intuitive
            // behavior. In tabbed display mode, only one window is visible, so individual
            // throttling makes more sense.
            let individual_throttling = self.options.disable_transactions || is_tabbed;

            let intent = if self.options.disable_resize_throttling {
                ConfigureIntent::CanSend
            } else if individual_throttling {
                // In this case, we don't use combined throttling, but rather compute throttling
                // individually below.
                ConfigureIntent::CanSend
            } else {
                col.tiles
                    .iter()
                    .fold(ConfigureIntent::NotNeeded, |intent, tile| {
                        match (intent, tile.window().configure_intent()) {
                            (_, ConfigureIntent::ShouldSend) => ConfigureIntent::ShouldSend,
                            (ConfigureIntent::NotNeeded, tile_intent) => tile_intent,
                            (ConfigureIntent::CanSend, ConfigureIntent::Throttled) => {
                                ConfigureIntent::Throttled
                            }
                            (intent, _) => intent,
                        }
                    })
            };

            for (tile_idx, tile) in col.tiles.iter_mut().enumerate() {
                let win = tile.window_mut();

                let active_in_column = col.active_tile_idx == tile_idx;
                win.set_active_in_column(active_in_column);
                win.set_floating(false);

                let mut active = is_active && self.active_column_idx == col_idx;
                if self.options.deactivate_unfocused_windows {
                    active &= active_in_column && is_focused;
                } else {
                    // In tabbed mode, all tabs have activated state to reduce unnecessary
                    // animations when switching tabs.
                    active &= active_in_column || is_tabbed;
                }
                win.set_activated(active);

                win.set_interactive_resize(col_resize_data);

                let border_config = self.options.layout.border.merged_with(&win.rules().border);
                let bounds = compute_toplevel_bounds(
                    border_config,
                    self.working_area.size,
                    extra_size,
                    self.options.layout.gaps,
                );
                win.set_bounds(bounds);

                let intent = if individual_throttling {
                    win.configure_intent()
                } else {
                    intent
                };

                if matches!(
                    intent,
                    ConfigureIntent::CanSend | ConfigureIntent::ShouldSend
                ) {
                    win.send_pending_configure();
                }

                win.refresh();
            }
        }
    }
}
