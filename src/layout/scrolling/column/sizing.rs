// Column sizing implementation - size calculation, width/height resolution, and complex height distribution
//
// This file contains the Column methods for:
// - Size calculation and extra size computation
// - Preset width/height resolution
// - Column width resolution
// - Complex tile height distribution algorithm
// - Width computation

use std::iter::zip;

use niri_config::PresetSize;
use niri_ipc::ColumnDisplay;
use ordered_float::NotNan;
use smithay::utils::{Logical, Size};

use super::super::super::tile::Tile;
use super::super::super::LayoutElement;
use super::super::types::WindowHeight;
use super::super::utils::resolve_preset_size;
use super::super::super::workspace::ResolvedSize;
use super::{Column, ColumnWidth};
use crate::utils::transaction::Transaction;
use crate::layout::SizingMode;

impl<W: LayoutElement> Column<W> {
    /// Extra size taken up by elements in the column such as the tab indicator.
    pub(in crate::layout::scrolling) fn extra_size(&self) -> Size<f64, Logical> {
        if self.display_mode == ColumnDisplay::Tabbed {
            self.tab_indicator.extra_size(self.tiles.len(), self.scale)
        } else {
            Size::from((0., 0.))
        }
    }

    pub(in crate::layout::scrolling) fn resolve_preset_width(&self, preset: PresetSize) -> ResolvedSize {
        let extra = self.extra_size();
        resolve_preset_size(preset, &self.options, self.working_area.size.w, extra.w)
    }

    pub(in crate::layout::scrolling) fn resolve_preset_height(&self, preset: PresetSize) -> ResolvedSize {
        let extra = self.extra_size();
        resolve_preset_size(preset, &self.options, self.working_area.size.h, extra.h)
    }

    pub(in crate::layout::scrolling) fn resolve_column_width(&self, width: ColumnWidth) -> f64 {
        let working_size = self.working_area.size;
        let gaps = self.options.layout.gaps;
        let extra = self.extra_size();

        match width {
            ColumnWidth::Proportion(proportion) => {
                (working_size.w - gaps) * proportion - gaps - extra.w
            }
            ColumnWidth::Fixed(width) => width,
        }
    }

    pub(in crate::layout::scrolling) fn update_tile_sizes(&mut self, animate: bool) {
        self.update_tile_sizes_with_transaction(animate, Transaction::new());
    }

    pub(in crate::layout::scrolling) fn update_tile_sizes_with_transaction(&mut self, animate: bool, transaction: Transaction) {
        let sizing_mode = self.pending_sizing_mode();
        if matches!(sizing_mode, SizingMode::Fullscreen | SizingMode::Maximized) {
            for (tile_idx, tile) in self.tiles.iter_mut().enumerate() {
                // In tabbed mode, only the visible window participates in the transaction.
                let is_active = tile_idx == self.active_tile_idx;
                let transaction = if self.display_mode == ColumnDisplay::Tabbed && !is_active {
                    None
                } else {
                    Some(transaction.clone())
                };

                if matches!(sizing_mode, SizingMode::Fullscreen) {
                    tile.request_fullscreen(animate, transaction);
                } else {
                    tile.request_maximized(self.parent_area.size, animate, transaction);
                }
            }
            return;
        }

        let is_tabbed = self.display_mode == ColumnDisplay::Tabbed;

        let min_size: Vec<_> = self
            .tiles
            .iter()
            .map(Tile::min_size_nonfullscreen)
            .map(|mut size| {
                size.w = size.w.max(1.);
                size.h = size.h.max(1.);
                size
            })
            .collect();
        let max_size: Vec<_> = self
            .tiles
            .iter()
            .map(Tile::max_size_nonfullscreen)
            .collect();

        // Compute the column width.
        let min_width = min_size
            .iter()
            .map(|size| NotNan::new(size.w).unwrap())
            .max()
            .map(NotNan::into_inner)
            .unwrap();
        let max_width = max_size
            .iter()
            .filter_map(|size| {
                let w = size.w;
                if w == 0. {
                    None
                } else {
                    Some(NotNan::new(w).unwrap())
                }
            })
            .min()
            .map(NotNan::into_inner)
            .unwrap_or(f64::from(i32::MAX));
        let max_width = f64::max(max_width, min_width);

        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let working_size = self.working_area.size;
        let extra_size = self.extra_size();

        let width = self.resolve_column_width(width);
        let width = f64::max(f64::min(width, max_width), min_width);
        let max_tile_height = working_size.h - self.options.layout.gaps * 2. - extra_size.h;

        // If there are multiple windows in a column, clamp the non-auto window's height according
        // to other windows' min sizes.
        let mut max_non_auto_window_height = None;
        if self.tiles.len() > 1 && !is_tabbed {
            if let Some(non_auto_idx) = self
                .data
                .iter()
                .position(|data| !matches!(data.height, WindowHeight::Auto { .. }))
            {
                let min_height_taken = min_size
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| *idx != non_auto_idx)
                    .map(|(_, min_size)| min_size.h + self.options.layout.gaps)
                    .sum::<f64>();

                let tile = &self.tiles[non_auto_idx];
                let height_left = max_tile_height - min_height_taken;
                max_non_auto_window_height = Some(f64::max(
                    1.,
                    tile.window_height_for_tile_height(height_left).round(),
                ));
            }
        }

