use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::Cursor;
use std::mem;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::rc::Rc;
use std::time::Duration;

use anyhow::Context as _;
use pipewire::context::Context;
use pipewire::core::Core;
use pipewire::main_loop::MainLoop;
use pipewire::properties::Properties;
use pipewire::spa::buffer::DataType;
use pipewire::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pipewire::spa::param::format_utils::parse_format;
use pipewire::spa::param::video::{VideoFormat, VideoInfoRaw};
use pipewire::spa::param::ParamType;
use pipewire::spa::pod::serialize::PodSerializer;
use pipewire::spa::pod::{self, ChoiceValue, Pod, Property, PropertyFlags};
use pipewire::spa::sys::*;
use pipewire::spa::utils::{
    Choice, ChoiceEnum, ChoiceFlags, Direction, Fraction, Rectangle, SpaTypes,
};
use pipewire::stream::{Stream, StreamFlags, StreamListener, StreamState};
use smithay::backend::allocator::dmabuf::{AsDmabuf, Dmabuf};
use smithay::backend::allocator::gbm::{GbmBuffer, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::output::WeakOutput;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::gbm::Modifier;
use smithay::utils::{Physical, Scale, Size, Transform};
use zbus::SignalContext;

use crate::dbus::mutter_screen_cast::{self, CursorMode};
use crate::niri::State;
use crate::render_helpers::render_to_dmabuf;

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
    pub size: Rc<Cell<CastSize>>,
    pub refresh: u32,
    offer_alpha: bool,
    pub cursor_mode: CursorMode,
    pub last_frame_time: Duration,
    pub min_time_between_frames: Rc<Cell<Duration>>,
    pub dmabufs: Rc<RefCell<HashMap<i32, Dmabuf>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastSize {
    InitialPending(Size<i32, Physical>),
    Ready(Size<i32, Physical>),
    ChangePending {
        last_negotiated: Size<i32, Physical>,
        pending: Size<i32, Physical>,
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
    ($params:ident, $size:expr, $refresh:expr, $alpha:expr) => {
        let mut b1 = Vec::new();
        let mut b2 = Vec::new();

        let o1 = make_video_params($size, $refresh, false);
        let pod1 = make_pod(&mut b1, o1);

        let mut p1;
        let mut p2;
        $params = if $alpha {
            let o2 = make_video_params($size, $refresh, true);
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
        let negotiated_alpha = Rc::new(Cell::new(false));

        let pending_size = size;
        let size = Rc::new(Cell::new(CastSize::InitialPending(size)));

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
                let size = size.clone();
                let negotiated_alpha = negotiated_alpha.clone();
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
                    trace!("pw stream: got format = {format:?}");

                    let expected_size = size.get().expected_format_size();
                    let format_size =
                        Size::from((format.size().width as i32, format.size().height as i32));

                    if format_size == expected_size {
                        size.set(CastSize::Ready(expected_size));
                    } else {
                        size.set(CastSize::ChangePending {
                            last_negotiated: format_size,
                            pending: expected_size,
                        });
                    }

                    negotiated_alpha.set(format.format() == VideoFormat::BGRA);

                    let max_frame_rate = format.max_framerate();
                    // Subtract 0.5 ms to improve edge cases when equal to refresh rate.
                    let min_frame_time = Duration::from_secs_f64(
                        max_frame_rate.denom as f64 / max_frame_rate.num as f64,
                    ) - Duration::from_micros(500);
                    min_time_between_frames.set(min_frame_time);

                    const BPP: u32 = 4;
                    let stride = format.size().width * BPP;
                    let size = stride * format.size().height;

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
                        Property::new(SPA_PARAM_BUFFERS_blocks, pod::Value::Int(1)),
                        Property::new(SPA_PARAM_BUFFERS_size, pod::Value::Int(size as i32)),
                        Property::new(SPA_PARAM_BUFFERS_stride, pod::Value::Int(stride as i32)),
                        Property::new(SPA_PARAM_BUFFERS_align, pod::Value::Int(16)),
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
                    stream.update_params(&mut params).unwrap();
                }
            })
            .add_buffer({
                let dmabufs = dmabufs.clone();
                let stop_cast = stop_cast.clone();
                let size = size.clone();
                let negotiated_alpha = negotiated_alpha.clone();
                move |stream, (), buffer| {
                    let size = size.get().negotiated_size();
                    let alpha = negotiated_alpha.get();
                    trace!("pw stream: add_buffer, size={:?}, alpha={alpha}", size);
                    let size = size.expect("size must be negotiated to allocate buffers");

                    unsafe {
                        let spa_buffer = (*buffer).buffer;
                        let spa_data = (*spa_buffer).datas;
                        assert!((*spa_buffer).n_datas > 0);
                        assert!((*spa_data).type_ & (1 << DataType::DmaBuf.as_raw()) > 0);

                        let fourcc = if alpha {
                            Fourcc::Argb8888
                        } else {
                            Fourcc::Xrgb8888
                        };

                        let bo = match gbm.create_buffer_object::<()>(
                            size.w as u32,
                            size.h as u32,
                            fourcc,
                            GbmBufferFlags::RENDERING | GbmBufferFlags::LINEAR,
                        ) {
                            Ok(bo) => bo,
                            Err(err) => {
                                warn!("error creating GBM buffer object: {err:?}");
                                stop_cast();
                                return;
                            }
                        };
                        let buffer = GbmBuffer::from_bo(bo, true);
                        let dmabuf = match buffer.export() {
                            Ok(dmabuf) => dmabuf,
                            Err(err) => {
                                warn!("error exporting GBM buffer object as dmabuf: {err:?}");
                                stop_cast();
                                return;
                            }
                        };

                        let fd = dmabuf.handles().next().unwrap().as_raw_fd();

                        (*spa_data).type_ = DataType::DmaBuf.as_raw();
                        (*spa_data).maxsize = dmabuf.strides().next().unwrap() * size.h as u32;
                        (*spa_data).fd = fd as i64;
                        (*spa_data).flags = SPA_DATA_FLAG_READWRITE;

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

                        let fd = (*spa_data).fd as i32;
                        dmabufs.borrow_mut().remove(&fd);
                    }
                }
            })
            .register()
            .unwrap();

        trace!("starting pw stream with size={pending_size:?}, refresh={refresh}");

        let params;
        make_params!(params, pending_size, refresh, alpha);
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
            size,
            refresh,
            offer_alpha: alpha,
            cursor_mode,
            last_frame_time: Duration::ZERO,
            min_time_between_frames,
            dmabufs,
        };
        Ok(cast)
    }
}

