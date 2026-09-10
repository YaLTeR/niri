use std::collections::HashMap;

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::{Buffer as _, Fourcc, Modifier};
use smithay::backend::drm::DrmNode;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::output::{Output, WeakOutput};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_pointer::WlPointer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Buffer as BufferCoords, Logical, Physical, Point, Scale, Size, Transform};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::image_capture_source::{
    ImageCaptureSource, ImageCaptureSourceHandler, OutputCaptureSourceHandler,
    OutputCaptureSourceState,
};
use smithay::wayland::image_copy_capture::{
    BufferConstraints, CaptureFailureReason, CursorSession, CursorSessionRef, DmabufConstraints,
    Frame, FrameRef, ImageCopyCaptureHandler, ImageCopyCaptureState, Session, SessionRef,
};
use smithay::wayland::shm;

use crate::cursor::{RenderCursor, XCursor};
use crate::niri::{Niri, State};

/// Output capture session.
pub struct ImageCopySession {
    pub session: Session,
    pub damage_tracker: OutputDamageTracker,
    /// Frame waiting for output damage.
    pub pending_frame: Option<Frame>,
}

/// Cursor capture session of an output.
pub struct ImageCopyCursorSession {
    pub session: CursorSession,
    /// Damage to the cursor image (i.e., not movement).
    pub damage_tracker: OutputDamageTracker,
    /// Frame waiting for cursor image change.
    pub pending_frame: Option<Frame>,
}

/// Output captured by a session, or `None` if it's gone (sessions can outlive
/// their output).
pub fn source_output(source: &ImageCaptureSource) -> Option<Output> {
    source.user_data().get::<WeakOutput>()?.upgrade()
}

/// Buffer constraints for capturing an output.
///
/// `render_node` is the primary renderer's DRM render node (see
/// `Backend::primary_render_node()`), and only shm will be supported without
/// one. It is required since querying it from the EGL context fails for the TTY
/// backend since as of Mesa 26.1.8, _eglGetGbmDisplay clears the display's
/// EGLDevice once a second EGLDisplay is created for the same GBM device, which
/// the TTY backend does during initialization.
pub fn output_capture_constraints(
    renderer: &GlesRenderer,
    render_node: Option<DrmNode>,
    output: &Output,
) -> Option<BufferConstraints> {
    let mode = output.current_mode()?;
    let size = Size::<i32, BufferCoords>::from((mode.size.w, mode.size.h));

    let dma = (|| {
        let node = render_node?;
        let egl = renderer.egl_context();

        // Offer all formats the renderer can draw into to avoid unnecessary conversions.
        let mut by_code: HashMap<Fourcc, Vec<Modifier>> = HashMap::new();
        for format in egl.dmabuf_render_formats().iter() {
            by_code
                .entry(format.code)
                .or_default()
                .push(format.modifier);
        }
        if by_code.is_empty() {
            return None;
        }

        // Xrgb8888 and Argb8888 go first since some clients just take the first
        // advertised format. The rest is sorted so the order is stable across
        // constraint updates for clients that depend on it.
        let mut formats: Vec<_> = by_code.into_iter().collect();
        for (_, modifiers) in &mut formats {
            modifiers.sort_by_key(|modifier| u64::from(*modifier));
        }
        formats.sort_by_key(|(code, _)| {
            let rank = match code {
                Fourcc::Xrgb8888 => 0,
                Fourcc::Argb8888 => 1,
                _ => 2,
            };
            (rank, *code as u32)
        });

        Some(DmabufConstraints { node, formats })
    })();

    Some(BufferConstraints {
        size,
        shm: vec![wl_shm::Format::Xrgb8888],
        dma,
    })
}

/// Buffer constraints for capturing the cursor of an output. Argb8888 since it has alpha.
pub fn cursor_capture_constraints(niri: &Niri, output: &Output) -> BufferConstraints {
    BufferConstraints {
        size: cursor_capture_size(niri, output),
        shm: vec![wl_shm::Format::Argb8888],
        dma: None,
    }
}

