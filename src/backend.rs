use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::output::Output;

use crate::niri::OutputRenderElements;
use crate::Niri;

pub trait Backend {
    fn seat_name(&self) -> String;
    fn renderer(&mut self) -> &mut GlesRenderer;
    fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        elements: &[OutputRenderElements<
            GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>,
        >],
    );
}
