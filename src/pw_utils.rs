use std::cell::RefCell;
use std::{mem, ptr};
use std::collections::HashMap;
use std::io::Cursor;
use std::iter::zip;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{ensure};
use anyhow::Context as _;
use calloop::timer::{TimeoutAction, Timer};
use calloop::RegistrationToken;
use smithay::backend::renderer::ExportMem;
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
use smithay::backend::allocator::{Fourcc};
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::sync::SyncPoint;
use smithay::output::{Output, OutputModeSource};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::rustix;
use smithay::reexports::gbm::Modifier;
use smithay::utils::{Physical, Scale, Size, Transform};
use zbus::object_server::SignalEmitter;

use crate::dbus::mutter_screen_cast::{self, CursorMode};
use crate::niri::{CastTarget, State};
use crate::render_helpers::{clear_dmabuf, render_to_dmabuf, render_and_download};
use crate::utils::get_monotonic_time;

// Give a 0.1 ms allowance for presentation time errors.
const CAST_DELAY_ALLOWANCE: Duration = Duration::from_micros(100);
const SHM_BLOCKS: usize = 1;
const SHM_BYTES_PER_PIXEL: usize = 4;


pub struct PipeWire {
    _context: Context,
    pub core: Core,
    pub token: RegistrationToken,
    event_loop: LoopHandle<'static, State>,
    to_niri: calloop::channel::Sender<PwToNiri>,
}

pub enum PwToNiri {
    StopCast { session_id: usize },
    Redraw { stream_id: usize },
    FatalError,
}

pub struct Cast {
    event_loop: LoopHandle<'static, State>,
    pub session_id: usize,
    pub stream_id: usize,
    pub stream: Stream,
    _listener: StreamListener<()>,
    pub target: CastTarget,
    pub dynamic_target: bool,
    formats: FormatSet,
    offer_alpha: bool,
    pub cursor_mode: CursorMode,
    pub last_frame_time: Duration,
    scheduled_redraw: Option<RegistrationToken>,
    // Incremented once per successful frame, stored in buffer meta.
    sequence_counter: u64,
    inner: Rc<RefCell<CastInner>>,
}

/// Mutable `Cast` state shared with PipeWire callbacks.
#[derive(Debug)]
struct CastInner {
    is_active: bool,
    node_id: Option<u32>,
    state: CastState,
    refresh: u32,
    min_time_between_frames: Duration,
    dmabufs: HashMap<i64, Dmabuf>,
    shmbufs: HashMap<i64, Shmbuf>,
    /// Buffers dequeued from PipeWire in process of rendering.
    ///
    /// This is an ordered list of buffers that we started rendering to and waiting for the
    /// rendering to complete. The completion can be checked from the `SyncPoint`s. The buffers are
    /// stored in order from oldest to newest, and the same ordering should be preserved when
    /// submitting completed buffers to PipeWire.
    rendering_buffers: Vec<(NonNull<pw_buffer>, SyncPoint)>,
}

#[derive(Debug, Clone, Copy)]
struct DmaNegotiationResult {
    modifier: Modifier,
    plane_count: i32,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum CastState {
    // extra_negotiation_result = Some(_) means DMA sharing
    // extra_negotiation_result = None    means SHM sharing
    ResizePending {
        pending_size: Size<u32, Physical>,
    },
    ConfirmationPending {
        size: Size<u32, Physical>,
        alpha: bool,
        extra_negotiation_result: Option<DmaNegotiationResult>,
    },
    Ready {
        size: Size<u32, Physical>,
        alpha: bool,
        extra_negotiation_result: Option<DmaNegotiationResult>,
        // Lazily-initialized to keep the initialization to a single place.
        damage_tracker: Option<OutputDamageTracker>,
    },
}

#[derive(PartialEq, Eq)]
pub enum CastSizeChange {
    Ready,
    Pending,
}


fn make_video_params(
    video_formats: &Vec<VideoFormat>,
    modifiers: &Vec<Modifier>,
    size: Size<u32, Physical>,
    refresh: u32,
    fixated: bool,
) -> pod::Object {
    let modifier_property = if modifiers.len() == 0 {
        None
    } else {
        let dont_fixate = if (!fixated) && modifiers.len() == 1 && modifiers[0] == Modifier::Invalid
        {
            PropertyFlags::DONT_FIXATE
        } else {
            PropertyFlags::empty()
        };
        let flags = PropertyFlags::MANDATORY | dont_fixate;
        let modifiers_i64 = modifiers
            .iter()
            .map(|m| u64::from(*m) as i64)
            .collect::<Vec<_>>();
        Some(Property {
            key: FormatProperties::VideoModifier.as_raw(),
            flags,
            value: pod::Value::Choice(ChoiceValue::Long(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Enum {
                            default: modifiers_i64[0],
                            alternatives: modifiers_i64,
                        },
            ))),
        })
    };

