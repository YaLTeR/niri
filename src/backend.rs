use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::space::SpaceRenderElements;

use crate::Niri;

pub trait Backend {
    fn seat_name(&self) -> String;
    fn renderer(&mut self) -> &mut GlesRenderer;
    fn render(
        &mut self,
        niri: &mut Niri,
        elements: &[SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>],
    );
}
