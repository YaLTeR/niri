use niri::layout::LayoutElement;
use niri::render_helpers::RenderTarget;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Point, Scale, Size};

use super::TestCase;
use crate::test_window::TestWindow;

pub struct Window {
    window: TestWindow,
}

impl Window {
    pub fn freeform(size: Size<i32, Logical>) -> Self {
        let window = TestWindow::freeform(0);
        window.request_size(size);
        window.communicate();
        Self { window }
    }

    pub fn fixed_size(size: Size<i32, Logical>) -> Self {
        let window = TestWindow::fixed_size(0);
        window.request_size(size);
        window.communicate();
        Self { window }
    }

    pub fn fixed_size_with_csd_shadow(size: Size<i32, Logical>) -> Self {
        let window = TestWindow::fixed_size(0);
        window.set_csd_shadow_width(64);
        window.request_size(size);
        window.communicate();
        Self { window }
    }
}

impl TestCase for Window {
    fn resize(&mut self, width: i32, height: i32) {
        self.window.request_size(Size::from((width, height)));
        self.window.communicate();
    }

    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
    ) -> Vec<Box<dyn RenderElement<GlesRenderer>>> {
        let win_size = self.window.size().to_physical(1);
        let location = Point::from(((size.w - win_size.w) / 2, (size.h - win_size.h) / 2));

        self.window
            .render(
                renderer,
                location,
                Scale::from(1.),
                1.,
                RenderTarget::Output,
            )
            .into_iter()
            .map(|elem| Box::new(elem) as _)
            .collect()
    }
}