    pipewire::spa::pod::Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: [
            vec![
                pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
                pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
            ],
            video_formats
                .iter()
                .map(|video_format| pod::property!(FormatProperties::VideoFormat, Id, video_format))
                .collect(),
                match modifier_property {
                    Some(prop) => vec![prop],
                    None => vec![],
                },
                vec![
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
                    ],
                    ]
                        .concat(),
    }
}

fn make_video_params_for_initial_negotiation(
    possible_modifiers: &FormatSet,
    size: Size<u32, Physical>,
    refresh: u32,
    alpha: bool,
) -> Vec<pod::Object> {
    let f = |alpha_| {
        let video_formats = if alpha_ {
            vec![VideoFormat::BGRA]
        } else {
            vec![VideoFormat::BGRx]
        };

        let fourcc = if alpha_ {
            Fourcc::Argb8888
        } else {
            Fourcc::Xrgb8888
        };

        let modifiers: Vec<_> = possible_modifiers
            .iter()
            .filter_map(|f| (f.code == fourcc).then_some(f.modifier))
            .collect();

        trace!("offering: {modifiers:?}");

        if modifiers.len() == 0 {
            vec![make_video_params(
                &video_formats,
                &vec![],
                size,
                refresh,
                false,
            )]
        } else {
            vec![
                make_video_params(&video_formats, &modifiers, size, refresh, false),
                make_video_params(&video_formats, &vec![], size, refresh, false),
            ]
        }
    };
    let pod_objects = if alpha {
        [f(true), f(false)].concat()
    } else {
        f(false)
    };
    pod_objects
}

macro_rules! make_video_params_for_initial_negotiation_macro {
    ($params:ident, $formats:expr, $size:expr, $refresh:expr, $alpha:expr) => {
        let $params = make_video_params_for_initial_negotiation($formats, $size, $refresh, $alpha);
        let mut obj_with_buffer: Vec<(&pod::Object, Vec<u8>)> =
            $params.iter().map(|obj| (obj, Vec::new())).collect();
        let $params: Vec<_> = obj_with_buffer
            .iter_mut()
            .map(|(obj, buf)| make_pod(buf, (*obj).clone()))
            .collect();
    };
}

