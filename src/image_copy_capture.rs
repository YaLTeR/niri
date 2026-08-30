//! Frame delivery for the ext-image-copy-capture-v1 protocol.
//!
//! The protocol objects themselves are handled by Smithay; this module keeps track of the
//! active capture sessions and renders the requested frames on output redraw, similarly to
//! wlr-screencopy and the PipeWire screencasts.

use std::collections::HashMap;
use std::mem;
use std::time::Duration;

use smithay::backend::allocator::{Fourcc, Modifier};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{buffer_dimensions, buffer_type, BufferType};
use smithay::output::{Output, OutputModeSource, WeakOutput};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Buffer, IsAlive, Physical, Rectangle, Scale, Size, Transform};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::image_capture_source::ImageCaptureSource;
use smithay::wayland::image_copy_capture::{
    BufferConstraints, CaptureFailureReason, DmabufConstraints, Frame, Session, SessionRef,
};

use crate::niri::{Niri, OutputRenderElements, PointerRenderElements, State};
use crate::niri_render_elements;
use crate::render_helpers::{render_to_dmabuf, render_to_shm, RenderCtx, RenderTarget};
use crate::window::mapped::WindowCastRenderElements;

niri_render_elements! {
    CopyCaptureRenderElement<R> => {
        Output = OutputRenderElements<R>,
        Window = WindowCastRenderElements<R>,
        RelocatedPointer = RelocateRenderElement<PointerRenderElements<R>>,
    }
}

/// An active ext-image-copy-capture session together with niri-side state.
pub struct CopyCaptureSession {
    session: Session,
    damage_tracker: OutputDamageTracker,
    pending_frame: Option<Frame>,
}

impl CopyCaptureSession {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            damage_tracker: OutputDamageTracker::new((0, 0), 1., Transform::Normal),
            pending_frame: None,
        }
    }
}

/// What an image capture source points at.
pub enum CaptureSourceTarget {
    Output(Output),
    Toplevel(WlSurface),
}

/// Resolves a capture source into its target, if the target is still around.
pub fn source_target(niri: &Niri, source: &ImageCaptureSource) -> Option<CaptureSourceTarget> {
    if let Some(weak) = source.user_data().get::<WeakOutput>() {
        return weak
            .upgrade()
            .filter(|output| niri.output_state.contains_key(output))
            .map(CaptureSourceTarget::Output);
    }

    if let Some(surface) = source.user_data().get::<WlSurface>() {
        if surface.alive() {
            return Some(CaptureSourceTarget::Toplevel(surface.clone()));
        }
    }

    None
}

impl State {
    /// Computes the current buffer constraints for a capture source.
    ///
    /// Returns `None` if the source is gone, which rejects the capture.
    pub fn image_capture_constraints(
        &mut self,
        source: &ImageCaptureSource,
    ) -> Option<BufferConstraints> {
        let size = self.image_capture_source_size(source)?;
        Some(self.build_image_capture_constraints(size))
    }

    /// Computes the current buffer size for a capture source.
    fn image_capture_source_size(&self, source: &ImageCaptureSource) -> Option<Size<i32, Buffer>> {
        let size = match source_target(&self.niri, source)? {
            CaptureSourceTarget::Output(output) => {
                let mode = output.current_mode()?;
                output.current_transform().transform_size(mode.size)
            }
            CaptureSourceTarget::Toplevel(surface) => {
                let (mapped, output) = self.niri.layout.find_window_and_output(&surface)?;
                let scale = output
                    .map(|output| Scale::from(output.current_scale().fractional_scale()))
                    .unwrap_or(Scale::from(1.));
                mapped
                    .window
                    .bbox_with_popups()
                    .to_physical_precise_up(scale)
                    .size
            }
        };

        if size.is_empty() {
            return None;
        }

        Some(size.to_logical(1).to_buffer(1, Transform::Normal))
    }

