use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{self, Cursor};
use std::iter::zip;
use std::mem;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Context as _;
use calloop::timer::{TimeoutAction, Timer};
use calloop::RegistrationToken;
use pipewire::context::Context;
use pipewire::core::{Core, PW_ID_CORE};
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
use pipewire::sys::{pw_buffer, pw_stream_queue_buffer};
use smithay::backend::allocator::dmabuf::{AsDmabuf, Dmabuf};
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::gbm::{GbmBuffer, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::{Format, Fourcc};
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::sync::SyncPoint;
use smithay::output::{Output, OutputModeSource};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::drm::control::{syncobj, Device as _};
use smithay::reexports::gbm::Modifier;
use smithay::utils::{Physical, Scale, Size, Transform};
use zbus::object_server::SignalEmitter;

use crate::dbus::mutter_screen_cast::{self, CursorMode};
use crate::niri::{CastTarget, State};
use crate::render_helpers::{clear_dmabuf, render_to_dmabuf};
use crate::utils::get_monotonic_time;

// Give a 0.1 ms allowance for presentation time errors.
const CAST_DELAY_ALLOWANCE: Duration = Duration::from_micros(100);

// Added in PipeWire 1.2.0.
#[allow(non_upper_case_globals)]
const SPA_META_SyncTimeline: spa_meta_type = 9;
#[allow(non_upper_case_globals)]
const SPA_PARAM_BUFFERS_metaType: spa_param_buffers = 7;
#[allow(non_upper_case_globals)]
const SPA_DATA_SyncObj: spa_data_type = 5;

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct spa_meta_sync_timeline {
    pub flags: u32,
    pub padding: u32,
    pub acquire_point: u64,
    pub release_point: u64,
}

/// A map of syncobj fd => handle for proper Drop.
struct SyncobjMap {
    gbm: GbmDevice<DrmDeviceFd>,
    map: HashMap<RawFd, syncobj::Handle>,
}

impl Drop for SyncobjMap {
    fn drop(&mut self) {
        if !self.map.is_empty() {
            debug!("dropping syncobjs on an abruptly stopped cast");
            for (fd, syncobj) in self.map.drain() {
                unsafe {
                    if let Err(err) = self.gbm.destroy_syncobj(syncobj) {
                        warn!("error destroying syncobj: {err:?}");
                    }

                    drop(OwnedFd::from_raw_fd(fd));
                }
            }
        }
    }
}

pub struct PipeWire {
    _context: Context,
    pub core: Core,
    pub token: RegistrationToken,
    to_niri: calloop::channel::Sender<PwToNiri>,
}

pub enum PwToNiri {
    StopCast { session_id: usize },
    Redraw { stream_id: usize },
    FatalError,
}

pub struct Cast {
    pub session_id: usize,
    pub stream_id: usize,
    pub stream: Stream,
    _listener: StreamListener<()>,
    pub is_active: Rc<Cell<bool>>,
    pub target: CastTarget,
    pub dynamic_target: bool,
    formats: FormatSet,
    state: Rc<RefCell<CastState>>,
    refresh: Rc<Cell<u32>>,
    offer_alpha: bool,
    pub cursor_mode: CursorMode,
    pub last_frame_time: Duration,
    min_time_between_frames: Rc<Cell<Duration>>,
    dmabufs: Rc<RefCell<HashMap<i64, Dmabuf>>>,
    syncobjs: Rc<RefCell<SyncobjMap>>,
    // Buffers we dequeued from PipeWire that are waiting for their release sync point to be
    // signalled before we can use them.
    dequeued_buffers: Rc<RefCell<Vec<NonNull<pw_buffer>>>>,
    gbm: GbmDevice<DrmDeviceFd>,
    scheduled_redraw: Option<RegistrationToken>,
}

#[allow(clippy::large_enum_variant)]
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
    pub fn new(
        event_loop: &LoopHandle<'static, State>,
        to_niri: calloop::channel::Sender<PwToNiri>,
    ) -> anyhow::Result<Self> {
        let main_loop = MainLoop::new(None).context("error creating MainLoop")?;
        let context = Context::new(&main_loop).context("error creating Context")?;
        let core = context.connect(None).context("error creating Core")?;

        let to_niri_ = to_niri.clone();
        let listener = core
            .add_listener_local()
            .error(move |id, seq, res, message| {
                warn!(id, seq, res, message, "pw error");

                // Reset PipeWire on connection errors.
                if id == PW_ID_CORE && res == -32 {
                    if let Err(err) = to_niri_.send(PwToNiri::FatalError) {
                        warn!("error sending FatalError to niri: {err:?}");
                    }
                }
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
        let token = event_loop
            .insert_source(generic, move |_, wrapper, _| {
                let _span = tracy_client::span!("pipewire iteration");
                wrapper.0.loop_().iterate(Duration::ZERO);
                Ok(PostAction::Continue)
            })
            .unwrap();

        Ok(Self {
            _context: context,
            core,
            token,
            to_niri,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_cast(
        &self,
        gbm: GbmDevice<DrmDeviceFd>,
        formats: FormatSet,
        session_id: usize,
        stream_id: usize,
        target: CastTarget,
        dynamic_target: bool,
        size: Size<i32, Physical>,
        refresh: u32,
        alpha: bool,
        cursor_mode: CursorMode,
        signal_ctx: SignalEmitter<'static>,
    ) -> anyhow::Result<Cast> {
        let _span = tracy_client::span!("PipeWire::start_cast");

        let to_niri_ = self.to_niri.clone();
        let stop_cast = move || {
            if let Err(err) = to_niri_.send(PwToNiri::StopCast { session_id }) {
                warn!("error sending StopCast to niri: {err:?}");
            }
        };
        let to_niri_ = self.to_niri.clone();
        let redraw = move || {
            if let Err(err) = to_niri_.send(PwToNiri::Redraw { stream_id }) {
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
        let syncobjs = SyncobjMap {
            gbm: gbm.clone(),
            map: HashMap::new(),
        };
        let syncobjs = Rc::new(RefCell::new(syncobjs));
        let dequeued_buffers = Rc::new(RefCell::new(Vec::new()));
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

                    let o1 = make_buffers_params(plane_count, true);
                    // Fallback without SyncTimeline.
                    let o2 = make_buffers_params(plane_count, false);

                    let o3 = pod::object!(
                        SpaTypes::ObjectParamMeta,
                        ParamType::Meta,
                        Property::new(
                            SPA_PARAM_META_type,
                            pod::Value::Id(spa::utils::Id(SPA_META_SyncTimeline))
                        ),
                        Property::new(
                            SPA_PARAM_META_size,
                            pod::Value::Int(size_of::<spa_meta_sync_timeline>() as i32)
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
                    let mut b2 = vec![];
                    let mut b3 = vec![];
                    let mut params = [
                        make_pod(&mut b1, o1),
                        make_pod(&mut b2, o2),
                        make_pod(&mut b3, o3),
                    ];

                    if let Err(err) = stream.update_params(&mut params) {
                        warn!("error updating stream params: {err:?}");
                        stop_cast();
                    }
                }
            })
            .add_buffer({
                let gbm = gbm.clone();
                let dmabufs = dmabufs.clone();
                let syncobjs = syncobjs.clone();
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

                        let have_sync_timeline = !spa_buffer_find_meta_data(
                            spa_buffer,
                            SPA_META_SyncTimeline,
                            mem::size_of::<spa_meta_sync_timeline>(),
                        )
                        .is_null();

                        let mut expected_n_datas = dmabuf.num_planes();
                        if have_sync_timeline {
                            expected_n_datas += 2;
                        }
                        assert_eq!((*spa_buffer).n_datas as usize, expected_n_datas);

                        for (i, fd) in dmabuf.handles().enumerate() {
                            let spa_data = (*spa_buffer).datas.add(i);
                            assert!((*spa_data).type_ & (1 << DataType::DmaBuf.as_raw()) > 0);

                            (*spa_data).type_ = DataType::DmaBuf.as_raw();
                            // With DMA-BUFs, consumers should ignore the maxsize field, and
                            // producers are allowed to set it to 0.
                            //
                            // https://docs.pipewire.org/page_dma_buf.html
                            (*spa_data).maxsize = 1;
                            (*spa_data).fd = fd.as_raw_fd() as i64;
                            (*spa_data).flags = SPA_DATA_FLAG_READWRITE;
                        }

                        let fd = (*(*spa_buffer).datas).fd;
                        assert!(dmabufs.borrow_mut().insert(fd, dmabuf).is_none());

                        let syncobjs = &mut *syncobjs.borrow_mut();
                        if let Err(err) = maybe_create_syncobj(&gbm, spa_buffer, &mut syncobjs.map)
                        {
                            warn!("error filling syncobj buffer data: {err:?}");
                        };
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
                let syncobjs = syncobjs.clone();
                let dequeued_buffers = dequeued_buffers.clone();
                let gbm = gbm.clone();
                move |_stream, (), buffer| {
                    trace!("pw stream: remove_buffer");

                    unsafe {
                        let spa_buffer = (*buffer).buffer;
                        let spa_data = (*spa_buffer).datas;
                        assert!((*spa_buffer).n_datas > 0);

                        let fd = (*spa_data).fd;
                        if let Some(dmabuf) = dmabufs.borrow_mut().remove(&fd) {
                            let have_sync_timeline = !spa_buffer_find_meta_data(
                                spa_buffer,
                                SPA_META_SyncTimeline,
                                mem::size_of::<spa_meta_sync_timeline>(),
                            )
                            .is_null();

                            let mut expected_n_datas = dmabuf.num_planes();
                            if have_sync_timeline {
                                expected_n_datas += 2;
                            }
                            assert_eq!((*spa_buffer).n_datas as usize, expected_n_datas);

                            let syncobjs = &mut *syncobjs.borrow_mut();
                            maybe_remove_syncobj(&gbm, spa_buffer, &mut syncobjs.map);

                            dequeued_buffers
                                .borrow_mut()
                                .retain(|buf: &NonNull<_>| buf.as_ptr() != buffer);
                        } else {
                            error!("missing dmabuf in remove_buffer()");
                        }
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
            stream_id,
            stream,
            _listener: listener,
            is_active,
            target,
            dynamic_target,
            formats,
            state,
            refresh,
            offer_alpha: alpha,
            cursor_mode,
            last_frame_time: Duration::ZERO,
            min_time_between_frames,
            dmabufs,
            syncobjs,
            dequeued_buffers,
            gbm,
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

    fn dequeue_available_buffer(&mut self) -> Option<NonNull<pw_buffer>> {
        let mut syncobjs = self.syncobjs.borrow_mut();
        let syncobjs = &mut syncobjs.map;

        unsafe {
            // Check if any already-dequeued buffers are ready.
            let mut dequeued_buffers = self.dequeued_buffers.borrow_mut();
            for (i, buffer) in dequeued_buffers.iter().enumerate() {
                if can_reuse_pw_buffer(&self.gbm, *buffer, syncobjs) {
                    debug!("buffer is now ready, yielding");
                    return Some(dequeued_buffers.remove(i));
                }
            }

            while let Some(buffer) = NonNull::new(self.stream.dequeue_raw_buffer()) {
                if can_reuse_pw_buffer(&self.gbm, buffer, syncobjs) {
                    return Some(buffer);
                }

                debug!("buffer isn't ready yet, storing");
                dequeued_buffers.push(buffer);
            }
        }

        None
    }

    pub fn dequeue_buffer_and_render(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &[impl RenderElement<GlesRenderer>],
        size: Size<i32, Physical>,
        scale: Scale<f64>,
        wait_for_sync: bool,
    ) -> bool {
        let mut state = self.state.borrow_mut();
        let CastState::Ready { damage_tracker, .. } = &mut *state else {
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
        drop(state);

        unsafe {
            let Some(pw_buffer) = self.dequeue_available_buffer() else {
                warn!("no available buffer in pw stream, skipping frame");
                return false;
            };
            let pw_buffer = pw_buffer.as_ptr();

            let spa_buffer = (*pw_buffer).buffer;
            let fd = (*(*spa_buffer).datas).fd;
            let dmabuf = &self.dmabufs.borrow()[&fd];

            match render_to_dmabuf(
                renderer,
                dmabuf.clone(),
                size,
                scale,
                Transform::Normal,
                elements.iter().rev(),
            ) {
                Ok(sync_point) => {
                    // FIXME: implement PipeWire explicit sync, and at the very least async wait.
                    if wait_for_sync {
                        let _span = tracy_client::span!("wait for completion");
                        if let Err(err) = sync_point.wait() {
                            warn!("error waiting for pw frame completion: {err:?}");
                        }
                    }

                    let syncobjs = &mut *self.syncobjs.borrow_mut();
                    if let Err(err) =
                        maybe_set_sync_points(&self.gbm, spa_buffer, &mut syncobjs.map, &sync_point)
                    {
                        warn!("error setting sync point: {err:?}");
                    };
                }
                Err(err) => {
                    warn!("error rendering to dmabuf: {err:?}");
                    return_unused_buffer(&self.stream, pw_buffer);
                    return false;
                }
            }

            for (i, (stride, offset)) in zip(dmabuf.strides(), dmabuf.offsets()).enumerate() {
                let spa_data = (*spa_buffer).datas.add(i);
                let chunk = (*spa_data).chunk;

                // With DMA-BUFs, consumers should ignore the size field, and producers are allowed
                // to set it to 0.
                //
                // https://docs.pipewire.org/page_dma_buf.html
                //
                // However, OBS checks for size != 0 as a workaround for old compositor versions,
                // so we set it to 1.
                (*chunk).size = 1;
                // Clear the corrupted flag we may have set before.
                (*chunk).flags = SPA_CHUNK_FLAG_NONE as i32;

                (*chunk).stride = stride as i32;
                (*chunk).offset = offset;

                trace!(
                    "pw buffer: fd = {}, stride = {stride}, offset = {offset}",
                    (*spa_data).fd
                );
            }

            pw_stream_queue_buffer(self.stream.as_raw_ptr(), pw_buffer);
        }

        true
    }

    pub fn dequeue_buffer_and_clear(
        &mut self,
        renderer: &mut GlesRenderer,
        wait_for_sync: bool,
    ) -> bool {
        // Clear out the damage tracker if we're in Ready state.
        if let CastState::Ready { damage_tracker, .. } = &mut *self.state.borrow_mut() {
            *damage_tracker = None;
        };

        unsafe {
            let Some(pw_buffer) = self.dequeue_available_buffer() else {
                warn!("no available buffer in pw stream, skipping clear");
                return false;
            };
            let pw_buffer = pw_buffer.as_ptr();

            let spa_buffer = (*pw_buffer).buffer;
            let fd = (*(*spa_buffer).datas).fd;
            let dmabuf = &self.dmabufs.borrow()[&fd];

            match clear_dmabuf(renderer, dmabuf.clone()) {
                Ok(sync_point) => {
                    // FIXME: implement PipeWire explicit sync, and at the very least async wait.
                    if wait_for_sync {
                        let _span = tracy_client::span!("wait for completion");
                        if let Err(err) = sync_point.wait() {
                            warn!("error waiting for pw frame completion: {err:?}");
                        }
                    }

                    let syncobjs = &mut *self.syncobjs.borrow_mut();
                    if let Err(err) =
                        maybe_set_sync_points(&self.gbm, spa_buffer, &mut syncobjs.map, &sync_point)
                    {
                        warn!("error setting sync point: {err:?}");
                    };
                }
                Err(err) => {
                    warn!("error clearing dmabuf: {err:?}");
                    return_unused_buffer(&self.stream, pw_buffer);
                    return false;
                }
            }

            for (i, (stride, offset)) in zip(dmabuf.strides(), dmabuf.offsets()).enumerate() {
                let spa_data = (*spa_buffer).datas.add(i);
                let chunk = (*spa_data).chunk;

                // With DMA-BUFs, consumers should ignore the size field, and producers are allowed
                // to set it to 0.
                //
                // https://docs.pipewire.org/page_dma_buf.html
                //
                // However, OBS checks for size != 0 as a workaround for old compositor versions,
                // so we set it to 1.
                (*chunk).size = 1;
                // Clear the corrupted flag we may have set before.
                (*chunk).flags = SPA_CHUNK_FLAG_NONE as i32;

                (*chunk).stride = stride as i32;
                (*chunk).offset = offset;

                trace!(
                    "pw buffer: fd = {}, stride = {stride}, offset = {offset}",
                    (*spa_data).fd
                );
            }

            pw_stream_queue_buffer(self.stream.as_raw_ptr(), pw_buffer);
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

fn make_buffers_params(mut plane_count: i32, sync_timeline: bool) -> pod::Object {
    if sync_timeline {
        // Two extra file descriptors for acquire and release.
        plane_count += 2;
    }

    let mut object = pod::object!(
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

    if sync_timeline {
        // TODO: do we need to gate this behind runtime check for PW 1.2.0? What happens on older
        // PW?
        object.properties.push(Property {
            key: SPA_PARAM_BUFFERS_metaType,
            flags: PropertyFlags::MANDATORY,
            value: pod::Value::Int(1 << SPA_META_SyncTimeline),
        });
    }

    object
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

unsafe fn maybe_create_syncobj(
    gbm: &GbmDevice<DrmDeviceFd>,
    spa_buffer: *mut spa_buffer,
    syncobjs: &mut HashMap<RawFd, syncobj::Handle>,
) -> anyhow::Result<()> {
    unsafe {
        let sync_timeline: *mut spa_meta_sync_timeline = spa_buffer_find_meta_data(
            spa_buffer,
            SPA_META_SyncTimeline,
            mem::size_of::<spa_meta_sync_timeline>(),
        )
        .cast();

        if sync_timeline.is_null() {
            return Ok(());
        }

        let syncobj = gbm
            .create_syncobj(false)
            .context("error creating syncobj")?;
        let fd = match gbm.syncobj_to_fd(syncobj, false) {
            Ok(x) => x,
            Err(err) => {
                let _ = gbm.destroy_syncobj(syncobj);
                return Err(err).context("error exporting syncobj to fd");
            }
        };

        debug!("filling syncobj fd={fd:?}");

        let n_datas = (*spa_buffer).n_datas as usize;
        assert!(n_datas >= 2);

        let acquire_data = (*spa_buffer).datas.add(n_datas - 2);
        (*acquire_data).type_ = SPA_DATA_SyncObj;
        (*acquire_data).flags = SPA_DATA_FLAG_READABLE;
        (*acquire_data).fd = i64::from(fd.as_raw_fd());

        let release_data = (*spa_buffer).datas.add(n_datas - 1);
        (*release_data).type_ = SPA_DATA_SyncObj;
        (*release_data).flags = SPA_DATA_FLAG_READABLE;
        (*release_data).fd = i64::from(fd.as_raw_fd());

        syncobjs.insert(fd.into_raw_fd(), syncobj);

        Ok(())
    }
}

unsafe fn maybe_remove_syncobj(
    gbm: &GbmDevice<DrmDeviceFd>,
    spa_buffer: *mut spa_buffer,
    syncobjs: &mut HashMap<RawFd, syncobj::Handle>,
) {
    unsafe {
        let sync_timeline: *mut spa_meta_sync_timeline = spa_buffer_find_meta_data(
            spa_buffer,
            SPA_META_SyncTimeline,
            mem::size_of::<spa_meta_sync_timeline>(),
        )
        .cast();

        if sync_timeline.is_null() {
            return;
        }

        let n_datas = (*spa_buffer).n_datas as usize;
        assert!(n_datas >= 2);

        let acquire_data = (*spa_buffer).datas.add(n_datas - 2);
        let fd = (*acquire_data).fd as RawFd;

        debug!("removing syncobj fd={fd:?}");

        let Some(syncobj) = syncobjs.remove(&fd) else {
            error!("missing syncobj in remove_buffer()");
            return;
        };

        if let Err(err) = gbm.destroy_syncobj(syncobj) {
            warn!("error destroying syncobj: {err:?}");
        }

        drop(OwnedFd::from_raw_fd(fd));
    }
}

unsafe fn maybe_set_sync_points(
    gbm: &GbmDevice<DrmDeviceFd>,
    spa_buffer: *mut spa_buffer,
    syncobjs: &mut HashMap<RawFd, syncobj::Handle>,
    sync_point: &SyncPoint,
) -> anyhow::Result<()> {
    unsafe {
        let sync_timeline: *mut spa_meta_sync_timeline = spa_buffer_find_meta_data(
            spa_buffer,
            SPA_META_SyncTimeline,
            mem::size_of::<spa_meta_sync_timeline>(),
        )
        .cast();

        if sync_timeline.is_null() {
            return Ok(());
        }

        // At this point, we must ensure that our syncobj contains a fence, since clients can do a
        // blocking wait until the fence is available (OBS does this).
        // TODO

        let n_datas = (*spa_buffer).n_datas as usize;
        assert!(n_datas >= 2);

        let acquire_data = (*spa_buffer).datas.add(n_datas - 2);
        let fd = (*acquire_data).fd as RawFd;

        let Some(syncobj) = syncobjs.get(&fd) else {
            error!("missing syncobj in maybe_set_sync_points()");
            return Ok(());
        };

        let Some(sync_fd) = sync_point.export() else {
            debug!("have sync_timeline but no sync_fd to export");
            return Ok(());
        };

        let acquire_point = (*sync_timeline).release_point + 1;

        // Import sync_fd into our syncobj at the correct point.
        let tmp = gbm
            .create_syncobj(false)
            .context("error creating temp syncobj")?;
        let res = drm_import_sync_file(gbm, tmp, sync_fd.as_fd())
            .context("error importing sync_fd to temp syncobj");
        let res = if res.is_ok() {
            gbm.syncobj_timeline_transfer(tmp, *syncobj, 0, acquire_point)
                .context("error transferring sync point")
        } else {
            res
        };
        let _ = gbm.destroy_syncobj(tmp);
        let () = res?;

        (*sync_timeline).acquire_point = acquire_point;
        (*sync_timeline).release_point = acquire_point + 1;

        debug!("set sync timeline fd={fd:?} to acquire={acquire_point}");

        Ok(())
    }
}

// Our own version until drm-ffi is fixed:
// https://github.com/Smithay/drm-rs/issues/224
unsafe fn drm_import_sync_file(
    gbm: &GbmDevice<DrmDeviceFd>,
    syncobj: syncobj::Handle,
    sync_file: BorrowedFd,
) -> io::Result<()> {
    use drm_ffi::drm_sys::*;
    use rustix::ioctl::{self, ioctl, Opcode, Updater};
    use smithay::reexports::rustix;

    unsafe fn fd_to_handle(fd: BorrowedFd, data: &mut drm_syncobj_handle) -> io::Result<()> {
        const OPCODE: Opcode =
            ioctl::opcode::read_write::<drm_syncobj_handle>(DRM_IOCTL_BASE, 0xC2);
        Ok(ioctl(fd, Updater::<OPCODE, drm_syncobj_handle>::new(data))?)
    }

    let mut args = drm_syncobj_handle {
        handle: u32::from(syncobj),
        flags: DRM_SYNCOBJ_FD_TO_HANDLE_FLAGS_IMPORT_SYNC_FILE,
        fd: sync_file.as_raw_fd(),
        pad: 0,
    };

    unsafe { fd_to_handle(gbm.as_fd(), &mut args) }
}

unsafe fn can_reuse_pw_buffer(
    gbm: &GbmDevice<DrmDeviceFd>,
    pw_buffer: NonNull<pw_buffer>,
    syncobjs: &mut HashMap<RawFd, syncobj::Handle>,
) -> bool {
    unsafe {
        let spa_buffer = (*pw_buffer.as_ptr()).buffer;

        let sync_timeline: *mut spa_meta_sync_timeline = spa_buffer_find_meta_data(
            spa_buffer,
            SPA_META_SyncTimeline,
            mem::size_of::<spa_meta_sync_timeline>(),
        )
        .cast();

        if sync_timeline.is_null() {
            // No explicit sync, can always reuse.
            return true;
        }

        let n_datas = (*spa_buffer).n_datas as usize;
        assert!(n_datas >= 2);

        let release_data = (*spa_buffer).datas.add(n_datas - 1);
        let fd = (*release_data).fd as RawFd;

        let Some(syncobj) = syncobjs.get(&fd) else {
            error!("missing syncobj in can_reuse_pw_buffer()");
            return false;
        };

        let mut points = [0];
        if let Err(err) = gbm.syncobj_timeline_query(&[*syncobj], &mut points, false) {
            warn!("error querying timeline signaled point: {err:?}");
            return false;
        }

        // For fresh buffers, this will return 0 and the condition will work out to true.
        let latest_signaled_point = points[0];
        debug!(
            "latest signaled point for fd={fd:?} is {latest_signaled_point}; release point is {}",
            (*sync_timeline).release_point
        );
        latest_signaled_point >= (*sync_timeline).release_point
    }
}

unsafe fn return_unused_buffer(stream: &Stream, pw_buffer: *mut pw_buffer) {
    // pw_stream_return_buffer() requires too new PipeWire (1.4.0). So, mark as
    // corrupted and queue.
    let spa_buffer = (*pw_buffer).buffer;
    let chunk = (*(*spa_buffer).datas).chunk;
    (*chunk).size = 0;
    (*chunk).flags = SPA_CHUNK_FLAG_CORRUPTED as i32;
    pw_stream_queue_buffer(stream.as_raw_ptr(), pw_buffer);
}