impl Cast {
    pub fn ensure_size(&self, size: Size<i32, Physical>) -> anyhow::Result<CastSizeChange> {
        let current_size = self.size.get();
        if current_size == CastSize::Ready(size) {
            return Ok(CastSizeChange::Ready);
        }

        if current_size.pending_size() == Some(size) {
            debug!("stream size still hasn't changed, skipping frame");
            return Ok(CastSizeChange::Pending);
        }

        let _span = tracy_client::span!("Cast::ensure_size");
        debug!("cast size changed, updating stream size");

        self.size.set(current_size.with_pending(size));

        let params;
        make_params!(params, size, self.refresh, self.offer_alpha);
        self.stream
            .update_params(params)
            .context("error updating stream params")?;

        Ok(CastSizeChange::Pending)
    }

    pub fn set_refresh(&mut self, refresh: u32) -> anyhow::Result<()> {
        if self.refresh == refresh {
            return Ok(());
        }

        let _span = tracy_client::span!("Cast::set_refresh");
        debug!("cast FPS changed, updating stream FPS");
        self.refresh = refresh;

        let size = self.size.get().expected_format_size();
        let params;
        make_params!(params, size, self.refresh, self.offer_alpha);
        self.stream
            .update_params(params)
            .context("error updating stream params")?;

        Ok(())
    }