    fn build_image_capture_constraints(&mut self, size: Size<i32, Buffer>) -> BufferConstraints {
        let dma = self.backend.primary_render_node().and_then(|node| {
            self.backend.with_primary_renderer(|renderer| {
                let mut formats: HashMap<Fourcc, Vec<Modifier>> = HashMap::new();
                for format in renderer.egl_context().dmabuf_render_formats().iter() {
                    formats
                        .entry(format.code)
                        .or_default()
                        .push(format.modifier);
                }
                DmabufConstraints {
                    node,
                    formats: formats.into_iter().collect(),
                }
            })
        });

        BufferConstraints {
            size,
            // render_to_shm() only supports Xrgb8888.
            shm: vec![wl_shm::Format::Xrgb8888],
            dma,
        }
    }

    /// Adds a new capture session.
    pub fn new_image_copy_capture_session(&mut self, session: Session) {
        self.niri
            .copy_capture_sessions
            .push(CopyCaptureSession::new(session));
    }

    /// Queues a capture frame for delivery on the next redraw with damage.
    pub fn image_copy_capture_frame_requested(&mut self, session: &SessionRef, frame: Frame) {
        let Some(entry) = self
            .niri
            .copy_capture_sessions
            .iter_mut()
            .find(|entry| entry.session == *session)
        else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };

        if entry.pending_frame.is_some() {
            // Only one frame can be captured at a time.
            frame.fail(CaptureFailureReason::Unknown);
            return;
        }

        entry.pending_frame = Some(frame);

        // Queue a redraw of the source's output so the frame gets delivered even when nothing
        // else causes a redraw. If the source didn't change since the last frame, delivery will
        // wait until it does.
        let output = match source_target(&self.niri, &session.source()) {
            Some(CaptureSourceTarget::Output(output)) => Some(output),
            Some(CaptureSourceTarget::Toplevel(surface)) => self
                .niri
                .layout
                .find_window_and_output(&surface)
                .and_then(|(_, output)| output.cloned()),
            // The source is gone; the session will be stopped in the next refresh, failing the
            // frame.
            None => None,
        };

        if let Some(output) = output {
            if self.niri.output_state.contains_key(&output) {
                self.niri.queue_redraw(&output);
            }
        }
    }

    /// Drops the queued frame that the client aborted.
    pub fn image_copy_capture_frame_aborted(
        &mut self,
        frame: &smithay::wayland::image_copy_capture::FrameRef,
    ) {
        for entry in &mut self.niri.copy_capture_sessions {
            if entry.pending_frame.as_ref().is_some_and(|f| **f == *frame) {
                entry.pending_frame = None;
            }
        }
    }

    /// Removes the session that the client destroyed.
    pub fn image_copy_capture_session_destroyed(&mut self, session: &SessionRef) {
        self.niri
            .copy_capture_sessions
            .retain(|entry| entry.session != *session);
    }

    /// Updates capture session constraints and stops sessions whose source is gone.
    pub fn refresh_image_copy_capture(&mut self) {
        let _span = tracy_client::span!("State::refresh_image_copy_capture");

        let mut sessions = mem::take(&mut self.niri.copy_capture_sessions);
        sessions.retain_mut(|entry| {
            if !entry.session.alive() {
                return false;
            }

            let source = entry.session.source();
            let Some(size) = self.image_capture_source_size(&source) else {
                // The source is gone; dropping the session stops it and fails all pending
                // frames.
                return false;
            };

            if entry.session.current_constraints().map(|c| c.size) != Some(size) {
                let constraints = self.build_image_capture_constraints(size);
                entry.session.update_constraints(constraints);

                // The pending frame's buffer no longer matches; fail it so the client can
                // reallocate.
                if let Some(frame) = entry.pending_frame.take() {
                    frame.fail(CaptureFailureReason::BufferConstraints);
                }
            }

            true
        });
        self.niri.copy_capture_sessions = sessions;
    }
}