impl PipeWire {
    pub fn new(
        event_loop: LoopHandle<'static, State>,
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
            event_loop,
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
                warn!(session_id, "error sending StopCast to niri: {err:?}");
            }
        };
        let to_niri_ = self.to_niri.clone();
        let redraw = move || {
            if let Err(err) = to_niri_.send(PwToNiri::Redraw { stream_id }) {
                warn!(stream_id, "error sending Redraw to niri: {err:?}");
            }
        };
        let redraw_ = redraw.clone();

        let stream = Stream::new(&self.core, "niri-screen-cast-src", Properties::new())
            .context("error creating Stream")?;

        let pending_size = Size::from((size.w as u32, size.h as u32));

        // Like in good old wayland-rs times...
        let inner = Rc::new(RefCell::new(CastInner {
            is_active: false,
            node_id: None,
            state: CastState::ResizePending { pending_size },
            refresh,
            min_time_between_frames: Duration::ZERO,
            dmabufs: HashMap::new(),
            shmbufs: HashMap::new(),
            rendering_buffers: Vec::new(),
        }));

        let listener = stream
            .add_local_listener_with_user_data(())
            .state_changed({
                let inner = inner.clone();
                let stop_cast = stop_cast.clone();
                move |stream, (), old, new| {
                    debug!(stream_id, "pw stream: state changed: {old:?} -> {new:?}");
                    let mut inner = inner.borrow_mut();

                    match new {
                        StreamState::Paused => {
                            if inner.node_id.is_none() {
                                let id = stream.node_id();
                                inner.node_id = Some(id);
                                debug!(stream_id, "pw stream: sending signal with {id}");

                                let _span = tracy_client::span!("sending PipeWireStreamAdded");
                                async_io::block_on(async {
                                    let res = mutter_screen_cast::Stream::pipe_wire_stream_added(
                                        &signal_ctx,
                                        id,
                                    )
                                    .await;

                                    if let Err(err) = res {
                                        warn!(
                                            stream_id,
                                            "error sending PipeWireStreamAdded: {err:?}"
                                        );
                                        stop_cast();
                                    }
                                });
                            }

                            inner.is_active = false;
                        }
                        StreamState::Error(_) => {
                            if inner.is_active {
                                inner.is_active = false;
                                stop_cast();
                            }
                        }
                        StreamState::Unconnected => (),
                        StreamState::Connecting => (),
                        StreamState::Streaming => {
                            inner.is_active = true;
                            redraw();
                        }
                    }
                }
            })
            .param_changed({
                let inner = inner.clone();
                let stop_cast = stop_cast.clone();
                let gbm = gbm.clone();
                let formats = formats.clone();
                move |stream, (), id, pod| {
                    let id = ParamType::from_raw(id);
                    trace!(stream_id, ?id, "pw stream: param_changed");
                    let mut inner = inner.borrow_mut();
                    let inner = &mut *inner;

                    if id != ParamType::Format {
                        return;
                    }

                    let Some(pod) = pod else { return };

                    let (m_type, m_subtype) = match parse_format(pod) {
                        Ok(x) => x,
                        Err(err) => {
                            warn!(stream_id, "pw stream: error parsing format: {err:?}");
                            return;
                        }
                    };

                    if m_type != MediaType::Video || m_subtype != MediaSubtype::Raw {
                        return;
                    }

                    let mut format = VideoInfoRaw::new();
                    format.parse(pod).unwrap();
                    debug!(stream_id, "pw stream: got format = {format:?}");

                    let format_size = Size::from((format.size().width, format.size().height));

                    let state = &mut inner.state;
                    if format_size != state.expected_format_size() {
                        if !matches!(&*state, CastState::ResizePending { .. }) {
                            warn!(stream_id, "pw stream: wrong size, but we're not resizing");
                            stop_cast();
                            return;
                        }

                        debug!(stream_id, "pw stream: wrong size, waiting");
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
                    inner.min_time_between_frames = min_frame_time;

                    // We have following cases when param_changed:
                    //
                    // 1. Modifier exists and its flags contain DONT_FIXATE
                    //
                    //    Do test allocation, set CastState to ConfirmationPending and send param
                    //    again.
                    //
                    // 2. Modifier exists and it doesn't need fixation
                    //
                    //    Do test allocation to ensure the modifier work, then set CastState to
                    //    Ready. Then set buffer to DMA.
                    //
                    // 3. Modifier doesn't exist
                    //
                    //    TODO: set CastState to Ready and set buffer to SHM.

                    let object = pod.as_object().unwrap();
                    let maybe_prop_modifier = object.find_prop(spa::utils::Id(FormatProperties::VideoModifier.0));


                    match maybe_prop_modifier {
                        Some(prop_modifier)
                            if prop_modifier.flags().contains(PodPropFlags::DONT_FIXATE) => {

                                debug!(stream_id, "pw stream: fixating the modifier");

                                let pod_modifier = prop_modifier.value();
                                let Ok((_, modifiers)) = PodDeserializer::deserialize_from::<Choice<i64>>(
                                    pod_modifier.as_bytes(),
                                ) else {
                                    warn!(stream_id, "pw stream: wrong modifier property type");
                                    stop_cast();
                                    return;
                                };

                                let ChoiceEnum::Enum { alternatives, .. } = modifiers.1 else {
                                    warn!(stream_id, "pw stream: wrong modifier choice type");
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
                                        warn!(
                                            stream_id,
                                            "pw stream: couldn't find preferred modifier: {err:?}"
                                        );
                                        stop_cast();
                                        return;
                                    }
                                };

                                debug!(
                                    stream_id,
                                    "pw stream: allocation successful \
                                    (modifier={modifier:?}, plane_count={plane_count}), \
                                    moving to confirmation pending"
                                );

                                *state = CastState::ConfirmationPending {
                                    size: format_size,
                                    alpha: format_has_alpha,
                                    extra_negotiation_result: Some(DmaNegotiationResult {
                                        modifier,
                                        plane_count: plane_count as i32,
                                    }),
                                };

                                let o = make_video_params(&vec![format.format()], &vec![modifier], format_size, refresh, true);
                                let mut b = Vec::new();
                                let pod = make_pod(&mut b, o);
                                let params_1 = vec![pod];

                                make_video_params_for_initial_negotiation_macro!(
                                    params_2,
                                    &formats,
                                    format_size,
                                    refresh,
                                    format_has_alpha
                                );


                                let params = [params_1, params_2].concat();

                                if let Err(err) = stream.update_params(params.clone().as_mut_slice()) {
                                    warn!(stream_id, "error updating stream params: {err:?}");
                                    stop_cast();
                                }
                            }
                        _ => {
                            let o1 = match maybe_prop_modifier {
                                Some(_) => {
                                    // Verify that alpha and modifier didn't change.
                                    let plane_count = match &*state {
                                        CastState::ConfirmationPending {
                                            size,
                                            alpha,
                                            extra_negotiation_result,
                                        }
                                        | CastState::Ready {
                                            size,
                                            alpha,
                                            extra_negotiation_result,
                                            ..
                                        } if *alpha == format_has_alpha
                                        && matches!(
                                            extra_negotiation_result, 
                                            Some(x) if x.modifier == Modifier::from(format.modifier())
                                        )  =>
                                            {
                                                let size = *size;
                                                let alpha = *alpha;
                                                let extra_negotiation_result = *extra_negotiation_result;

                                                let damage_tracker =
                                                    if let CastState::Ready { damage_tracker, .. } = &mut *state {
                                                        damage_tracker.take()
                                                    } else {
                                                        None
                                                    };

                                                debug!(stream_id, "pw stream: moving to ready state");

                                                *state = CastState::Ready {
                                                    size,
                                                    alpha,
                                                    extra_negotiation_result,
                                                    damage_tracker,
                                                };

                                                // Due to matches! guard this unwrap is safe
                                                extra_negotiation_result.unwrap().plane_count
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
                                                    warn!(stream_id, "pw stream: test allocation failed: {err:?}");
                                                    stop_cast();
                                                    return;
                                                }
                                            };

                                            debug!(
                                                stream_id,
                                                "pw stream: allocation successful \
                                                (modifier={modifier:?}, plane_count={plane_count}), \
                                                moving to ready"
                                            );

                                            *state = CastState::Ready {
                                                size: format_size,
                                                alpha: format_has_alpha,
                                                extra_negotiation_result: Some(DmaNegotiationResult {
                                                    modifier,
                                                    plane_count: plane_count as i32,
                                                }),
                                                damage_tracker: None,
                                            };

                                            plane_count as i32
                                        }
                                    };
                                    pod::object!(
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
                                    )
                                },
                                None => {
                                    *state = CastState::Ready {
                                        size: format_size,
                                        alpha: format_has_alpha,
                                        extra_negotiation_result: None,
                                        damage_tracker: None,
                                    };
                                    pod::object!(
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
                                        Property::new(
                                            SPA_PARAM_BUFFERS_blocks,
                                            pod::Value::Int(SHM_BLOCKS as i32),
                                        ),
                                        Property::new(
                                            SPA_PARAM_BUFFERS_dataType,
                                            pod::Value::Choice(ChoiceValue::Int(Choice(
                                                        ChoiceFlags::empty(),
                                                        ChoiceEnum::Flags {
                                                            default: 1 << DataType::MemFd.as_raw(),
                                                            flags: vec![1 << DataType::MemFd.as_raw()],
                                                        },
                                            ))),
                                        ),
                                        )

                                }

                            };
                            let mut b1 = vec![];

                            // const BPP: u32 = 4;
                            // let stride = format.size().width * BPP;
                            // let size = stride * format.size().height;

                            // FIXME: Hidden / embedded / metadata cursor

                            let o2 = pod::object!(
                                SpaTypes::ObjectParamMeta,
                                ParamType::Meta,
                                Property::new(
                                    SPA_PARAM_META_type,
                                    pod::Value::Id(spa::utils::Id(SPA_META_Header))
                                ),
                                Property::new(
                                    SPA_PARAM_META_size,
                                    pod::Value::Int(size_of::<spa_meta_header>() as i32)
                                ),
                            );
                            let mut b2 = vec![];

                            let mut params = [make_pod(&mut b1, o1), make_pod(&mut b2, o2)];

                            if let Err(err) = stream.update_params(&mut params) {
                                warn!(stream_id, "error updating stream params: {err:?}");
                                stop_cast();
                            }
                        }
                    }
                }
            })
            .add_buffer({
                let inner = inner.clone();
                let stop_cast = stop_cast.clone();
                move |stream, (), buffer| {
                    let mut inner = inner.borrow_mut();

                    match inner.state {
                        CastState::Ready {
                            size, 
                            alpha, 
                            extra_negotiation_result,
                            ..
                        } => {
                            match extra_negotiation_result {
                                Some(DmaNegotiationResult { modifier, .. }) => {
                                    trace!(
                                        stream_id,
                                        "pw stream: add_buffer (dma), size={size:?}, alpha={alpha}, \
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
                                                warn!(stream_id, "error allocating dmabuf: {err:?}");
                                                stop_cast();
                                                return;
                                            }
                                        };

                                        let plane_count = dmabuf.num_planes();
                                        assert_eq!((*spa_buffer).n_datas as usize, plane_count);

                                        for (i, (fd, (stride, offset))) in
                                            zip(dmabuf.handles(), zip(dmabuf.strides(), dmabuf.offsets()))
                                                .enumerate()
                                        {
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

                                            let chunk = (*spa_data).chunk;
                                            (*chunk).stride = stride as i32;
                                            (*chunk).offset = offset;

                                            trace!(
                                                stream_id,
                                                "pw buffer plane: fd={}, stride={stride}, offset={offset}",
                                                (*spa_data).fd
                                            );
                                        }

                                        let fd = (*(*spa_buffer).datas).fd;
                                        assert!(inner.dmabufs.insert(fd, dmabuf).is_none());
                                    }

                                    // During size re-negotiation, the stream sometimes just keeps running, in
                                    // which case we may need to force a redraw once we got a newly sized buffer.
                                    if inner.dmabufs.len() == 1 && stream.state() == StreamState::Streaming {
                                        redraw_();
                                    }

                                }
                                None => {
                                    trace!("pw stream: add_buffer (shm), size={size:?}, alpha={alpha}");
                                    unsafe {
                                        let spa_buffer = (*buffer).buffer;

                                        let shmbuf = match allocate_shmbuf(size) {
                                            Ok(x) => x,
                                            Err(err) => {
                                                warn!("error allocating shmbuf: {err:?}");
                                                stop_cast();
                                                return;
                                            }
                                        };

                                        assert_eq!((*spa_buffer).n_datas as usize, SHM_BLOCKS);

                                        let spa_data = (*spa_buffer).datas;
                                        assert!((*spa_data).type_ & (1 << DataType::MemFd.as_raw()) > 0);

                                        (*spa_data).type_ = DataType::MemFd.as_raw();
                                        (*spa_data).maxsize = shmbuf.size as u32;
                                        (*spa_data).fd = shmbuf.fd.as_raw_fd() as i64;
                                        (*spa_data).flags = SPA_DATA_FLAG_READWRITE;

                                        let fd = (*(*spa_buffer).datas).fd;
                                        assert!(inner.shmbufs.insert(fd, shmbuf).is_none());
                                    }
                                }
                            }
                        }
                        _ => {
                            trace!(stream_id, "pw stream: add buffer, but not ready yet");
                        }
                    }
                }
            })
            .remove_buffer({
                let inner = inner.clone();
                move |_stream, (), buffer| {
                    trace!(stream_id, "pw stream: remove_buffer");
                    let mut inner = inner.borrow_mut();

                    inner
                        .rendering_buffers
                        .retain(|(buf, _)| buf.as_ptr() != buffer);

                    unsafe {
                        let spa_buffer = (*buffer).buffer;
                        let spa_data = (*spa_buffer).datas;

                        if (*spa_data).type_ == DataType::DmaBuf.as_raw() {
                            trace!("pw stream: remove_buffer (dma)");
                            assert!((*spa_buffer).n_datas > 0);

                            let fd = (*spa_data).fd;
                            inner.dmabufs.remove(&fd);
                        } else if (*spa_data).type_ == DataType::MemFd.as_raw() {
                            trace!("pw stream: remove_buffer (shm)");
                            assert_eq!((*spa_buffer).n_datas, SHM_BLOCKS as u32);
                            let fd = (*spa_data).fd;
                            inner.shmbufs.remove(&fd);
                        } else {
                            warn!("pw stream: remove_buffer (unknown), impossible case happens, {:?}", (*spa_data).type_);
                        }
                    }
                }
            })
            .register()
            .unwrap();

        trace!(
            stream_id,
            "starting pw stream with size={pending_size:?}, refresh={refresh:?}"
        );

        make_video_params_for_initial_negotiation_macro!(params, &formats, pending_size, refresh, alpha);
        stream
            .connect(
                Direction::Output,
                None,
                StreamFlags::DRIVER | StreamFlags::ALLOC_BUFFERS,
                params.clone().as_mut_slice(),
            )
            .context("error connecting stream")?;

        let cast = Cast {
            event_loop: self.event_loop.clone(),
            session_id,
            stream_id,
            stream,
            _listener: listener,
            target,
            dynamic_target,
            formats,
            offer_alpha: alpha,
            cursor_mode,
            last_frame_time: Duration::ZERO,
            scheduled_redraw: None,
            sequence_counter: 0,
            inner,
        };
        Ok(cast)
    }
}

