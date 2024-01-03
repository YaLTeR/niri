use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::output::Output;

use crate::input::CompositorMod;
use crate::niri::OutputRenderElements;
use crate::Niri;

pub mod tty;
pub use tty::Tty;

pub mod winit;
pub use winit::Winit;

pub enum Backend {
    Tty(Tty),
    Winit(Winit),
}

#[derive(PartialEq, Eq)]
pub enum RenderResult {
    /// The frame was submitted to the backend for presentation.
    Submitted,
    /// Rendering succeeded, but there was no damage.
    NoDamage,
    /// An error has occurred, the frame was not submitted.
    Error,
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

    pub fn renderer(&mut self) -> Option<&mut GlesRenderer> {
        match self {
            Backend::Tty(tty) => tty.renderer(),
            Backend::Winit(winit) => Some(winit.renderer()),
        }
    }

    pub fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
        target_presentation_time: Duration,
    ) -> RenderResult {
        match self {
            Backend::Tty(tty) => tty.render(niri, output, elements, target_presentation_time),
            Backend::Winit(winit) => winit.render(niri, output, elements),
        }
    }

    pub fn mod_key(&self) -> CompositorMod {
        match self {
            Backend::Tty(_) => CompositorMod::Super,
            Backend::Winit(_) => CompositorMod::Alt,
        }
    }

    pub fn change_vt(&mut self, vt: i32) {
        match self {
            Backend::Tty(tty) => tty.change_vt(vt),
            Backend::Winit(_) => (),
        }
    }

    pub fn suspend(&mut self) {
        match self {
            Backend::Tty(tty) => tty.suspend(),
            Backend::Winit(_) => (),
        }
    }

    pub fn toggle_debug_tint(&mut self) {
        match self {
            Backend::Tty(tty) => tty.toggle_debug_tint(),
            Backend::Winit(winit) => winit.toggle_debug_tint(),
        }
    }

    pub fn import_dmabuf(&mut self, dmabuf: &Dmabuf) -> Result<(), ()> {
        match self {
            Backend::Tty(tty) => tty.import_dmabuf(dmabuf),
            Backend::Winit(winit) => winit.import_dmabuf(dmabuf),
        }
    }

    #[cfg_attr(not(feature = "dbus"), allow(unused))]
    pub fn connectors(&self) -> Arc<Mutex<HashMap<String, Output>>> {
        match self {
            Backend::Tty(tty) => tty.connectors(),
            Backend::Winit(winit) => winit.connectors(),
        }
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn gbm_device(
        &self,
    ) -> Option<smithay::backend::allocator::gbm::GbmDevice<smithay::backend::drm::DrmDeviceFd>>
    {
        match self {
            Backend::Tty(tty) => tty.gbm_device(),
            Backend::Winit(_) => None,
        }
    }

    pub fn is_active(&self) -> bool {
        match self {
            Backend::Tty(tty) => tty.is_active(),
            Backend::Winit(_) => true,
        }
    }

    pub fn set_monitors_active(&self, active: bool) {
        match self {
            Backend::Tty(tty) => tty.set_monitors_active(active),
            Backend::Winit(_) => (),
        }
    }

    pub fn tty(&mut self) -> &mut Tty {
        if let Self::Tty(v) = self {
            v
        } else {
            panic!("backend is not Tty");
        }
    }

    pub fn winit(&mut self) -> &mut Winit {
        if let Self::Winit(v) = self {
            v
        } else {
            panic!("backend is not Winit")
        }
    }
}