impl Niri {
    /// Renders and delivers queued capture frames for sources on this output.
    ///
    /// Called after a redraw, so the render elements are up to date. Frames are only delivered
    /// when their source accumulated damage since the last delivered frame; a fresh session's
    /// damage tracker reports full damage, so the first frame is delivered right away.
    pub fn render_for_image_copy_capture(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
        target_presentation_time: Duration,
    ) {
        let _span = tracy_client::span!("Niri::render_for_image_copy_capture");

        let mut sessions = mem::take(&mut self.copy_capture_sessions);
        for entry in &mut sessions {
            if entry.pending_frame.is_none() {
                continue;
            }

            match source_target(self, &entry.session.source()) {
                Some(CaptureSourceTarget::Output(o)) if o == *output => {
                    self.render_output_capture_session(
                        entry,
                        renderer,
                        output,
                        target_presentation_time,
                    );
                }
                Some(CaptureSourceTarget::Toplevel(surface)) => {
                    self.render_toplevel_capture_session(
                        entry,
                        renderer,
                        output,
                        &surface,
                        target_presentation_time,
                    );
                }
                _ => (),
            }
        }
        self.copy_capture_sessions = sessions;
    }

    fn render_output_capture_session(
        &self,
        entry: &mut CopyCaptureSession,
        renderer: &mut GlesRenderer,
        output: &Output,
        presentation_time: Duration,
    ) {
        let Some(mode) = output.current_mode() else {
            return;
        };
        let transform = output.current_transform();
        let size = transform.transform_size(mode.size);
        let scale = Scale::from(output.current_scale().fractional_scale());

        ensure_damage_tracker(&mut entry.damage_tracker, size, scale);

        let mut elements: Vec<CopyCaptureRenderElement<GlesRenderer>> = Vec::new();
        let ctx = RenderCtx {
            renderer: &mut *renderer,
            target: RenderTarget::ScreenCapture,
            xray: None,
        };
        self.render(ctx, output, entry.session.draw_cursor(), &mut |elem| {
            elements.push(CopyCaptureRenderElement::from(elem));
        });

        deliver_frame(
            entry,
            renderer,
            &elements,
            size,
            presentation_time,
            &self.event_loop,
        );
    }

    fn render_toplevel_capture_session(
        &self,
        entry: &mut CopyCaptureSession,
        renderer: &mut GlesRenderer,
        output: &Output,
        surface: &WlSurface,
        presentation_time: Duration,
    ) {
        let Some((mapped, mapped_output)) = self.layout.find_window_and_output(surface) else {
            return;
        };
        if mapped_output != Some(output) {
            return;
        }

        let scale = Scale::from(output.current_scale().fractional_scale());
        let bbox = mapped
            .window
            .bbox_with_popups()
            .to_physical_precise_up(scale);
        let size = bbox.size;

        ensure_damage_tracker(&mut entry.damage_tracker, size, scale);

        let mut elements: Vec<CopyCaptureRenderElement<GlesRenderer>> = Vec::new();

        if entry.session.draw_cursor() {
            if let Some((_, win_pos)) = self.pointer_pos_for_window_cast(mapped) {
                // See render_windows_for_screen_cast() for the coordinate space logic.
                let buf_pos = win_pos + bbox.loc.to_f64().to_logical(scale);
                let pos = buf_pos.to_physical_precise_round(scale).upscale(-1);
                self.render_pointer(renderer, output, &mut |elem| {
                    let elem = RelocateRenderElement::from_element(elem, pos, Relocate::Relative);
                    elements.push(CopyCaptureRenderElement::from(elem));
                });
            }
        }

        mapped.render_for_screen_cast(renderer, scale, &mut |elem| {
            elements.push(CopyCaptureRenderElement::from(elem));
        });

        deliver_frame(
            entry,
            renderer,
            &elements,
            size,
            presentation_time,
            &self.event_loop,
        );
    }
}

