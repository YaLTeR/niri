use anyhow::bail;
use smithay::reexports::calloop::LoopHandle;

use crate::niri::State;

pub struct PipeWire;
pub struct Cast;

impl PipeWire {
    pub fn new(_event_loop: &LoopHandle<'static, State>) -> anyhow::Result<Self> {
        bail!("PipeWire support is disabled (see \"xdp-gnome-screencast\" feature)");
    }
}
