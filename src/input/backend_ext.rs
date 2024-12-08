use ::input as libinput;
use smithay::backend::input;
use smithay::backend::winit::WinitVirtualDevice;
use smithay::output::Output;

use crate::backend::Backend;
use crate::niri::State;
use crate::protocols::virtual_pointer::VirtualPointer;

pub trait NiriInputBackend: input::InputBackend<Device = Self::NiriDevice> {
    type NiriDevice: NiriInputDevice;
}
impl<T: input::InputBackend> NiriInputBackend for T
where
    Self::Device: NiriInputDevice,
{
    type NiriDevice = Self::Device;
}

pub trait NiriInputDevice: input::Device {
    // FIXME: this should maybe be per-event, not per-device,
    // but it's not clear that this matters in practice?
    // it might be more obvious once we implement it for libinput
    fn output(&self, state: &State) -> Option<Output>;
}

impl NiriInputDevice for libinput::Device {
    fn output(&self, _state: &State) -> Option<Output> {
        // FIXME: Allow specifying the output per-device?
        None
    }
}

impl NiriInputDevice for WinitVirtualDevice {
    fn output(&self, state: &State) -> Option<Output> {
        match state.backend {
            Backend::Winit(ref winit) => Some(winit.single_output().clone()),
            // returning None over panicking here because it's not worth panicking over
            // and also, foreseeably, someone might want to, at some point, use `WinitInputBackend`
            // for dirty hacks or mocking or whatever, in which case this will be useful.
            _ => None,
        }
    }
}

impl NiriInputDevice for VirtualPointer {
    fn output(&self, _: &State) -> Option<Output> {
        self.output().cloned()
    }
}
