use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::Cursor;
use std::iter::zip;
use std::mem;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::rc::Rc;
use std::time::Duration;

use anyhow::Context as _;
use calloop::timer::{TimeoutAction, Timer};
use calloop::RegistrationToken;
use pipewire::context::Context;
use pipewire::core::Core;
use pipewire::main_loop::MainLoop;
use pipewire::properties::Properties;
use pipewire::spa::buffer::DataType;
use pipewire::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pipewire::spa::param::format_utils::parse_format;
use pipewire::spa::param::video::{VideoFormat, VideoInfoRaw};
use pipewire::spa::param::ParamType;
use pipewire::spa::pod::deserialize::PodDeserializer;
use pipewire::spa::pod::serialize::PodSerializer;
use pipewire::spa::pod::{self, ChoiceValue, Pod, PodPropFlags, Property, PropertyFlags};
use pipewire::spa::sys::*;
use pipewire::spa::utils::{
    Choice, ChoiceEnum, ChoiceFlags, Direction, Fraction, Rectangle, SpaTypes,
};
use pipewire::spa::{self};
use pipewire::stream::{Stream, StreamFlags, StreamListener, StreamState};
use smithay::backend::allocator::dmabuf::{AsDmabuf, Dmabuf};
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::gbm::{GbmBuffer, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::{Format, Fourcc};
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::output::{Output, OutputModeSource, WeakOutput};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::gbm::Modifier;
use smithay::utils::{Physical, Scale, Size, Transform};
use zbus::SignalContext;

use crate::dbus::mutter_screen_cast::{self, CursorMode};
use crate::niri::State;
use crate::render_helpers::render_to_dmabuf;
use crate::utils::get_monotonic_time;

// Give a 0.1 ms allowance for presentation time errors.
const CAST_DELAY_ALLOWANCE: Duration = Duration::from_micros(100);

pub struct PipeWire {
    _context: Context,
    pub core: Core,
    to_niri: calloop::channel::Sender<PwToNiri>,
}

pub enum PwToNiri {
    StopCast { session_id: usize },
    Redraw(CastTarget),
}

pub struct Cast {
    pub session_id: usize,
    pub stream: Stream,
    _listener: StreamListener<()>,
    pub is_active: Rc<Cell<bool>>,
    pub target: CastTarget,
    formats: FormatSet,
    state: Rc<RefCell<CastState>>,
    refresh: Rc<Cell<u32>>,
    offer_alpha: bool,
    pub cursor_mode: CursorMode,
    pub last_frame_time: Duration,
    min_time_between_frames: Rc<Cell<Duration>>,
    dmabufs: Rc<RefCell<HashMap<i64, Dmabuf>>>,
    scheduled_redraw: Option<RegistrationToken>,
}

#[derive(Debug)]
pub enum CastState {
    ResizePending {
        pending_size: Size<u32, Physical>,
    },
    ConfirmationPending {
        size: Size<u32, Physical>,
        alpha: bool,
        modifier: Modifier,
        plane_count: i32,
    },
    Ready {
        size: Size<u32, Physical>,
        alpha: bool,
        modifier: Modifier,
        plane_count: i32,
        // Lazily-initialized to keep the initialization to a single place.
        damage_tracker: Option<OutputDamageTracker>,
    },
}

#[derive(PartialEq, Eq)]
pub enum CastSizeChange {
    Ready,
    Pending,
}

#[derive(Clone, PartialEq, Eq)]
pub enum CastTarget {
    Output(WeakOutput),
    Window { id: u64 },
}

macro_rules! make_params {
    ($params:ident, $formats:expr, $size:expr, $refresh:expr, $alpha:expr) => {
        let mut b1 = Vec::new();
        let mut b2 = Vec::new();

        let o1 = make_video_params($formats, $size, $refresh, false);
        let pod1 = make_pod(&mut b1, o1);

        let mut p1;
        let mut p2;
        $params = if $alpha {
            let o2 = make_video_params($formats, $size, $refresh, true);
            p2 = [pod1, make_pod(&mut b2, o2)];
            &mut p2[..]
        } else {
            p1 = [pod1];
            &mut p1[..]
        };
    };
}

impl PipeWire {
    pub fn new(event_loop: &LoopHandle<'static, State>) -> anyhow::Result<Self> {
        let main_loop = MainLoop::new(None).context("error creating MainLoop")?;
        let context = Context::new(&main_loop).context("error creating Context")?;
        let core = context.connect(None).context("error creating Core")?;

        let listener = core
            .add_listener_local()
            .error(|id, seq, res, message| {
                warn!(id, seq, res, message, "pw error");
            })
            .register();
        mem::forget(listener);

        struct AsFdWrapper(MainLoop);
        impl AsFd for AsFdWrapper {
            fn as_fd(&self) -> BorrowedFd<'_> {
                self.0.loop_().fd()
            }
        }
        let generic = Generic::new(AsFdWrapper(main_loop), Interest::READ, Mode::Level);
        event_loop
            .insert_source(generic, move |_, wrapper, _| {
                let _span = tracy_client::span!("pipewire iteration");
                wrapper.0.loop_().iterate(Duration::ZERO);
                Ok(PostAction::Continue)
            })
            .unwrap();

        let (to_niri, from_pipewire) = calloop::channel::channel();
        event_loop
            .insert_source(from_pipewire, move |event, _, state| match event {
                calloop::channel::Event::Msg(msg) => state.on_pw_msg(msg),
                calloop::channel::Event::Closed => (),
            })
            .unwrap();

        Ok(Self {
            _context: context,
            core,
            to_niri,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_cast(
        &self,
        gbm: GbmDevice<DrmDeviceFd>,
        formats: FormatSet,
        session_id: usize,
        target: CastTarget,
        size: Size<i32, Physical>,
        refresh: u32,
        alpha: bool,
        cursor_mode: CursorMode,
        signal_ctx: SignalContext<'static>,
    ) -> anyhow::Result<Cast> {
        let _span = tracy_client::span!("PipeWire::start_cast");

        let to_niri_ = self.to_niri.clone();
        let stop_cast = move || {
            if let Err(err) = to_niri_.send(PwToNiri::StopCast { session_id }) {
                warn!("error sending StopCast to niri: {err:?}");
            }
        };
        let target_ = target.clone();
        let to_niri_ = self.to_niri.clone();
        let redraw = move || {
            if let Err(err) = to_niri_.send(PwToNiri::Redraw(target_.clone())) {
                warn!("error sending Redraw to niri: {err:?}");
            }
        };
        let redraw_ = redraw.clone();

        let stream = Stream::new(&self.core, "niri-screen-cast-src", Properties::new())
            .context("error creating Stream")?;

        // Like in good old wayland-rs times...
        let node_id = Rc::new(Cell::new(None));
        let is_active = Rc::new(Cell::new(false));
        let min_time_between_frames = Rc::new(Cell::new(Duration::ZERO));
        let dmabufs = Rc::new(RefCell::new(HashMap::new()));
        let refresh = Rc::new(Cell::new(refresh));

        let pending_size = Size::from((size.w as u32, size.h as u32));
        let state = Rc::new(RefCell::new(CastState::ResizePending { pending_size }));

        let listener = stream
            .add_local_listener_with_user_data(())
            .state_changed({
                let is_active = is_active.clone();
                let stop_cast = stop_cast.clone();
                move |stream, (), old, new| {
                    debug!("pw stream: state changed: {old:?} -> {new:?}");

                    match new {
                        StreamState::Paused => {
                            if node_id.get().is_none() {
                                let id = stream.node_id();
                                node_id.set(Some(id));
                                debug!("pw stream: sending signal with {id}");

                                let _span = tracy_client::span!("sending PipeWireStreamAdded");
                                async_io::block_on(async {
                                    let res = mutter_screen_cast::Stream::pipe_wire_stream_added(
                                        &signal_ctx,
                                        id,
                                    )
                                    .await;

                                    if let Err(err) = res {
                                        warn!("error sending PipeWireStreamAdded: {err:?}");
                                        stop_cast();
                                    }
                                });
                            }

                            is_active.set(false);
                        }
                        StreamState::Error(_) => {
                            if is_active.get() {
                                is_active.set(false);
                                stop_cast();
                            }
                        }
                        StreamState::Unconnected => (),
                        StreamState::Connecting => (),
                        StreamState::Streaming => {
                            is_active.set(true);
                            redraw();
                        }
                    }
                }
            })
            .param_changed({
                let min_time_between_frames = min_time_between_frames.clone();
                let stop_cast = stop_cast.clone();
                let state = state.clone();
                let gbm = gbm.clone();
                let formats = formats.clone();
                let refresh = refresh.clone();
                move |stream, (), id, pod| {
                    let id = ParamType::from_raw(id);
                    trace!(?id, "pw stream: param_changed");

                    if id != ParamType::Format {
                        return;
                    }

                    let Some(pod) = pod else { return };

                    let (m_type, m_subtype) = match parse_format(pod) {
                        Ok(x) => x,
                        Err(err) => {
                            warn!("pw stream: error parsing format: {err:?}");
                            return;
                        }
                    };

                    if m_type != MediaType::Video || m_subtype != MediaSubtype::Raw {
                        return;
                    }

                    let mut format = VideoInfoRaw::new();
                    format.parse(pod).unwrap();
                    debug!("pw stream: got format = {format:?}");

                    let format_size = Size::from((format.size().width, format.size().height));

                    let mut state = state.borrow_mut();
                    if format_size != state.expected_format_size() {
                        if !matches!(&*state, CastState::ResizePending { .. }) {
                            warn!("pw stream: wrong size, but we're not resizing");
                            stop_cast();
                            return;
                        }

                        debug!("pw stream: wrong size, waiting");
                        return;
                    }

                    let format_has_alpha = format.format() == VideoFormat::BGRA;
                    let fourcc = if format_has_alpha {
                        Fourcc::Argb8888
                    } else {
                        Fourcc::Xrgb8888
                    };

                    let max_frame_rate = format.max_framerate();
                    let min_frame_time = Duration::from_micros(
                        1_000_000 * u64::from(max_frame_rate.denom) / u64::from(max_frame_rate.num),
                    );
                    min_time_between_frames.set(min_frame_time);

                    let object = pod.as_object().unwrap();
                    let Some(prop_modifier) =
                        object.find_prop(spa::utils::Id(FormatProperties::VideoModifier.0))
                    else {
                        warn!("pw stream: modifier prop missing");
                        stop_cast();
                        return;
                    };

                    if prop_modifier.flags().contains(PodPropFlags::DONT_FIXATE) {
                        debug!("pw stream: fixating the modifier");

                        let pod_modifier = prop_modifier.value();
                        let Ok((_, modifiers)) = PodDeserializer::deserialize_from::<Choice<i64>>(
                            pod_modifier.as_bytes(),
                        ) else {
                            warn!("pw stream: wrong modifier property type");
                            stop_cast();
                            return;
                        };

                        let ChoiceEnum::Enum { alternatives, .. } = modifiers.1 else {
                            warn!("pw stream: wrong modifier choice type");
                            stop_cast();
                            return;
                        };

                        let (modifier, plane_count) = match find_preferred_modifier(
                            &gbm,
                            format_size,
                            fourcc,
                            alternatives,
                        ) {
                            Ok(x) => x,
                            Err(err) => {
                                warn!("pw stream: couldn't find preferred modifier: {err:?}");
                                stop_cast();
                                return;
                            }
                        };

                        debug!(
                            "pw stream: allocation successful \
                             (modifier={modifier:?}, plane_count={plane_count}), \
                             moving to confirmation pending"
                        );

                        *state = CastState::ConfirmationPending {
                            size: format_size,
                            alpha: format_has_alpha,
                            modifier,
                            plane_count: plane_count as i32,
                        };

                        let fixated_format = FormatSet::from_iter([Format {
                            code: fourcc,
                            modifier,
                        }]);

                        let mut b1 = Vec::new();
                        let mut b2 = Vec::new();

                        let o1 = make_video_params(
                            &fixated_format,
                            format_size,
                            refresh.get(),
                            format_has_alpha,
                        );
                        let pod1 = make_pod(&mut b1, o1);

                        let o2 = make_video_params(
                            &formats,
                            format_size,
                            refresh.get(),
                            format_has_alpha,
                        );
                        let mut params = [pod1, make_pod(&mut b2, o2)];

                        if let Err(err) = stream.update_params(&mut params) {
                            warn!("error updating stream params: {err:?}");
                            stop_cast();
                        }

                        return;
                    }

                    // Verify that alpha and modifier didn't change.
                    let plane_count = match &*state {
                        CastState::ConfirmationPending {
                            size,
                            alpha,
                            modifier,
                            plane_count,
                        }
                        | CastState::Ready {
                            size,
                            alpha,
                            modifier,
                            plane_count,
                            ..
                        } if *alpha == format_has_alpha
                            && *modifier == Modifier::from(format.modifier()) =>
                        {
                            let size = *size;
                            let alpha = *alpha;
                            let modifier = *modifier;
                            let plane_count = *plane_count;

                            let damage_tracker =
                                if let CastState::Ready { damage_tracker, .. } = &mut *state {
                                    damage_tracker.take()
                                } else {
                                    None
                                };

                            debug!("pw stream: moving to ready state");

                            *state = CastState::Ready {
                                size,
                                alpha,
                                modifier,
                                plane_count,
                                damage_tracker,
                            };

                            plane_count
                        }
                        _ => {
                            // We're negotiating a single modifier, or alpha or modifier changed,
                            // so we need to do a test allocation.
                            let (modifier, plane_count) = match find_preferred_modifier(
                                &gbm,
                                format_size,
                                fourcc,
                                vec![format.modifier() as i64],
                            ) {
                                Ok(x) => x,
                                Err(err) => {
                                    warn!("pw stream: test allocation failed: {err:?}");
                                    stop_cast();
                                    return;
                                }
                            };

                            debug!(
                                "pw stream: allocation successful \
                                 (modifier={modifier:?}, plane_count={plane_count}), \
                                 moving to ready"
                            );

                            *state = CastState::Ready {
                                size: format_size,
                                alpha: format_has_alpha,
                                modifier,
                                plane_count: plane_count as i32,
                                damage_tracker: None,
                            };

                            plane_count as i32
                        }
                    };

                    // const BPP: u32 = 4;
                    // let stride = format.size().width * BPP;
                    // let size = stride * format.size().height;

                    let o1 = pod::object!(
                        SpaTypes::ObjectParamBuffers,
                        ParamType::Buffers,
                        Property::new(
                            SPA_PARAM_BUFFERS_buffers,
                            pod::Value::Choice(ChoiceValue::Int(Choice(
                                ChoiceFlags::empty(),
                                ChoiceEnum::Range {
                                    default: 16,
                                    min: 2,
                                    max: 16
                                }
                            ))),
                        ),
                        Property::new(SPA_PARAM_BUFFERS_blocks, pod::Value::Int(plane_count)),
                        // Property::new(SPA_PARAM_BUFFERS_size, pod::Value::Int(size as i32)),
                        // Property::new(SPA_PARAM_BUFFERS_stride, pod::Value::Int(stride as i32)),
                        // Property::new(SPA_PARAM_BUFFERS_align, pod::Value::Int(16)),
                        Property::new(
                            SPA_PARAM_BUFFERS_dataType,
                            pod::Value::Choice(ChoiceValue::Int(Choice(
                                ChoiceFlags::empty(),
                                ChoiceEnum::Flags {
                                    default: 1 << DataType::DmaBuf.as_raw(),
                                    flags: vec![1 << DataType::DmaBuf.as_raw()],
                                },
                            ))),
                        ),
                    );

                    // FIXME: Hidden / embedded / metadata cursor

                    // let o2 = pod::object!(
                    //     SpaTypes::ObjectParamMeta,
                    //     ParamType::Meta,
                    //     Property::new(SPA_PARAM_META_type,
                    // pod::Value::Id(Id(SPA_META_Header))),
                    //     Property::new(
                    //         SPA_PARAM_META_size,
                    //         pod::Value::Int(size_of::<spa_meta_header>() as i32)
                    //     ),
                    // );
                    let mut b1 = vec![];
                    // let mut b2 = vec![];
                    let mut params = [
                        make_pod(&mut b1, o1), // make_pod(&mut b2, o2)
                    ];

                    if let Err(err) = stream.update_params(&mut params) {
                        warn!("error updating stream params: {err:?}");
                        stop_cast();
                    }
                }
            })
            .add_buffer({
                let dmabufs = dmabufs.clone();
                let stop_cast = stop_cast.clone();
                let state = state.clone();
                move |stream, (), buffer| {
                    let (size, alpha, modifier) = if let CastState::Ready {
                        size,
                        alpha,
                        modifier,
                        ..
                    } = &*state.borrow()
                    {
                        (*size, *alpha, *modifier)
                    } else {
                        trace!("pw stream: add buffer, but not ready yet");
                        return;
                    };

                    trace!(
                        "pw stream: add_buffer, size={size:?}, alpha={alpha}, \
                         modifier={modifier:?}"
                    );

                    unsafe {
                        let spa_buffer = (*buffer).buffer;

                        let fourcc = if alpha {
                            Fourcc::Argb8888
                        } else {
                            Fourcc::Xrgb8888
                        };

                        let dmabuf = match allocate_dmabuf(&gbm, size, fourcc, modifier) {
                            Ok(dmabuf) => dmabuf,
                            Err(err) => {
                                warn!("error allocating dmabuf: {err:?}");
                                stop_cast();
                                return;
                            }
                        };

                        let plane_count = dmabuf.num_planes();
                        assert_eq!((*spa_buffer).n_datas as usize, plane_count);

                        for (i, fd) in dmabuf.handles().enumerate() {
                            let spa_data = (*spa_buffer).datas.add(i);
                            assert!((*spa_data).type_ & (1 << DataType::DmaBuf.as_raw()) > 0);

                            (*spa_data).type_ = DataType::DmaBuf.as_raw();
                            (*spa_data).maxsize = 1;
                            (*spa_data).fd = fd.as_raw_fd() as i64;
                            (*spa_data).flags = SPA_DATA_FLAG_READWRITE;
                        }

                        let fd = (*(*spa_buffer).datas).fd;
                        assert!(dmabufs.borrow_mut().insert(fd, dmabuf).is_none());
                    }

                    // During size re-negotiation, the stream sometimes just keeps running, in
                    // which case we may need to force a redraw once we got a newly sized buffer.
                    if dmabufs.borrow().len() == 1 && stream.state() == StreamState::Streaming {
                        redraw_();
                    }
                }
            })
            .remove_buffer({
                let dmabufs = dmabufs.clone();
                move |_stream, (), buffer| {
                    trace!("pw stream: remove_buffer");

                    unsafe {
                        let spa_buffer = (*buffer).buffer;
                        let spa_data = (*spa_buffer).datas;
                        assert!((*spa_buffer).n_datas > 0);

                        let fd = (*spa_data).fd;
                        dmabufs.borrow_mut().remove(&fd);
                    }
                }
            })
            .register()
            .unwrap();

        trace!("starting pw stream with size={pending_size:?}, refresh={refresh:?}");

        let params;
        make_params!(params, &formats, pending_size, refresh.get(), alpha);
        stream
            .connect(
                Direction::Output,
                None,
                StreamFlags::DRIVER | StreamFlags::ALLOC_BUFFERS,
                params,
            )
            .context("error connecting stream")?;

        let cast = Cast {
            session_id,
            stream,
            _listener: listener,
            is_active,
            target,
            formats,
            state,
            refresh,
            offer_alpha: alpha,
            cursor_mode,
            last_frame_time: Duration::ZERO,
            min_time_between_frames,
            dmabufs,
            scheduled_redraw: None,
        };
        Ok(cast)
    }
}

impl Cast {
    pub fn ensure_size(&self, size: Size<i32, Physical>) -> anyhow::Result<CastSizeChange> {
        let new_size = Size::from((size.w as u32, size.h as u32));

        let mut state = self.state.borrow_mut();
        if matches!(&*state, CastState::Ready { size, .. } if *size == new_size) {
            return Ok(CastSizeChange::Ready);
        }

        if state.pending_size() == Some(new_size) {
            debug!("stream size still hasn't changed, skipping frame");
            return Ok(CastSizeChange::Pending);
        }

        let _span = tracy_client::span!("Cast::ensure_size");
        debug!("cast size changed, updating stream size");

        *state = CastState::ResizePending {
            pending_size: new_size,
        };

        let params;
        make_params!(
            params,
            &self.formats,
            new_size,
            self.refresh.get(),
            self.offer_alpha
        );
        self.stream
            .update_params(params)
            .context("error updating stream params")?;

        Ok(CastSizeChange::Pending)
    }

    pub fn set_refresh(&mut self, refresh: u32) -> anyhow::Result<()> {
        if self.refresh.get() == refresh {
            return Ok(());
        }

        let _span = tracy_client::span!("Cast::set_refresh");
        debug!("cast FPS changed, updating stream FPS");
        self.refresh.set(refresh);

        let size = self.state.borrow().expected_format_size();
        let params;
        make_params!(params, &self.formats, size, refresh, self.offer_alpha);
        self.stream
            .update_params(params)
            .context("error updating stream params")?;

        Ok(())
    }

    fn compute_extra_delay(&self, target_frame_time: Duration) -> Duration {
        let last = self.last_frame_time;
        let min = self.min_time_between_frames.get();

        if last.is_zero() {
            trace!(?target_frame_time, ?last, "last is zero, recording");
            return Duration::ZERO;
        }

        if target_frame_time < last {
            // Record frame with a warning; in case it was an overflow this will fix it.
            warn!(
                ?target_frame_time,
                ?last,
                "target frame time is below last, did it overflow or did we mispredict?"
            );
            return Duration::ZERO;
        }

        let diff = target_frame_time - last;
        if diff < min {
            let delay = min - diff;
            trace!(
                ?target_frame_time,
                ?last,
                "frame is too soon: min={min:?}, delay={:?}",
                delay
            );
            return delay;
        } else {
            trace!("overshoot={:?}", diff - min);
        }

        Duration::ZERO
    }

    fn schedule_redraw(
        &mut self,
        event_loop: &LoopHandle<'static, State>,
        output: Output,
        target_time: Duration,
    ) {
        if self.scheduled_redraw.is_some() {
            return;
        }

        let now = get_monotonic_time();
        let duration = target_time.saturating_sub(now);
        let timer = Timer::from_duration(duration);
        let token = event_loop
            .insert_source(timer, move |_, _, state| {
                // Guard against output disconnecting before the timer has a chance to run.
                if state.niri.output_state.contains_key(&output) {
                    state.niri.queue_redraw(&output);
                }

                TimeoutAction::Drop
            })
            .unwrap();
        self.scheduled_redraw = Some(token);
    }

    fn remove_scheduled_redraw(&mut self, event_loop: &LoopHandle<'static, State>) {
        if let Some(token) = self.scheduled_redraw.take() {
            event_loop.remove(token);
        }
    }

    /// Checks whether this frame should be skipped because it's too soon.
    ///
    /// If the frame should be skipped, schedules a redraw and returns `true`. Otherwise, removes a
    /// scheduled redraw, if any, and returns `false`.
    ///
    /// When this method returns `false`, the calling code is assumed to follow up with
    /// [`Cast::dequeue_buffer_and_render()`].
    pub fn check_time_and_schedule(
        &mut self,
        event_loop: &LoopHandle<'static, State>,
        output: &Output,
        target_frame_time: Duration,
    ) -> bool {
        let delay = self.compute_extra_delay(target_frame_time);
        if delay >= CAST_DELAY_ALLOWANCE {
            trace!("delay >= allowance, scheduling redraw");
            self.schedule_redraw(event_loop, output.clone(), target_frame_time + delay);
            true
        } else {
            self.remove_scheduled_redraw(event_loop);
            false
        }
    }

    pub fn dequeue_buffer_and_render(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &[impl RenderElement<GlesRenderer>],
        size: Size<i32, Physical>,
        scale: Scale<f64>,
    ) -> bool {
        let CastState::Ready { damage_tracker, .. } = &mut *self.state.borrow_mut() else {
            error!("cast must be in Ready state to render");
            return false;
        };
        let damage_tracker = damage_tracker
            .get_or_insert_with(|| OutputDamageTracker::new(size, scale, Transform::Normal));

        // Size change will drop the damage tracker, but scale change won't, so check it here.
        let OutputModeSource::Static { scale: t_scale, .. } = damage_tracker.mode() else {
            unreachable!();
        };
        if *t_scale != scale {
            *damage_tracker = OutputDamageTracker::new(size, scale, Transform::Normal);
        }

        let (damage, _states) = damage_tracker.damage_output(1, elements).unwrap();
        if damage.is_none() {
            trace!("no damage, skipping frame");
            return false;
        }

        let mut buffer = match self.stream.dequeue_buffer() {
            Some(buffer) => buffer,
            None => {
                warn!("no available buffer in pw stream, skipping frame");
                return false;
            }
        };

        let fd = buffer.datas_mut()[0].as_raw().fd;
        let dmabuf = &self.dmabufs.borrow()[&fd];

        if let Err(err) = render_to_dmabuf(
            renderer,
            dmabuf.clone(),
            size,
            scale,
            Transform::Normal,
            elements.iter().rev(),
        ) {
            warn!("error rendering to dmabuf: {err:?}");
            return false;
        }

        for (data, (stride, offset)) in
            zip(buffer.datas_mut(), zip(dmabuf.strides(), dmabuf.offsets()))
        {
            let chunk = data.chunk_mut();
            *chunk.size_mut() = 1;
            *chunk.stride_mut() = stride as i32;
            *chunk.offset_mut() = offset;

            trace!(
                "pw buffer: fd = {}, stride = {stride}, offset = {offset}",
                data.as_raw().fd
            );
        }

        true
    }
}

impl CastState {
    fn pending_size(&self) -> Option<Size<u32, Physical>> {
        match self {
            CastState::ResizePending { pending_size } => Some(*pending_size),
            CastState::ConfirmationPending { size, .. } => Some(*size),
            CastState::Ready { .. } => None,
        }
    }

    fn expected_format_size(&self) -> Size<u32, Physical> {
        match self {
            CastState::ResizePending { pending_size } => *pending_size,
            CastState::ConfirmationPending { size, .. } => *size,
            CastState::Ready { size, .. } => *size,
        }
    }
}

fn make_video_params(
    formats: &FormatSet,
    size: Size<u32, Physical>,
    refresh: u32,
    alpha: bool,
) -> pod::Object {
    let format = if alpha {
        VideoFormat::BGRA
    } else {
        VideoFormat::BGRx
    };

    let fourcc = if alpha {
        Fourcc::Argb8888
    } else {
        Fourcc::Xrgb8888
    };

    let formats: Vec<_> = formats
        .iter()
        .filter_map(|f| (f.code == fourcc).then_some(u64::from(f.modifier) as i64))
        .collect();

    trace!("offering: {formats:?}");

    let dont_fixate = if formats.len() > 1 {
        PropertyFlags::DONT_FIXATE
    } else {
        PropertyFlags::empty()
    };

    pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pod::property!(FormatProperties::VideoFormat, Id, format),
        Property {
            key: FormatProperties::VideoModifier.as_raw(),
            flags: PropertyFlags::MANDATORY | dont_fixate,
            value: pod::Value::Choice(ChoiceValue::Long(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Enum {
                    default: formats[0],
                    alternatives: formats,
                }
            )))
        },
        pod::property!(
            FormatProperties::VideoSize,
            Rectangle,
            Rectangle {
                width: size.w,
                height: size.h,
            }
        ),
        pod::property!(
            FormatProperties::VideoFramerate,
            Fraction,
            Fraction { num: 0, denom: 1 }
        ),
        pod::property!(
            FormatProperties::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            Fraction {
                num: refresh,
                denom: 1000
            },
            Fraction { num: 1, denom: 1 },
            Fraction {
                num: refresh,
                denom: 1000
            }
        ),
    )
}

fn make_pod(buffer: &mut Vec<u8>, object: pod::Object) -> &Pod {
    PodSerializer::serialize(Cursor::new(&mut *buffer), &pod::Value::Object(object)).unwrap();
    Pod::from_bytes(buffer).unwrap()
}

fn find_preferred_modifier(
    gbm: &GbmDevice<DrmDeviceFd>,
    size: Size<u32, Physical>,
    fourcc: Fourcc,
    modifiers: Vec<i64>,
) -> anyhow::Result<(Modifier, usize)> {
    debug!("find_preferred_modifier: size={size:?}, fourcc={fourcc}, modifiers={modifiers:?}");

    let (buffer, modifier) = allocate_buffer(gbm, size, fourcc, &modifiers)?;

    let dmabuf = buffer
        .export()
        .context("error exporting GBM buffer object as dmabuf")?;
    let plane_count = dmabuf.num_planes();

    // FIXME: Ideally this also needs to try binding the dmabuf for rendering.

    Ok((modifier, plane_count))
}

fn allocate_buffer(
    gbm: &GbmDevice<DrmDeviceFd>,
    size: Size<u32, Physical>,
    fourcc: Fourcc,
    modifiers: &[i64],
) -> anyhow::Result<(GbmBuffer, Modifier)> {
    let (w, h) = (size.w, size.h);
    let flags = GbmBufferFlags::RENDERING;

    if modifiers.len() == 1 && Modifier::from(modifiers[0] as u64) == Modifier::Invalid {
        let bo = gbm
            .create_buffer_object::<()>(w, h, fourcc, flags)
            .context("error creating GBM buffer object")?;

        let buffer = GbmBuffer::from_bo(bo, true);
        Ok((buffer, Modifier::Invalid))
    } else {
        let modifiers = modifiers
            .iter()
            .map(|m| Modifier::from(*m as u64))
            .filter(|m| *m != Modifier::Invalid);

        let bo = gbm
            .create_buffer_object_with_modifiers2::<()>(w, h, fourcc, modifiers, flags)
            .context("error creating GBM buffer object")?;

        let modifier = bo.modifier();
        let buffer = GbmBuffer::from_bo(bo, false);
        Ok((buffer, modifier))
    }
}

fn allocate_dmabuf(
    gbm: &GbmDevice<DrmDeviceFd>,
    size: Size<u32, Physical>,
    fourcc: Fourcc,
    modifier: Modifier,
) -> anyhow::Result<Dmabuf> {
    let (buffer, _modifier) = allocate_buffer(gbm, size, fourcc, &[u64::from(modifier) as i64])?;
    let dmabuf = buffer
        .export()
        .context("error exporting GBM buffer object as dmabuf")?;
    Ok(dmabuf)
}
