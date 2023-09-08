use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::Cursor;
use std::mem;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Context as _;
use pipewire::spa::data::DataType;
use pipewire::spa::format::{FormatProperties, MediaSubtype, MediaType};
use pipewire::spa::param::format_utils::parse_format;
use pipewire::spa::param::video::{VideoFormat, VideoInfoRaw};
use pipewire::spa::param::ParamType;
use pipewire::spa::pod::serialize::PodSerializer;
use pipewire::spa::pod::{self, ChoiceValue, Pod, Property, PropertyFlags};
use pipewire::spa::sys::*;
use pipewire::spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Rectangle, SpaTypes};
use pipewire::spa::Direction;
use pipewire::stream::{Stream, StreamFlags, StreamListener, StreamState};
use pipewire::{Context, Core, MainLoop, Properties};
use smithay::backend::allocator::dmabuf::{AsDmabuf, Dmabuf};
use smithay::backend::allocator::gbm::{GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::DrmDeviceFd;
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{self, Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::gbm::Modifier;
use zbus::SignalContext;

use crate::dbus::mutter_screen_cast::{self, CursorMode, ToNiriMsg};
use crate::LoopData;

pub struct PipeWire {
    _context: Context<MainLoop>,
    pub core: Core,
}

pub struct Cast {
    pub session_id: usize,
    pub stream: Rc<Stream>,
    _listener: StreamListener<()>,
    pub is_active: Rc<Cell<bool>>,
    pub output: Output,
    pub cursor_mode: CursorMode,
    pub last_frame_time: Duration,
    pub min_time_between_frames: Rc<Cell<Duration>>,
    pub dmabufs: Rc<RefCell<HashMap<i32, Dmabuf>>>,
}

impl PipeWire {
    pub fn new(event_loop: &LoopHandle<'static, LoopData>) -> anyhow::Result<Self> {
        let main_loop = MainLoop::new().context("error creating MainLoop")?;
        let context = Context::new(&main_loop).context("error creating Context")?;
        let core = context.connect(None).context("error creating Core")?;

        let listener = core
            .add_listener_local()
            .error(|id, seq, res, message| {
                warn!(id, seq, res, message, "pw error");
            })
            .register();
        mem::forget(listener);

        let generic = Generic::new(main_loop.fd().as_raw_fd(), Interest::READ, Mode::Level);
        event_loop
            .insert_source(generic, move |_, _, _| {
                let _span = tracy_client::span!("pipewire iteration");
                main_loop.iterate(Duration::ZERO);
                Ok(PostAction::Continue)
            })
            .unwrap();

        Ok(Self {
            _context: context,
            core,
        })
    }

    pub fn start_cast(
        &self,
        to_niri: calloop::channel::Sender<ToNiriMsg>,
        gbm: GbmDevice<DrmDeviceFd>,
        session_id: usize,
        output: Output,
        cursor_mode: CursorMode,
        signal_ctx: SignalContext<'static>,
    ) -> anyhow::Result<Cast> {
        let _span = tracy_client::span!("PipeWire::start_cast");

        let stop_cast = move || {
            if let Err(err) = to_niri.send(ToNiriMsg::StopCast { session_id }) {
                warn!("error sending StopCast to niri: {err:?}");
            }
        };

        let mode = output.current_mode().unwrap();
        let size = mode.size;
        let refresh = mode.refresh;

        let stream = Stream::new(&self.core, "niri-screen-cast-src", Properties::new())
            .context("error creating Stream")?;

        // Like in good old wayland-rs times...
        let stream = Rc::new(stream);
        let node_id = Rc::new(Cell::new(None));
        let is_active = Rc::new(Cell::new(false));
        let min_time_between_frames = Rc::new(Cell::new(Duration::ZERO));
        let dmabufs = Rc::new(RefCell::new(HashMap::new()));

        let listener = stream
            .add_local_listener_with_user_data(())
            .state_changed({
                let stream = stream.clone();
                let is_active = is_active.clone();
                let stop_cast = stop_cast.clone();
                move |old, new| {
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
                        }
                    }
                }
            })
            .param_changed({
                let min_time_between_frames = min_time_between_frames.clone();
                move |stream, id, _data, pod| {
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
                        make_pod(&mut b1, o1).as_raw_ptr().cast_const(),
                        // make_pod(&mut b2, o2).as_raw_ptr().cast_const(),
                    ];
                    stream.update_params(&mut params).unwrap();
                }
            })
            .add_buffer({
                let dmabufs = dmabufs.clone();
                let stop_cast = stop_cast.clone();
                move |buffer| {
                    trace!("pw stream: add_buffer");

                    unsafe {
                        let spa_buffer = (*buffer).buffer;
                        let spa_data = (*spa_buffer).datas;
                        assert!((*spa_buffer).n_datas > 0);
                        assert!((*spa_data).type_ & (1 << DataType::DmaBuf.as_raw()) > 0);

                        let bo = match gbm.create_buffer_object::<()>(
                            size.w as u32,
                            size.h as u32,
                            Fourcc::Xrgb8888,
                            GbmBufferFlags::RENDERING | GbmBufferFlags::LINEAR,
                        ) {
                            Ok(bo) => bo,
                            Err(err) => {
                                warn!("error creating GBM buffer object: {err:?}");
                                stop_cast();
                                return;
                            }
                        };
                        let dmabuf = match bo.export() {
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
                }
            })
            .remove_buffer({
                let dmabufs = dmabufs.clone();
                move |buffer| {
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

        let object = pod::object!(
            SpaTypes::ObjectParamFormat,
            ParamType::EnumFormat,
            pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
            pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
            pod::property!(FormatProperties::VideoFormat, Id, VideoFormat::BGRx),
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
                    num: refresh as u32,
                    denom: 1000
                },
                Fraction { num: 1, denom: 1 },
                Fraction {
                    num: refresh as u32,
                    denom: 1000
                }
            ),
        );

        let mut buffer = vec![];
        let mut params = [make_pod(&mut buffer, object)];
        stream
            .connect(
                Direction::Output,
                None,
                StreamFlags::DRIVER | StreamFlags::ALLOC_BUFFERS,
                &mut params,
            )
            .context("error connecting stream")?;

        let cast = Cast {
            session_id,
            stream,
            _listener: listener,
            is_active,
            output,
            cursor_mode,
            last_frame_time: Duration::ZERO,
            min_time_between_frames,
            dmabufs,
        };
        Ok(cast)
    }
}

fn make_pod(buffer: &mut Vec<u8>, object: pod::Object) -> &Pod {
    PodSerializer::serialize(Cursor::new(&mut *buffer), &pod::Value::Object(object)).unwrap();
    Pod::from_bytes(buffer).unwrap()
}