        // Compute the tile heights. Start by converting window heights to tile heights.
        let mut heights = zip(&self.tiles, &self.data)
            .map(|(tile, data)| match data.height {
                auto @ WindowHeight::Auto { .. } => auto,
                WindowHeight::Fixed(height) => {
                    let mut window_height = height.round().max(1.);
                    if let Some(max) = max_non_auto_window_height {
                        window_height = f64::min(window_height, max);
                    } else {
                        // In any case, clamp to the working area height.
                        let max = tile.window_height_for_tile_height(max_tile_height).round();
                        window_height = f64::min(window_height, max);
                    }

                    WindowHeight::Fixed(tile.tile_height_for_window_height(window_height))
                }
                WindowHeight::Preset(idx) => {
                    let preset = self.options.layout.preset_window_heights[idx];
                    let window_height = match self.resolve_preset_height(preset) {
                        ResolvedSize::Tile(h) => tile.window_height_for_tile_height(h),
                        ResolvedSize::Window(h) => h,
                    };

                    let mut window_height = window_height.round().clamp(1., 100000.);
                    if let Some(max) = max_non_auto_window_height {
                        window_height = f64::min(window_height, max);
                    }

                    let tile_height = tile.tile_height_for_window_height(window_height);
                    WindowHeight::Fixed(tile_height)
                }
            })
            .collect::<Vec<_>>();

        // In tabbed display mode, fill fixed heights right away.
        if is_tabbed {
            // All tiles have the same height, equal to the height of the only fixed tile (if any).
            let tabbed_height = heights
                .iter()
                .find_map(|h| {
                    if let WindowHeight::Fixed(h) = h {
                        Some(*h)
                    } else {
                        None
                    }
                })
                .unwrap_or(max_tile_height);

            // We also take min height of all tabs into account.
            let min_height = min_size
                .iter()
                .map(|size| NotNan::new(size.h).unwrap())
                .max()
                .map(NotNan::into_inner)
                .unwrap();
            // But, if there's a larger-than-workspace tab, we don't want to force all tabs to that
            // size.
            let min_height = f64::min(max_tile_height, min_height);
            let tabbed_height = f64::max(tabbed_height, min_height);

            heights.fill(WindowHeight::Fixed(tabbed_height));

            // The following logic will apply individual min/max height, etc.
        }

        let gaps_left = self.options.layout.gaps * (self.tiles.len() + 1) as f64;
        let mut height_left = working_size.h - gaps_left;
        let mut auto_tiles_left = self.tiles.len();

        // Subtract all fixed-height tiles.
        for (h, (min_size, max_size)) in zip(&mut heights, zip(&min_size, &max_size)) {
            // Check if the tile has an exact height constraint.
            if min_size.h == max_size.h {
                *h = WindowHeight::Fixed(min_size.h);
            }

            if let WindowHeight::Fixed(h) = h {
                if max_size.h > 0. {
                    *h = f64::min(*h, max_size.h);
                }
                *h = f64::max(*h, min_size.h);

                height_left -= *h;
                auto_tiles_left -= 1;
            }
        }