/// Size the cursor renders at on this output.
fn cursor_capture_size(niri: &Niri, output: &Output) -> Size<i32, BufferCoords> {
    let int_scale = output.current_scale().integer_scale();
    let scale = Scale::from(output.current_scale().fractional_scale());

    let size: Size<i32, Physical> = match niri.cursor_manager.get_render_cursor(int_scale) {
        RenderCursor::Hidden => Size::from((0, 0)),
        RenderCursor::Surface { surface, .. } => {
            let bbox = smithay::desktop::utils::bbox_from_surface_tree(&surface, (0, 0));
            bbox.to_f64().to_physical_precise_up(scale).size
        }
        RenderCursor::Named {
            scale: buffer_scale,
            cursor,
            ..
        } => {
            let (_idx, frame) = cursor.frame(niri.start_time.elapsed().as_millis() as u32);
            // The image is loaded at the integer scale but drawn at the fractional one, so it
            // ends up smaller than its own buffer whenever the two differ.
            let logical = Size::<f64, Logical>::from((
                f64::from(frame.width) / f64::from(buffer_scale),
                f64::from(frame.height) / f64::from(buffer_scale),
            ));
            logical.to_physical_precise_ceil(scale)
        }
    };

    // Fall back to the nominal cursor size when the cursor is currently hidden or has no
    // buffer, so that the session always has valid constraints.
    if size.is_empty() {
        let fallback = i32::from(niri.config.borrow().cursor.xcursor_size) * int_scale;
        return Size::from((fallback, fallback));
    }

    Size::from((size.w, size.h))
}

/// Cursor hotspot in capture buffer coordinates.
pub fn cursor_capture_hotspot(niri: &Niri, output: &Output) -> Point<i32, BufferCoords> {
    let int_scale = output.current_scale().integer_scale();
    let scale = Scale::from(output.current_scale().fractional_scale());

    let hotspot: Point<i32, Physical> = match niri.cursor_manager.get_render_cursor(int_scale) {
        RenderCursor::Hidden => Point::from((0, 0)),
        RenderCursor::Surface { surface, hotspot } => {
            // The tree is shifted to put its bounding box at the origin in
            // render_cursor_for_capture(), so shift the hotspot too.
            let bbox = smithay::desktop::utils::bbox_from_surface_tree(&surface, (0, 0));
            (hotspot - bbox.loc)
                .to_f64()
                .to_physical_precise_round(scale)
        }
        RenderCursor::Named {
            scale: buffer_scale,
            cursor,
            ..
        } => {
            let (_idx, frame) = cursor.frame(niri.start_time.elapsed().as_millis() as u32);
            // Same rescaling as in cursor_capture_size().
            XCursor::hotspot(frame)
                .to_logical(buffer_scale)
                .to_f64()
                .to_physical_precise_round(scale)
        }
    };

    Point::from((hotspot.x, hotspot.y))
}

pub enum CaptureBuffer {
    Dma(Dmabuf),
    Shm,
}

/// Checks a frame's buffer for compatibility with the render helpers.
///
/// Smithay only validates buffers against the protocol constraints, which allow
/// a client to attach a larger buffer than asked for, but the render helpers
/// need an exact match, and checking them here allows mismatches to be reported
/// to the client.
pub fn capture_buffer(
    buffer: &WlBuffer,
    size: Size<i32, BufferCoords>,
    format: wl_shm::Format,
) -> Option<CaptureBuffer> {
    if let Ok(dmabuf) = get_dmabuf(buffer) {
        let size_matches = dmabuf.width() == size.w as u32 && dmabuf.height() == size.h as u32;
        return size_matches.then(|| CaptureBuffer::Dma(dmabuf.clone()));
    }

    // Stride and pool placement are defined by the client, but the format and
    // dimensions need to match.
    let matches = shm::with_buffer_contents(buffer, |_ptr, _len, data| {
        data.format == format && data.width == size.w && data.height == size.h
    })
    .unwrap_or(false);
    matches.then_some(CaptureBuffer::Shm)
}