    pub fn should_skip_frame(&self, target_frame_time: Duration) -> bool {
        let last = self.last_frame_time;
        let min = self.min_time_between_frames.get();

        if last.is_zero() {
            trace!(?target_frame_time, ?last, "last is zero, recording");
            return false;
        }

        if target_frame_time < last {
            // Record frame with a warning; in case it was an overflow this will fix it.
            warn!(
                ?target_frame_time,
                ?last,
                "target frame time is below last, did it overflow or did we mispredict?"
            );
            return false;
        }

        let diff = target_frame_time - last;
        if diff < min {
            trace!(
                ?target_frame_time,
                ?last,
                "skipping frame because it is too soon: diff={diff:?} < min={min:?}",
            );
            return true;
        }

        false
    }

    pub fn dequeue_buffer_and_render(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: impl Iterator<Item = impl RenderElement<GlesRenderer>>,
        size: Size<i32, Physical>,
        scale: Scale<f64>,
    ) -> bool {
        let mut buffer = match self.stream.dequeue_buffer() {
            Some(buffer) => buffer,
            None => {
                warn!("no available buffer in pw stream, skipping frame");
                return false;
            }
        };

        let data = &mut buffer.datas_mut()[0];
        let fd = data.as_raw().fd as i32;
        let dmabuf = self.dmabufs.borrow()[&fd].clone();

        if let Err(err) =
            render_to_dmabuf(renderer, dmabuf, size, scale, Transform::Normal, elements)
        {
            warn!("error rendering to dmabuf: {err:?}");
            return false;
        }

        let maxsize = data.as_raw().maxsize;
        let chunk = data.chunk_mut();
        *chunk.size_mut() = maxsize;
        *chunk.stride_mut() = maxsize as i32 / size.h;

        true
    }
}

impl CastSize {
    fn pending_size(self) -> Option<Size<i32, Physical>> {
        match self {
            CastSize::InitialPending(pending) => Some(pending),
            CastSize::Ready(_) => None,
            CastSize::ChangePending { pending, .. } => Some(pending),
        }
    }

    fn negotiated_size(self) -> Option<Size<i32, Physical>> {
        match self {
            CastSize::InitialPending(_) => None,
            CastSize::Ready(size) => Some(size),
            CastSize::ChangePending {
                last_negotiated, ..
            } => Some(last_negotiated),
        }
    }

    fn expected_format_size(self) -> Size<i32, Physical> {
        match self {
            CastSize::InitialPending(pending) => pending,
            CastSize::Ready(size) => size,
            CastSize::ChangePending { pending, .. } => pending,
        }
    }

    fn with_pending(self, pending: Size<i32, Physical>) -> Self {
        match self {
            CastSize::InitialPending(_) => CastSize::InitialPending(pending),
            CastSize::Ready(size) => CastSize::ChangePending {
                last_negotiated: size,
                pending,
            },
            CastSize::ChangePending {
                last_negotiated, ..
            } => CastSize::ChangePending {
                last_negotiated,
                pending,
            },
        }
    }
}

fn make_video_params(size: Size<i32, Physical>, refresh: u32, alpha: bool) -> pod::Object {
    let format = if alpha {
        VideoFormat::BGRA
    } else {
        VideoFormat::BGRx
    };

    pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pod::property!(FormatProperties::VideoFormat, Id, format),
        Property {
            key: FormatProperties::VideoModifier.as_raw(),
            value: pod::Value::Long(u64::from(Modifier::Invalid) as i64),
            flags: PropertyFlags::MANDATORY,
        },
        pod::property!(
            FormatProperties::VideoSize,
            Rectangle,
            Rectangle {
                width: size.w as u32,
                height: size.h as u32,
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