impl Cast {
    pub fn is_active(&self) -> bool {
        self.inner.borrow().is_active
    }

    pub fn ensure_size(&self, size: Size<i32, Physical>) -> anyhow::Result<CastSizeChange> {
        let mut inner = self.inner.borrow_mut();

        let new_size = Size::from((size.w as u32, size.h as u32));

        let state = &mut inner.state;
        if matches!(state, CastState::Ready { size, .. } if *size == new_size) {
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

        make_video_params_for_initial_negotiation_macro!(
            params,
            &self.formats,
            new_size,
            inner.refresh,
            self.offer_alpha
        );
        self.stream
            .update_params(params.clone().as_mut_slice())
            .context("error updating stream params")?;

        Ok(CastSizeChange::Pending)
    }

    pub fn set_refresh(&mut self, refresh: u32) -> anyhow::Result<()> {
        let mut inner = self.inner.borrow_mut();

        if inner.refresh == refresh {
            return Ok(());
        }

        let _span = tracy_client::span!("Cast::set_refresh");
        debug!("cast FPS changed, updating stream FPS");
        inner.refresh = refresh;

        let size = inner.state.expected_format_size();
        make_video_params_for_initial_negotiation_macro!(params, &self.formats, size, refresh, self.offer_alpha);
        self.stream
            .update_params(params.clone().as_mut_slice())
            .context("error updating stream params")?;

        Ok(())
    }

    fn compute_extra_delay(&self, target_frame_time: Duration) -> Duration {
        let inner = self.inner.borrow();

        let last = self.last_frame_time;
        let min = inner.min_time_between_frames;

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

    fn schedule_redraw(&mut self, output: Output, target_time: Duration) {
        if self.scheduled_redraw.is_some() {
            return;
        }

        let now = get_monotonic_time();
        let duration = target_time.saturating_sub(now);
        let timer = Timer::from_duration(duration);
        let token = self
            .event_loop
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

    fn remove_scheduled_redraw(&mut self) {
        if let Some(token) = self.scheduled_redraw.take() {
            self.event_loop.remove(token);
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
        output: &Output,
        target_frame_time: Duration,
    ) -> bool {
        let delay = self.compute_extra_delay(target_frame_time);
        if delay >= CAST_DELAY_ALLOWANCE {
            trace!("delay >= allowance, scheduling redraw");
            self.schedule_redraw(output.clone(), target_frame_time + delay);
            true
        } else {
            self.remove_scheduled_redraw();
            false
        }
    }

    fn dequeue_available_buffer(&mut self) -> Option<NonNull<pw_buffer>> {
        unsafe { NonNull::new(self.stream.dequeue_raw_buffer()) }
    }

    fn queue_completed_buffers(&mut self) {
        let mut inner = self.inner.borrow_mut();

        // We want to queue buffers in order, so find the first still-rendering buffer, and queue
        // everything up to that. Even if there are completed buffers past the first
        // still-rendering buffer, we do not want to queue them, since that would send frames out
        // of order.
        let first_in_progress_idx = inner
            .rendering_buffers
            .iter()
            .position(|(_, sync)| !sync.is_reached())
            .unwrap_or(inner.rendering_buffers.len());

        for (buffer, _) in inner.rendering_buffers.drain(..first_in_progress_idx) {
            trace!("queueing completed buffer");
            unsafe {
                pw_stream_queue_buffer(self.stream.as_raw_ptr(), buffer.as_ptr());
            }
        }
    }

    unsafe fn queue_after_sync(&mut self, pw_buffer: NonNull<pw_buffer>, sync_point: SyncPoint) {
        let _span = tracy_client::span!("Cast::queue_after_sync");

        let mut inner = self.inner.borrow_mut();

        let mut sync_point = sync_point;
        let sync_fd = match sync_point.export() {
            Some(sync_fd) => Some(sync_fd),
            None => {
                // There are two main ways this can happen. First is that the SyncPoint is
                // pre-signalled, then the buffer is already ready and no waiting is needed. Second
                // is that the SyncPoint is potentially still not signalled, but exporting a fence
                // fd had failed. In this case, there's not much we can do (perhaps do a blocking
                // wait for the SyncPoint, which itself might fail).
                //
                // So let's hope for the best and mark the buffer as submittable. We do not reuse
                // the original SyncPoint because if we do hit the second case (when it's not
                // signalled), then without a sync fd we cannot schedule a queue upon its
                // completion, effectively going stuck. It's better to queue an incomplete buffer
                // than getting stuck.
                sync_point = SyncPoint::signaled();
                None
            }
        };

        inner.rendering_buffers.push((pw_buffer, sync_point));
        drop(inner);

        match sync_fd {
            None => {
                trace!("sync_fd is None, queueing completed buffers");
                // In case this is the only buffer in the list, we will queue it right away.
                self.queue_completed_buffers();
            }
            Some(sync_fd) => {
                trace!("scheduling buffer to queue");
                let stream_id = self.stream_id;
                let source = Generic::new(sync_fd, Interest::READ, Mode::OneShot);
                self.event_loop
                    .insert_source(source, move |_, _, state| {
                        for cast in &mut state.niri.casts {
                            if cast.stream_id == stream_id {
                                cast.queue_completed_buffers();
                            }
                        }

                        Ok(PostAction::Remove)
                    })
                    .unwrap();
            }
        }
    }

    pub fn dequeue_buffer_and_render(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &[impl RenderElement<GlesRenderer>],
        size: Size<i32, Physical>,
        scale: Scale<f64>,
    ) -> bool {
        let mut inner = self.inner.borrow_mut();

        if let CastState::Ready {
            damage_tracker,
            extra_negotiation_result,
            alpha,
            ..
        } = &mut inner.state {
            let damage_tracker = damage_tracker
                .get_or_insert_with(|| OutputDamageTracker::new(size, scale, Transform::Normal));

            let extra_negotiation_result = extra_negotiation_result.clone();
            let alpha = alpha.clone();

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
            drop(inner);

            let Some(pw_buffer) = self.dequeue_available_buffer() else {
                warn!("no available buffer in pw stream, skipping frame");
                return false;
            };

            let buffer = pw_buffer.as_ptr();

            match extra_negotiation_result {
                Some(_) => {
                    unsafe {
                        let spa_buffer = (*buffer).buffer;

                        let fd = (*(*spa_buffer).datas).fd;
                        let dmabuf = self.inner.borrow().dmabufs[&fd].clone();

                        match render_to_dmabuf(
                            renderer,
                            dmabuf,
                            size,
                            scale,
                            Transform::Normal,
                            elements.iter().rev(),
                        ) {
                            Ok(sync_point) => {
                                mark_buffer_after_render(pw_buffer, &mut self.sequence_counter);
                                trace!("queueing buffer with seq={}", self.sequence_counter);
                                self.queue_after_sync(pw_buffer, sync_point);
                                true
                            }
                            Err(err) => {
                                warn!("error rendering to dmabuf: {err:?}");
                                return_unused_buffer(&self.stream, pw_buffer);
                                false
                            }
                        }
                    }
                },
                None => {
                    unsafe {
                        let spa_buffer = (*buffer).buffer;

                        let fd = (*(*spa_buffer).datas).fd;
                        let shmbuf = self.inner.borrow().shmbufs[&fd].clone();

                        let fourcc = if alpha {
                            Fourcc::Argb8888
                        } else {
                            Fourcc::Xrgb8888
                        };


                        match render_to_shmbuf(
                            renderer,
                            &shmbuf,
                            size,
                            scale,
                            Transform::Normal,
                            fourcc,
                            elements,
                        ) {
                            Ok(()) => {
                                mark_buffer_after_render(pw_buffer, &mut self.sequence_counter);
                                trace!("queueing buffer with seq={}", self.sequence_counter);
                                self.queue_after_sync(pw_buffer, SyncPoint::signaled());
                                true
                            }
                            Err(err) => {
                                warn!("error rendering to shmbuf: {err:?}");
                                return_unused_buffer(&self.stream, pw_buffer);
                                self.queue_after_sync(pw_buffer, SyncPoint::signaled());
                                false
                            }
                        }
                    }
                },
            }

        } else {
            error!("cast must be in Ready state to render");
            false
        }
    }

    pub fn dequeue_buffer_and_clear(&mut self, renderer: &mut GlesRenderer) -> bool {
        let mut inner = self.inner.borrow_mut();

        // Clear out the damage tracker if we're in Ready state.
        if let CastState::Ready { damage_tracker, .. } = &mut inner.state {
            *damage_tracker = None;
        };
        drop(inner);

        let Some(pw_buffer) = self.dequeue_available_buffer() else {
            warn!("no available buffer in pw stream, skipping frame");
            return false;
        };
        let buffer = pw_buffer.as_ptr();

        unsafe {
            if (*(*(*buffer).buffer).datas).type_ == DataType::DmaBuf.as_raw() {
                let spa_buffer = (*buffer).buffer;

                let fd = (*(*spa_buffer).datas).fd;
                let dmabuf = self.inner.borrow().dmabufs[&fd].clone();

                match clear_dmabuf(renderer, dmabuf) {
                    Ok(sync_point) => {
                        mark_buffer_after_render(pw_buffer, &mut self.sequence_counter);
                        trace!("queueing clear buffer with seq={}", self.sequence_counter);
                        self.queue_after_sync(pw_buffer, sync_point);
                        true
                    }
                    Err(err) => {
                        warn!("error clearing dmabuf: {err:?}");
                        return_unused_buffer(&self.stream, pw_buffer);
                        false
                    }
                }
            } else if (*(*(*buffer).buffer).datas).type_ == DataType::MemFd.as_raw() {
                let blocks = (*(*buffer).buffer).n_datas;

                if blocks as usize != SHM_BLOCKS {
                    warn!("expected {SHM_BLOCKS} blocks, got {blocks}");
                    return false;
                };

                let spa_buffer = (*buffer).buffer;

                let fd = (*(*spa_buffer).datas).fd;
                let shmbuf = self.inner.borrow().shmbufs[&fd].clone();

                match clear_shmbuf(&shmbuf) {
                    Ok (()) => {
                        mark_buffer_after_render(pw_buffer, &mut self.sequence_counter);
                        trace!("queueing clear buffer with seq={}", self.sequence_counter);
                        self.queue_after_sync(pw_buffer, SyncPoint::signaled());
                        true
                    }
                    Err(err) => {
                        warn!("error clearing shmbuf: {err:?}");
                        return_unused_buffer(&self.stream, pw_buffer);
                        false
                    }
                }
            } else {
                warn!("unknown data type in dequeue_buffer_and_clear");
                false
            }

        }
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

#[derive(Debug, Clone)]
pub struct Shmbuf {
    fd: Rc<smithay::reexports::rustix::fd::OwnedFd>,
    stride: usize,
    size: usize,
}

fn allocate_shmbuf(size: Size<u32, Physical>) -> anyhow::Result<Shmbuf> {
    let (w, h) = (size.w as usize, size.h as usize);
    let stride = w * SHM_BYTES_PER_PIXEL;
    let size = stride * h;
    let fd = smithay::reexports::rustix::fs::memfd_create(
        "shm_buffer",
        smithay::reexports::rustix::fs::MemfdFlags::CLOEXEC
            | smithay::reexports::rustix::fs::MemfdFlags::ALLOW_SEALING,
    )
    .context("error creating memfd")?;
    let _ = smithay::reexports::rustix::fs::ftruncate(&fd, size.try_into().unwrap())
        .context("error set size of the fd")?;
    let _ = smithay::reexports::rustix::fs::fcntl_add_seals(
        &fd,
        smithay::reexports::rustix::fs::SealFlags::SEAL
            | smithay::reexports::rustix::fs::SealFlags::SHRINK
            | smithay::reexports::rustix::fs::SealFlags::GROW,
    )
    .context("error sealing the fd")?;
    Ok(Shmbuf {
        fd: fd.into(),
        size,
        stride,
    })
}


unsafe fn return_unused_buffer(stream: &Stream, pw_buffer: NonNull<pw_buffer>) {
    // pw_stream_return_buffer() requires too new PipeWire (1.4.0). So, mark as
    // corrupted and queue.
    let pw_buffer = pw_buffer.as_ptr();
    let spa_buffer = (*pw_buffer).buffer;
    let chunk = (*(*spa_buffer).datas).chunk;
    // Some (older?) consumers will check for size == 0 instead of the CORRUPTED flag.
    (*chunk).size = 0;
    (*chunk).flags = SPA_CHUNK_FLAG_CORRUPTED as i32;

    if let Some(header) = find_meta_header(spa_buffer) {
        let header = header.as_ptr();
        (*header).flags = SPA_META_HEADER_FLAG_CORRUPTED;
    }

    pw_stream_queue_buffer(stream.as_raw_ptr(), pw_buffer);
}

unsafe fn mark_buffer_after_render(pw_buffer: NonNull<pw_buffer>, sequence: &mut u64) {
    let pw_buffer = pw_buffer.as_ptr();
    let spa_buffer = (*pw_buffer).buffer;
    let chunk = (*(*spa_buffer).datas).chunk;

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

    *sequence = sequence.wrapping_add(1);
    if let Some(header) = find_meta_header(spa_buffer) {
        let header = header.as_ptr();
        // Clear the corrupted flag we may have set before.
        (*header).flags = 0;
        (*header).seq = *sequence;
    }
}

unsafe fn find_meta_header(buffer: *mut spa_buffer) -> Option<NonNull<spa_meta_header>> {
    let p = spa_buffer_find_meta_data(buffer, SPA_META_Header, size_of::<spa_meta_header>()).cast();
    NonNull::new(p)
}

fn render_to_shmbuf(
    renderer: &mut GlesRenderer,
    buffer: &Shmbuf,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
    fourcc: Fourcc,
    elements: &[impl RenderElement<GlesRenderer>],
) -> anyhow::Result<()> {
    let expected_size =
        size.w as usize * size.h as usize * SHM_BYTES_PER_PIXEL as usize;
    ensure!(buffer.size == expected_size, "invalid buffer size");
    let mapping = render_and_download(
        renderer,
        size,
        scale,
        transform,
        fourcc,
        elements.iter().rev(),
    )?;
    let bytes = renderer
        .map_texture(&mapping)
        .context("error mapping texture")?;

    unsafe {
        let buf =  rustix::mm::mmap(
            std::ptr::null_mut(),
            buffer.size as usize,
            rustix::mm::ProtFlags::READ | rustix::mm::ProtFlags::WRITE,
            rustix::mm::MapFlags::SHARED,
            buffer.fd.clone(),
            0,
        )?;
        ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast(), buffer.size);
        let _ = rustix::mm::munmap(buf, buffer.size).unwrap();
    }
    Ok(())
}

fn clear_shmbuf(shmbuf: &Shmbuf) -> anyhow::Result<()> {
    let bytes: Vec<u8> = vec![0u8; shmbuf.size];            
    unsafe {
        let buf = rustix::mm::mmap(
                std::ptr::null_mut(),
                shmbuf.size as usize,
                rustix::mm::ProtFlags::READ | rustix::mm::ProtFlags::WRITE,
                rustix::mm::MapFlags::SHARED,
                shmbuf.fd.clone(),
                0,
            )?;
        ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast(), shmbuf.size);
        let _ = rustix::mm::munmap(buf, shmbuf.size).unwrap();
    }
    Ok(())
}
