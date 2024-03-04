use std::os::fd::OwnedFd;
use std::time::UNIX_EPOCH;

use calloop::generic::Generic;
use calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_frame_v1::{
    Flags, ZwlrScreencopyFrameV1,
};
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::utils::{Physical, Rectangle};
use smithay::wayland::shm;

const VERSION: u32 = 3;

pub struct ScreencopyManagerState;

pub struct ScreencopyManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

impl ScreencopyManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: GlobalDispatch<ZwlrScreencopyManagerV1, ScreencopyManagerGlobalData>,
        D: Dispatch<ZwlrScreencopyManagerV1, ()>,
        D: Dispatch<ZwlrScreencopyFrameV1, ScreencopyFrameState>,
        D: ScreencopyHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = ScreencopyManagerGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, ZwlrScreencopyManagerV1, _>(VERSION, global_data);

        Self
    }
}

impl<D> GlobalDispatch<ZwlrScreencopyManagerV1, ScreencopyManagerGlobalData, D>
    for ScreencopyManagerState
where
    D: GlobalDispatch<ZwlrScreencopyManagerV1, ScreencopyManagerGlobalData>,
    D: Dispatch<ZwlrScreencopyManagerV1, ()>,
    D: Dispatch<ZwlrScreencopyFrameV1, ScreencopyFrameState>,
    D: ScreencopyHandler,
    D: 'static,
{
    fn bind(
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        manager: New<ZwlrScreencopyManagerV1>,
        _manager_state: &ScreencopyManagerGlobalData,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(manager, ());
    }

    fn can_view(client: Client, global_data: &ScreencopyManagerGlobalData) -> bool {
        (global_data.filter)(&client)
    }
}

impl<D> Dispatch<ZwlrScreencopyManagerV1, (), D> for ScreencopyManagerState
where
    D: GlobalDispatch<ZwlrScreencopyManagerV1, ScreencopyManagerGlobalData>,
    D: Dispatch<ZwlrScreencopyManagerV1, ()>,
    D: Dispatch<ZwlrScreencopyFrameV1, ScreencopyFrameState>,
    D: ScreencopyHandler,
    D: 'static,
{
    fn request(
        _state: &mut D,
        _client: &Client,
        manager: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        let (frame, overlay_cursor, rect, output) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor,
                output,
            } => {
                let output = Output::from_resource(&output).unwrap();
                let rect =
                    Rectangle::from_loc_and_size((0, 0), output.current_mode().unwrap().size);
                (frame, overlay_cursor, rect, output)
            }
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                overlay_cursor,
                x,
                y,
                width,
                height,
                output,
            } => {
                let output = Output::from_resource(&output).unwrap();
                let (x, width) = if width < 0 {
                    (x + width, -width)
                } else {
                    (x, width)
                };
                let (y, height) = if height < 0 {
                    (y + height, -height)
                } else {
                    (y, height)
                };
                let rect = Rectangle::from_loc_and_size((x, y), (width, height));

                // Translate logical rect to physical framebuffer coordinates.
                let output_transform = output.current_transform();
                let rotated_rect = output_transform.transform_rect_in(
                    rect,
                    &output
                        .current_mode()
                        .unwrap()
                        .size
                        .to_f64()
                        .to_logical(output.current_scale().fractional_scale())
                        .to_i32_round(),
                );
                let physical_rect =
                    rotated_rect.to_physical_precise_round(output.current_scale().integer_scale());

                // Clamp captured region to the output.
                let clamped_rect = physical_rect
                    .intersection(Rectangle::from_loc_and_size(
                        (0, 0),
                        output.current_mode().unwrap().size,
                    ))
                    .unwrap_or_default();

                (frame, overlay_cursor, clamped_rect, output)
            }
            zwlr_screencopy_manager_v1::Request::Destroy => return,
            _ => unreachable!(),
        };

        // Create the frame.
        let overlay_cursor = overlay_cursor != 0;
        let frame = data_init.init(
            frame,
            ScreencopyFrameState {
                output,
                overlay_cursor,
                rect,
            },
        );

        // Send desired SHM buffer parameters.
        frame.buffer(
            wl_shm::Format::Argb8888,
            rect.size.w as u32,
            rect.size.h as u32,
            rect.size.w as u32 * 4,
        );

        if manager.version() >= 3 {
            // // Send desired DMA buffer parameters.
            // frame.linux_dmabuf(
            //     Fourcc::Argb8888 as u32,
            //     rect.size.w as u32,
            //     rect.size.h as u32,
            // );

            // Notify client that all supported buffers were enumerated.
            frame.buffer_done();
        }
    }
}

