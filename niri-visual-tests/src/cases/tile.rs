use std::rc::Rc;
use std::time::Duration;

use niri::layout::Options;
use niri::render_helpers::RenderTarget;
use niri_config::Color;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Point, Scale, Size};

use super::TestCase;
use crate::test_window::TestWindow;

pub struct Tile {
    window: TestWindow,
    tile: niri::layout::tile::Tile<TestWindow>,
}

impl Tile {
    pub fn freeform(size: Size<i32, Logical>) -> Self {
        let window = TestWindow::freeform(0);
        let mut rv = Self::with_window(window);
        rv.tile.request_tile_size(size);
        rv.window.communicate();
        rv
    }

    pub fn fixed_size(size: Size<i32, Logical>) -> Self {
        let window = TestWindow::fixed_size(0);
        let mut rv = Self::with_window(window);
        rv.tile.request_tile_size(size);
        rv.window.communicate();
        rv
    }

    pub fn fixed_size_with_csd_shadow(size: Size<i32, Logical>) -> Self {
        let window = TestWindow::fixed_size(0);
        window.set_csd_shadow_width(64);
        let mut rv = Self::with_window(window);
        rv.tile.request_tile_size(size);
        rv.window.communicate();
        rv
    }

    pub fn freeform_open(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::freeform(size);
        rv.window.set_color([0.1, 0.1, 0.1, 1.]);
        rv.tile.start_open_animation();
        rv
    }

    pub fn fixed_size_open(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::fixed_size(size);
        rv.window.set_color([0.1, 0.1, 0.1, 1.]);
        rv.tile.start_open_animation();
        rv
    }

    pub fn fixed_size_with_csd_shadow_open(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::fixed_size_with_csd_shadow(size);
        rv.window.set_color([0.1, 0.1, 0.1, 1.]);
        rv.tile.start_open_animation();
        rv
    }

    pub fn with_window(window: TestWindow) -> Self {
        let options = Options {
            focus_ring: niri_config::FocusRing {
                off: true,
                ..Default::default()
            },
            border: niri_config::Border {
                off: false,
                width: 32,
                active_color: Color::new(255, 163, 72, 255),
                ..Default::default()
            },
            ..Default::default()
        };
        let tile = niri::layout::tile::Tile::new(window.clone(), Rc::new(options));
        Self { window, tile }
    }
}

impl TestCase for Tile {
    fn resize(&mut self, width: i32, height: i32) {
        self.tile.request_tile_size(Size::from((width, height)));
        self.window.communicate();
    }

    fn are_animations_ongoing(&self) -> bool {
        self.tile.are_animations_ongoing()
    }

    fn advance_animations(&mut self, current_time: Duration) {
        self.tile.advance_animations(current_time, true);
    }

    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
    ) -> Vec<Box<dyn RenderElement<GlesRenderer>>> {
        let tile_size = self.tile.tile_size().to_physical(1);
        let location = Point::from(((size.w - tile_size.w) / 2, (size.h - tile_size.h) / 2));

        self.tile
            .render(
                renderer,
                location,
                Scale::from(1.),
                size.to_logical(1),
                true,
                RenderTarget::Output,
            )
            .map(|elem| Box::new(elem) as _)
            .collect()
    }
}
