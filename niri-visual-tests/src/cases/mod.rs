use std::time::Duration;

use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Physical, Size};

pub mod gradient_angle;
pub mod gradient_area;
pub mod gradient_srgb;
pub mod gradient_srgblinear;
pub mod gradient_oklab;
pub mod gradient_oklch_shorter;
pub mod gradient_oklch_longer;
pub mod gradient_oklch_increasing;
pub mod gradient_oklch_decreasing;
pub mod gradient_oklch_alpha;
pub mod layout;
pub mod tile;
pub mod window;

pub trait TestCase {
    fn resize(&mut self, _width: i32, _height: i32) {}
    fn are_animations_ongoing(&self) -> bool {
        false
    }
    fn advance_animations(&mut self, _current_time: Duration) {}
    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
    ) -> Vec<Box<dyn RenderElement<GlesRenderer>>>;
}