/// Handler trait for wlr-screencopy.
pub trait ScreencopyHandler {
    /// Handle new screencopy request.
    fn frame(&mut self, frame: Screencopy);
}

#[allow(missing_docs)]
#[macro_export]
macro_rules! delegate_screencopy {
    ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
        smithay::reexports::wayland_server::delegate_global_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1: $crate::protocols::screencopy::ScreencopyManagerGlobalData
        ] => $crate::protocols::screencopy::ScreencopyManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1: ()
        ] => $crate::protocols::screencopy::ScreencopyManagerState);

        smithay::reexports::wayland_server::delegate_dispatch!($(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $ty: [
            smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1: $crate::protocols::screencopy::ScreencopyFrameState
        ] => $crate::protocols::screencopy::ScreencopyManagerState);
    };
}

#[derive(Clone)]
pub struct ScreencopyFrameState {
    pub output: Output,
    pub rect: Rectangle<i32, Physical>,
    pub overlay_cursor: bool,
}

impl<D> Dispatch<ZwlrScreencopyFrameV1, ScreencopyFrameState, D> for ScreencopyManagerState
where
    D: Dispatch<ZwlrScreencopyFrameV1, ScreencopyFrameState>,
    D: ScreencopyHandler,
    D: 'static,
{
    fn request(
        state: &mut D,
        _client: &Client,
        frame: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        data: &ScreencopyFrameState,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        let (buffer, send_damage) = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => (buffer, false),
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => (buffer, true),
            zwlr_screencopy_frame_v1::Request::Destroy => return,
            _ => unreachable!(),
        };

        if let Ok(true) = shm::with_buffer_contents(&buffer, |_buf, shm_len, buffer_data| {
            buffer_data.format == wl_shm::Format::Argb8888
                && buffer_data.stride == data.rect.size.w * 4
                && buffer_data.height == data.rect.size.h
                && shm_len as i32 == buffer_data.stride * buffer_data.height
        }) {
            state.frame(Screencopy {
                send_damage,
                buffer,
                frame: frame.clone(),
                frame_state: data.clone(),
                submitted: false,
            });
        } else {
            warn!("Client provided invalid buffer for screencopy. Rejecting.");
            frame.failed();
        }
    }
}

/// Screencopy frame.
pub struct Screencopy {
    frame_state: ScreencopyFrameState,
    frame: ZwlrScreencopyFrameV1,
    send_damage: bool,
    buffer: WlBuffer,
    submitted: bool,
}

impl Drop for Screencopy {
    fn drop(&mut self) {
        if !self.submitted {
            self.frame.failed();
        }
    }
}

impl Screencopy {
    /// Get the target buffer to copy to.
    pub fn buffer(&self) -> &WlBuffer {
        &self.buffer
    }

    /// Get the region which should be copied.
    pub fn region(&self) -> Rectangle<i32, Physical> {
        self.frame_state.rect
    }

    pub fn output(&self) -> &Output {
        &self.frame_state.output
    }

    pub fn overlay_cursor(&self) -> bool {
        self.frame_state.overlay_cursor
    }

    /// Mark damaged regions of the screencopy buffer.
    pub fn damage(&mut self, damage: &[Rectangle<i32, Physical>]) {
        if !self.send_damage {
            return;
        }

        for Rectangle { loc, size } in damage {
            self.frame
                .damage(loc.x as u32, loc.y as u32, size.w as u32, size.h as u32);
        }
    }

    /// Submit the copied content.
    pub fn submit(mut self, y_invert: bool) {
        // Notify client that buffer is ordinary.
        self.frame.flags(if y_invert {
            Flags::YInvert
        } else {
            Flags::empty()
        });

        // Notify client about successful copy.
        let now = UNIX_EPOCH.elapsed().unwrap();
        let secs = now.as_secs();
        self.frame
            .ready((secs >> 32) as u32, secs as u32, now.subsec_nanos());

        // Mark frame as submitted to ensure destructor isn't run.
        self.submitted = true;
    }

    pub fn submit_after_sync<T>(
        self,
        y_invert: bool,
        sync_point: Option<OwnedFd>,
        event_loop: &LoopHandle<'_, T>,
    ) {
        match sync_point {
            None => self.submit(y_invert),
            Some(sync_fd) => {
                let source = Generic::new(sync_fd, Interest::READ, Mode::OneShot);
                let mut screencopy = Some(self);
                event_loop
                    .insert_source(source, move |_, _, _| {
                        screencopy.take().unwrap().submit(y_invert);
                        Ok(PostAction::Remove)
                    })
                    .unwrap();
            }
        }
    }
}
