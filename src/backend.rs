use smithay::backend::renderer::gles::GlesRenderer;
use smithay::output::Output;

use crate::niri::OutputRenderElements;
use crate::tty::Tty;
use crate::winit::Winit;
use crate::Niri;

pub enum Backend {
    Tty(Tty),
    Winit(Winit),
}

impl Backend {
    pub fn init(&mut self, niri: &mut Niri) {
        match self {
            Backend::Tty(tty) => tty.init(niri),
            Backend::Winit(winit) => winit.init(niri),
        }
    }

    pub fn seat_name(&self) -> String {
        match self {
            Backend::Tty(tty) => tty.seat_name(),
            Backend::Winit(winit) => winit.seat_name(),
        }
    }

    pub fn renderer(&mut self) -> &mut GlesRenderer {
        match self {
            Backend::Tty(tty) => tty.renderer(),
            Backend::Winit(winit) => winit.renderer(),
        }
    }

    pub fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
    ) {
        match self {
            Backend::Tty(tty) => tty.render(niri, output, elements),
            Backend::Winit(winit) => winit.render(niri, output, elements),
        }
    }

    pub fn tty(&mut self) -> Option<&mut Tty> {
        if let Self::Tty(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn winit(&mut self) -> Option<&mut Winit> {
        if let Self::Winit(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