        let mut total_weight: f64 = heights
            .iter()
            .filter_map(|h| {
                if let WindowHeight::Auto { weight } = *h {
                    Some(weight)
                } else {
                    None
                }
            })
            .sum();

        // Iteratively try to distribute the remaining height, checking against tile min heights.
        // Pick an auto height according to the current sizes, then check if it satisfies all
        // remaining min heights. If not, allocate fixed height to those tiles and repeat the
        // loop. On each iteration the auto height will get smaller.
        //
        // NOTE: we do not respect max height here. Doing so would complicate things: if the current
        // auto height is above some tile's max height, then the auto height can become larger.
        // Combining this with the min height loop is where the complexity appears.
        //
        // However, most max height uses are for fixed-size dialogs, where min height == max_height.
        // This case is separately handled above.
        'outer: while auto_tiles_left > 0 {
            // Wayland requires us to round the requested size for a window to integer logical
            // pixels, therefore we compute the remaining auto height dynamically.
            let mut height_left_2 = height_left;
            let mut total_weight_2 = total_weight;
            for ((h, tile), min_size) in zip(zip(&mut heights, &self.tiles), &min_size) {
                let weight = match *h {
                    WindowHeight::Auto { weight } => weight,
                    WindowHeight::Fixed(_) => continue,
                    WindowHeight::Preset(_) => unreachable!(),
                };
                let factor = weight / total_weight_2;

                // Compute the current auto height.
                let mut auto = height_left_2 * factor;

                // Check if the auto height satisfies the min height.
                if min_size.h > auto {
                    auto = min_size.h;
                    *h = WindowHeight::Fixed(auto);
                    height_left -= auto;
                    total_weight -= weight;
                    auto_tiles_left -= 1;

                    // If a min height was unsatisfied, then we allocate the tile more than the
                    // auto height, which means that the remaining auto tiles now have less height
                    // to work with, and the loop must run again.
                    //
                    // If we keep going in this loop and break out later, we may allocate less
                    // height to the subsequent tiles than would be available next iteration and
                    // potentially trip their min height check earlier than necessary, leading to
                    // visible snapping.
                    continue 'outer;
                }

                auto = tile.tile_height_for_window_height(
                    tile.window_height_for_tile_height(auto).round().max(1.),
                );

                height_left_2 -= auto;
                total_weight_2 -= weight;
            }

            // All min heights were satisfied, fill them in.
            for (h, tile) in zip(&mut heights, &self.tiles) {
                let weight = match *h {
                    WindowHeight::Auto { weight } => weight,
                    WindowHeight::Fixed(_) => continue,
                    WindowHeight::Preset(_) => unreachable!(),
                };
                let factor = weight / total_weight;

                // Compute the current auto height.
                let auto = height_left * factor;
                let auto = tile.tile_height_for_window_height(
                    tile.window_height_for_tile_height(auto).round().max(1.),
                );

                *h = WindowHeight::Fixed(auto);
                height_left -= auto;
                total_weight -= weight;
                auto_tiles_left -= 1;
            }

            assert_eq!(auto_tiles_left, 0);
        }

        for (tile_idx, (tile, h)) in zip(&mut self.tiles, heights).enumerate() {
            let WindowHeight::Fixed(height) = h else {
                unreachable!()
            };

            let size = Size::from((width, height));

            // In tabbed mode, only the visible window participates in the transaction.
            let is_active = tile_idx == self.active_tile_idx;
            let transaction = if self.display_mode == ColumnDisplay::Tabbed && !is_active {
                None
            } else {
                Some(transaction.clone())
            };

            tile.request_tile_size(size, animate, transaction);
        }
    }

    pub fn width(&self) -> f64 {
        let mut tiles_width = self
            .data
            .iter()
            .map(|data| NotNan::new(data.size.w).unwrap())
            .max()
            .map(NotNan::into_inner)
            .unwrap();

        if self.display_mode == ColumnDisplay::Tabbed && self.sizing_mode().is_normal() {
            let extra_size = self.tab_indicator.extra_size(self.tiles.len(), self.scale);
            tiles_width += extra_size.w;
        }

        tiles_width
    }
}
