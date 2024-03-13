use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

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
use smithay::utils::{Physical, Point, Rectangle, Size};
use smithay::wayland::shm;

// We do not support copy_with_damage() semantics yet.
const VERSION: u32 = 1;

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
        _manager: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        let (frame, overlay_cursor, buffer_size, region_loc, output) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor,
                output,
            } => {
                let output = Output::from_resource(&output).unwrap();
                let buffer_size = output.current_mode().unwrap().size;
                let region_loc = Point::from((0, 0));

                (frame, overlay_cursor, buffer_size, region_loc, output)
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
                if width <= 0 || height <= 0 {
                    trace!("screencopy client requested invalid sized region");
                    let frame = data_init.init(frame, ScreencopyFrameState::Failed);
                    frame.failed();
                    return;
                }

                let output = Output::from_resource(&output).unwrap();
                let output_transform = output.current_transform();
                let output_physical_size =
                    output_transform.transform_size(output.current_mode().unwrap().size);
                let output_rect = Rectangle::from_loc_and_size((0, 0), output_physical_size);

                let rect = Rectangle::from_loc_and_size((x, y), (width, height));

                let output_scale = output.current_scale().integer_scale();
                let physical_rect = rect.to_physical(output_scale);

                // Clamp captured region to the output.
                let Some(clamped_rect) = physical_rect.intersection(output_rect) else {
                    trace!("screencopy client requested region outside of output");
                    let frame = data_init.init(frame, ScreencopyFrameState::Failed);
                    frame.failed();
                    return;
                };

                let untransformed_rect = output_transform
                    .invert()
                    .transform_rect_in(clamped_rect, &output_physical_size);

                (
                    frame,
                    overlay_cursor,
                    untransformed_rect.size,
                    clamped_rect.loc,
                    output,
                )
            }
            zwlr_screencopy_manager_v1::Request::Destroy => return,
            _ => unreachable!(),
        };

        // Create the frame.
        let overlay_cursor = overlay_cursor != 0;
        let info = ScreencopyFrameInfo {
            output,
            overlay_cursor,
            buffer_size,
            region_loc,
        };
        let frame = data_init.init(
            frame,
            ScreencopyFrameState::Pending {
                info,
                copied: Arc::new(AtomicBool::new(false)),
            },
        );

        // Send desired SHM buffer parameters.
        frame.buffer(
            wl_shm::Format::Argb8888,
            buffer_size.w as u32,
            buffer_size.h as u32,
            buffer_size.w as u32 * 4,
        );

        // if manager.version() >= 3 {
        //     // Send desired DMA buffer parameters.
        //     frame.linux_dmabuf(
        //         Fourcc::Argb8888 as u32,
        //         buffer_size.w as u32,
        //         buffer_size.h as u32,
        //     );
        //
        //     // Notify client that all supported buffers were enumerated.
        //     frame.buffer_done();
        // }
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
pub struct ScreencopyFrameInfo {
    output: Output,
    buffer_size: Size<i32, Physical>,
    region_loc: Point<i32, Physical>,
    overlay_cursor: bool,
}

pub enum ScreencopyFrameState {
    Failed,
    Pending {
        info: ScreencopyFrameInfo,
        copied: Arc<AtomicBool>,
    },
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
        if matches!(request, zwlr_screencopy_frame_v1::Request::Destroy) {
            return;
        }

        let (info, copied) = match data {
            ScreencopyFrameState::Failed => return,
            ScreencopyFrameState::Pending { info, copied } => (info, copied),
        };

        if copied.load(Ordering::SeqCst) {
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::AlreadyUsed,
                "copy was already requested",
            );
            return;
        }

        let (buffer, with_damage) = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => (buffer, false),
            // zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => (buffer, true),
            _ => unreachable!(),
        };

        if !shm::with_buffer_contents(&buffer, |_buf, shm_len, buffer_data| {
            buffer_data.format == wl_shm::Format::Argb8888
                && buffer_data.stride == info.buffer_size.w * 4
                && buffer_data.height == info.buffer_size.h
                && shm_len as i32 == buffer_data.stride * buffer_data.height
        })
        .unwrap_or(false)
        {
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::InvalidBuffer,
                "invalid buffer",
            );
            return;
        }

        copied.store(true, Ordering::SeqCst);

        state.frame(Screencopy {
            with_damage,
            buffer,
            frame: frame.clone(),
            info: info.clone(),
            submitted: false,
        });
    }
}

/// Screencopy frame.
pub struct Screencopy {
    info: ScreencopyFrameInfo,
    frame: ZwlrScreencopyFrameV1,
    #[allow(unused)]
    with_damage: bool,
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

    pub fn region_loc(&self) -> Point<i32, Physical> {
        self.info.region_loc
    }

    pub fn buffer_size(&self) -> Size<i32, Physical> {
        self.info.buffer_size
    }

    pub fn output(&self) -> &Output {
        &self.info.output
    }

    pub fn overlay_cursor(&self) -> bool {
        self.info.overlay_cursor
    }

    // pub fn damage(&mut self, damage: &[Rectangle<i32, Physical>]) {
    //     assert!(self.with_damage);
    //
    //     for Rectangle { loc, size } in damage {
    //         self.frame
    //             .damage(loc.x as u32, loc.y as u32, size.w as u32, size.h as u32);
    //     }
    // }

    /// Submit the copied content.
    pub fn submit(mut self, y_invert: bool) {
        // Notify client that buffer is ordinary.
        self.frame.flags(if y_invert {
            Flags::YInvert
        } else {
            Flags::empty()
        });

        // Notify client about successful copy.
        let time = UNIX_EPOCH.elapsed().unwrap();
        let tv_sec_hi = (time.as_secs() >> 32) as u32;
        let tv_sec_lo = (time.as_secs() & 0xFFFFFFFF) as u32;
        let tv_nsec = time.subsec_nanos();
        self.frame.ready(tv_sec_hi, tv_sec_lo, tv_nsec);

        // Mark frame as submitted to ensure destructor isn't run.
        self.submitted = true;
    }

    // pub fn submit_after_sync<T>(
    //     self,
    //     y_invert: bool,
    //     sync_point: Option<OwnedFd>,
    //     event_loop: &LoopHandle<'_, T>,
    // ) {
    //     match sync_point {
    //         None => self.submit(y_invert),
    //         Some(sync_fd) => {
    //             let source = Generic::new(sync_fd, Interest::READ, Mode::OneShot);
    //             let mut screencopy = Some(self);
    //             event_loop
    //                 .insert_source(source, move |_, _, _| {
    //                     screencopy.take().unwrap().submit(y_invert);
    //                     Ok(PostAction::Remove)
    //                 })
    //                 .unwrap();
    //         }
    //     }
    // }
}