impl ImageCaptureSourceHandler for State {}

impl OutputCaptureSourceHandler for State {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.niri.output_capture_source_state
    }

    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}

impl ImageCopyCaptureHandler for State {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.niri.image_copy_capture_state
    }

    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        let output = source_output(source)?;
        if !self.niri.output_state.contains_key(&output) {
            return None;
        }

        let render_node = self.backend.primary_render_node();
        self.backend
            .with_primary_renderer(|renderer| {
                output_capture_constraints(renderer, render_node, &output)
            })
            .flatten()
    }

    fn cursor_capture_constraints(
        &mut self,
        source: &ImageCaptureSource,
        _pointer: &WlPointer,
    ) -> Option<BufferConstraints> {
        let output = source_output(source)?;
        if !self.niri.output_state.contains_key(&output) {
            return None;
        }

        Some(cursor_capture_constraints(&self.niri, &output))
    }

    fn new_session(&mut self, session: Session) {
        // Will be updated with the output properties before capture.
        let damage_tracker = OutputDamageTracker::new((0, 0), 1.0, Transform::Normal);
        self.niri.image_copy_sessions.push(ImageCopySession {
            session,
            damage_tracker,
            pending_frame: None,
        });
    }

    fn new_cursor_session(&mut self, session: CursorSession) {
        // Will be updated with the output properties before capture.
        let damage_tracker = OutputDamageTracker::new((0, 0), 1.0, Transform::Normal);
        self.niri
            .image_copy_cursor_sessions
            .push(ImageCopyCursorSession {
                session,
                damage_tracker,
                pending_frame: None,
            });
        // Send the initial cursor position and hotspot.
        self.niri.refresh_image_copy_cursor_sessions();
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        let Some(s) = self
            .niri
            .image_copy_sessions
            .iter_mut()
            .find(|s| s.session == *session)
        else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };

        // A session may only have one frame object in flight at a time; a
        // second one is a duplicate_frame protocol error. Smithay raises that
        // itself, but still check anyways since to avoid leaking frames if it
        // doesn't.
        if s.pending_frame.is_some() {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        }
        s.pending_frame = Some(frame);

        // The frame is captured on the next redraw with damage.
        if let Some(output) = source_output(&session.source()) {
            // The output may be gone already (the global lingers).
            if self.niri.output_exists(&output) {
                self.niri.queue_redraw(&output);
            }
        }
    }

    fn cursor_frame(&mut self, session: &CursorSessionRef, frame: Frame) {
        let Some(s) = self
            .niri
            .image_copy_cursor_sessions
            .iter_mut()
            .find(|s| s.session == *session)
        else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };

        // Same as above.
        if s.pending_frame.is_some() {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        }
        s.pending_frame = Some(frame);

        // The frame is captured on the next redraw when the cursor image changes.
        if let Some(output) = source_output(&session.source()) {
            // The output may be gone already (the global lingwrs).
            if self.niri.output_exists(&output) {
                self.niri.queue_redraw(&output);
            }
        }
    }

    fn frame_aborted(&mut self, frame: FrameRef) {
        for s in &mut self.niri.image_copy_sessions {
            if s.pending_frame.as_ref().is_some_and(|f| *f == frame) {
                s.pending_frame = None;
            }
        }
        for s in &mut self.niri.image_copy_cursor_sessions {
            if s.pending_frame.as_ref().is_some_and(|f| *f == frame) {
                s.pending_frame = None;
            }
        }
    }

    fn session_destroyed(&mut self, session: SessionRef) {
        self.niri
            .image_copy_sessions
            .retain(|s| s.session != session);
        self.niri.image_copy_capture_state.cleanup();
    }

    fn cursor_session_destroyed(&mut self, session: CursorSessionRef) {
        self.niri
            .image_copy_cursor_sessions
            .retain(|s| s.session != session);
        self.niri.image_copy_capture_state.cleanup();
    }
}