fn ensure_damage_tracker(
    damage_tracker: &mut OutputDamageTracker,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
) {
    let OutputModeSource::Static {
        size: last_size,
        scale: last_scale,
        transform: last_transform,
    } = damage_tracker.mode().clone()
    else {
        unreachable!("damage tracker must have static mode");
    };

    if size != last_size || scale != last_scale || last_transform != Transform::Normal {
        *damage_tracker = OutputDamageTracker::new(size, scale, Transform::Normal);
    }
}

fn deliver_frame(
    entry: &mut CopyCaptureSession,
    renderer: &mut GlesRenderer,
    elements: &[CopyCaptureRenderElement<GlesRenderer>],
    size: Size<i32, Physical>,
    presentation_time: Duration,
    event_loop: &LoopHandle<'static, State>,
) {
    let (damage, states) = match entry.damage_tracker.damage_output(1, elements) {
        Ok(x) => x,
        Err(err) => {
            warn!("error computing damage for image copy capture: {err:?}");
            return;
        }
    };

    // No damage since the last delivered frame; wait for the next redraw.
    let Some(damage) = damage else {
        return;
    };

    // The buffer has Transform::Normal contents, so the conversion is 1:1.
    let buffer_damage: Vec<Rectangle<i32, Buffer>> = damage
        .iter()
        .map(|dmg| {
            dmg.to_logical(1)
                .to_buffer(1, Transform::Normal, &size.to_logical(1))
        })
        .collect();

    let frame = entry.pending_frame.take().unwrap();
    let buffer = frame.buffer();

    // The size might have changed since the buffer passed Smithay's constraint check.
    if buffer_dimensions(&buffer) != Some(size.to_logical(1).to_buffer(1, Transform::Normal)) {
        frame.fail(CaptureFailureReason::BufferConstraints);
        // Recreate the damage tracker to report full damage next time.
        entry.damage_tracker = OutputDamageTracker::new((0, 0), 1., Transform::Normal);
        return;
    }

    let res = match buffer_type(&buffer) {
        Some(BufferType::Shm) => render_to_shm(
            renderer,
            &mut entry.damage_tracker,
            &buffer,
            elements,
            states,
        )
        .map(|()| None),
        Some(BufferType::Dma) => match get_dmabuf(&buffer) {
            Ok(dmabuf) => render_to_dmabuf(
                renderer,
                &mut entry.damage_tracker,
                dmabuf.clone(),
                elements,
                states,
            )
            .map(Some),
            Err(err) => Err(anyhow::anyhow!("error getting dmabuf from buffer: {err:?}")),
        },
        _ => Err(anyhow::anyhow!("unsupported buffer type")),
    };

    match res {
        Ok(sync) => success_after_sync(frame, buffer_damage, presentation_time, sync, event_loop),
        Err(err) => {
            warn!("error rendering for image copy capture: {err:?}");
            // Recreate the damage tracker to report full damage next time.
            entry.damage_tracker = OutputDamageTracker::new((0, 0), 1., Transform::Normal);
            frame.fail(CaptureFailureReason::Unknown);
        }
    }
}

/// Signals a successful capture, delaying it until the GPU is done rendering if necessary.
fn success_after_sync(
    frame: Frame,
    damage: Vec<Rectangle<i32, Buffer>>,
    presentation_time: Duration,
    sync: Option<SyncPoint>,
    event_loop: &LoopHandle<'static, State>,
) {
    match sync.and_then(|sync| sync.export()) {
        None => frame.success(Transform::Normal, damage, presentation_time),
        Some(sync_fd) => {
            let source = Generic::new(sync_fd, Interest::READ, Mode::OneShot);
            let mut frame = Some(frame);
            let mut damage = Some(damage);
            event_loop
                .insert_source(source, move |_, _, _| {
                    frame.take().unwrap().success(
                        Transform::Normal,
                        damage.take().unwrap(),
                        presentation_time,
                    );
                    Ok(PostAction::Remove)
                })
                .unwrap();
        }
    }
}
