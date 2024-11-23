use ::input as libinput;
use smithay::backend::input;
use smithay::backend::winit::WinitVirtualDevice;
use smithay::output::Output;

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
    // FIXME: should this be per-event? logically yes,
    // but right now we only use it for virtual pointers, which have static outputs.
    fn output(&self) -> Option<Output>;
}

impl NiriInputDevice for libinput::Device {
    fn output(&self) -> Option<Output> {
        // FIXME: Allow specifying the output per-device?
        // In that case, change the method to take a reference to our state or config or something
        // (because we can't easily change the libinput Device struct)
        None
    }
}

impl NiriInputDevice for WinitVirtualDevice {
    fn output(&self) -> Option<Output> {
        // here it's actually *correct* to return None, because there is only one output.
        None
    }
}

impl NiriInputDevice for VirtualPointer {
    fn output(&self) -> Option<Output> {
        self.output().cloned()
    }
}
