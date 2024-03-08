use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{io, mem};

use anyhow::{anyhow, Context};
use libc::dev_t;
use niri_config::Config;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::{Format, Fourcc};
use smithay::backend::drm::compositor::{DrmCompositor, PrimaryPlaneElement};
use smithay::backend::drm::{
    DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata, DrmEventTime, DrmNode, NodeType,
};
use smithay::backend::egl::context::ContextPriority;
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::{Capability, GlesRenderer};
use smithay::backend::renderer::multigpu::gbm::GbmGlesBackend;
use smithay::backend::renderer::multigpu::{GpuManager, MultiFrame, MultiRenderer};
use smithay::backend::renderer::{DebugFlags, ImportDma, ImportEgl, Renderer};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{self, UdevBackend, UdevEvent};
use smithay::desktop::utils::OutputPresentationFeedback;
use smithay::output::{Mode, Output, OutputModeSource, PhysicalProperties};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{Dispatcher, LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::{
    self, connector, crtc, property, Device, Mode as DrmMode, ModeFlags, ModeTypeFlags,
};
use smithay::reexports::gbm::Modifier;
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_protocols;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::DeviceFd;
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal};
use smithay::wayland::drm_lease::{
    DrmLease, DrmLeaseBuilder, DrmLeaseRequest, DrmLeaseState, LeaseRejected,
};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use smithay_drm_extras::edid::EdidInfo;
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags;
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;

use super::RenderResult;
use crate::frame_clock::FrameClock;
use crate::niri::{Niri, RedrawState, State};
use crate::render_helpers::renderer::AsGlesRenderer;
use crate::render_helpers::shaders;
use crate::utils::get_monotonic_time;

const SUPPORTED_COLOR_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Abgr8888];

pub struct Tty {
    config: Rc<RefCell<Config>>,
    session: LibSeatSession,
    udev_dispatcher: Dispatcher<'static, UdevBackend, State>,
    libinput: Libinput,
    gpu_manager: GpuManager<GbmGlesBackend<GlesRenderer, DrmDeviceFd>>,
    // DRM node corresponding to the primary GPU. May or may not be the same as
    // primary_render_node.
    primary_node: DrmNode,
    // DRM render node corresponding to the primary GPU.
    primary_render_node: DrmNode,
    // Devices indexed by DRM node (not necessarily the render node).
    devices: HashMap<DrmNode, OutputDevice>,
    // The dma-buf global corresponds to the output device (the primary GPU). It is only `Some()`
    // if we have a device corresponding to the primary GPU.
    dmabuf_global: Option<DmabufGlobal>,
    // The output config had changed, but the session is paused, so we need to update it on resume.
    update_output_config_on_resume: bool,
    // Whether the debug tinting is enabled.
    debug_tint: bool,
    ipc_outputs: Rc<RefCell<HashMap<String, niri_ipc::Output>>>,
    enabled_outputs: Arc<Mutex<HashMap<String, Output>>>,
}

pub type TtyRenderer<'render> = MultiRenderer<
    'render,
    'render,
    GbmGlesBackend<GlesRenderer, DrmDeviceFd>,
    GbmGlesBackend<GlesRenderer, DrmDeviceFd>,
>;

pub type TtyFrame<'render, 'frame> = MultiFrame<
    'render,
    'render,
    'frame,
    GbmGlesBackend<GlesRenderer, DrmDeviceFd>,
    GbmGlesBackend<GlesRenderer, DrmDeviceFd>,
>;

pub type TtyRendererError<'render> = <TtyRenderer<'render> as Renderer>::Error;

type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    (OutputPresentationFeedback, Duration),
    DrmDeviceFd,
>;

pub struct OutputDevice {
    token: RegistrationToken,
    render_node: DrmNode,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, Surface>,
    // SAFETY: drop after all the objects used with them are dropped.
    // See https://github.com/Smithay/smithay/issues/1102.
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,

    pub drm_lease_state: DrmLeaseState,
    non_desktop_connectors: HashSet<(connector::Handle, crtc::Handle)>,
    active_leases: Vec<DrmLease>,
}

impl OutputDevice {
    pub fn lease_request(
        &self,
        request: DrmLeaseRequest,
    ) -> Result<DrmLeaseBuilder, LeaseRejected> {
        let mut builder = DrmLeaseBuilder::new(&self.drm);
        for connector in request.connectors {
            let (_, crtc) = self
                .non_desktop_connectors
                .iter()
                .find(|(conn, _)| connector == *conn)
                .ok_or_else(|| {
                    warn!("Attempted to lease connector that is not non-desktop");
                    LeaseRejected::default()
                })?;
            builder.add_connector(connector);
            builder.add_crtc(*crtc);
            let planes = self.drm.planes(crtc).map_err(LeaseRejected::with_cause)?;
            builder.add_plane(planes.primary.handle);
        }
        Ok(builder)
    }

    pub fn new_lease(&mut self, lease: DrmLease) {
        self.active_leases.push(lease);
    }

    pub fn remove_lease(&mut self, lease_id: u32) {
        self.active_leases.retain(|l| l.id() != lease_id);
    }
}

#[derive(Debug, Clone, Copy)]
struct TtyOutputState {
    node: DrmNode,
    crtc: crtc::Handle,
}

struct Surface {
    name: String,
    compositor: GbmDrmCompositor,
    dmabuf_feedback: Option<SurfaceDmabufFeedback>,
    /// Tracy frame that goes from vblank to vblank.
    vblank_frame: Option<tracy_client::Frame>,
    /// Frame name for the VBlank frame.
    vblank_frame_name: tracy_client::FrameName,
    /// Plot name for the time since presentation plot.
    time_since_presentation_plot_name: tracy_client::PlotName,
    /// Plot name for the presentation misprediction plot.
    presentation_misprediction_plot_name: tracy_client::PlotName,
    sequence_delta_plot_name: tracy_client::PlotName,
}

pub struct SurfaceDmabufFeedback {
    pub render: DmabufFeedback,
    pub scanout: DmabufFeedback,
}

impl Tty {
    pub fn new(
        config: Rc<RefCell<Config>>,
        event_loop: LoopHandle<'static, State>,
    ) -> anyhow::Result<Self> {
        let (session, notifier) = LibSeatSession::new().context(
            "Error creating a session. This might mean that you're trying to run niri on a TTY \
             that is already busy, for example if you're running this inside tmux that had been \
             originally started on a different TTY",
        )?;
        let seat_name = session.seat();

        let udev_backend =
            UdevBackend::new(session.seat()).context("error creating a udev backend")?;
        let udev_dispatcher = Dispatcher::new(udev_backend, move |event, _, state: &mut State| {
            state.backend.tty().on_udev_event(&mut state.niri, event);
        });
        event_loop
            .register_dispatcher(udev_dispatcher.clone())
            .unwrap();

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput
            .udev_assign_seat(&seat_name)
            .map_err(|()| anyhow!("error assigning the seat to libinput"))?;

        let input_backend = LibinputInputBackend::new(libinput.clone());
        event_loop
            .insert_source(input_backend, |mut event, _, state| {
                state.process_libinput_event(&mut event);
                state.process_input_event(event);
            })
            .unwrap();

        event_loop
            .insert_source(notifier, move |event, _, state| {
                state.backend.tty().on_session_event(&mut state.niri, event);
            })
            .unwrap();

        let config_ = config.clone();
        let create_renderer = move |display: &EGLDisplay| {
            let color_transforms = config_
                .borrow()
                .debug
                .enable_color_transformations_capability;

            let egl_context = EGLContext::new_with_priority(display, ContextPriority::High)?;
            let gles = if color_transforms {
                unsafe { GlesRenderer::new(egl_context)? }
            } else {
                let capabilities = unsafe { GlesRenderer::supported_capabilities(&egl_context) }?
                    .into_iter()
                    .filter(|c| *c != Capability::ColorTransformations);
                unsafe { GlesRenderer::with_capabilities(egl_context, capabilities)? }
            };
            Ok(gles)
        };
        let api = GbmGlesBackend::with_factory(Box::new(create_renderer));
        let gpu_manager = GpuManager::new(api).context("error creating the GPU manager")?;

        let (primary_node, primary_render_node) = primary_node_from_config(&config.borrow())
            .ok_or(())
            .or_else(|()| {
                let primary_gpu_path = udev::primary_gpu(&seat_name)
                    .context("error getting the primary GPU")?
                    .context("couldn't find a GPU")?;
                let primary_node = DrmNode::from_path(primary_gpu_path)
                    .context("error opening the primary GPU DRM node")?;
                let primary_render_node = primary_node
                    .node_with_type(NodeType::Render)
                    .context("error getting the render node for the primary GPU")?
                    .context("error getting the render node for the primary GPU")?;

                Ok::<_, anyhow::Error>((primary_node, primary_render_node))
            })?;

        let mut node_path = String::new();
        if let Some(path) = primary_render_node.dev_path() {
            write!(node_path, "{:?}", path).unwrap();
        } else {
            write!(node_path, "{}", primary_render_node).unwrap();
        }
        info!("using as the render node: {}", node_path);

        Ok(Self {
            config,
            session,
            udev_dispatcher,
            libinput,
            gpu_manager,
            primary_node,
            primary_render_node,
            devices: HashMap::new(),
            dmabuf_global: None,
            update_output_config_on_resume: false,
            debug_tint: false,
            ipc_outputs: Rc::new(RefCell::new(HashMap::new())),
            enabled_outputs: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn init(&mut self, niri: &mut Niri) {
        for (device_id, path) in self.udev_dispatcher.clone().as_source_ref().device_list() {
            if let Err(err) = self.device_added(device_id, path, niri) {
                warn!("error adding device: {err:?}");
            }
        }
    }

    fn on_udev_event(&mut self, niri: &mut Niri, event: UdevEvent) {
        let _span = tracy_client::span!("Tty::on_udev_event");

        match event {
            UdevEvent::Added { device_id, path } => {
                if !self.session.is_active() {
                    debug!("skipping UdevEvent::Added as session is inactive");
                    return;
                }

                if let Err(err) = self.device_added(device_id, &path, niri) {
                    warn!("error adding device: {err:?}");
                }
            }
            UdevEvent::Changed { device_id } => {
                if !self.session.is_active() {
                    debug!("skipping UdevEvent::Changed as session is inactive");
                    return;
                }

                self.device_changed(device_id, niri)
            }
            UdevEvent::Removed { device_id } => {
                if !self.session.is_active() {
                    debug!("skipping UdevEvent::Removed as session is inactive");
                    return;
                }

                self.device_removed(device_id, niri)
            }
        }
    }

    fn on_session_event(&mut self, niri: &mut Niri, event: SessionEvent) {
        let _span = tracy_client::span!("Tty::on_session_event");

        match event {
            SessionEvent::PauseSession => {
                debug!("pausing session");

                self.libinput.suspend();

                for device in self.devices.values_mut() {
                    device.drm.pause();
                }
            }
            SessionEvent::ActivateSession => {
                debug!("resuming session");

                if self.libinput.resume().is_err() {
                    warn!("error resuming libinput");
                }

                let mut device_list = self
                    .udev_dispatcher
                    .as_source_ref()
                    .device_list()
                    .map(|(device_id, path)| (device_id, path.to_owned()))
                    .collect::<HashMap<_, _>>();

                let removed_devices = self
                    .devices
                    .keys()
                    .filter(|node| !device_list.contains_key(&node.dev_id()))
                    .copied()
                    .collect::<Vec<_>>();

                let remained_devices = self
                    .devices
                    .keys()
                    .filter(|node| device_list.contains_key(&node.dev_id()))
                    .copied()
                    .collect::<Vec<_>>();

                // Remove removed devices.
                for node in removed_devices {
                    device_list.remove(&node.dev_id());
                    self.device_removed(node.dev_id(), niri);
                }

                // Update remained devices.
                for node in remained_devices {
                    device_list.remove(&node.dev_id());

                    // It hasn't been removed, update its state as usual.
                    let device = self.devices.get_mut(&node).unwrap();
                    if let Err(err) = device.drm.activate(true) {
                        warn!("error activating DRM device: {err:?}");
                    }

                    // Refresh the connectors.
                    self.device_changed(node.dev_id(), niri);
                }

                // Add new devices.
                for (device_id, path) in device_list.into_iter() {
                    if let Err(err) = self.device_added(device_id, &path, niri) {
                        warn!("error adding device: {err:?}");
                    }
                }

                if self.update_output_config_on_resume {
                    self.on_output_config_changed(niri);
                }

                self.refresh_ipc_outputs();

                niri.idle_notifier_state.notify_activity(&niri.seat);
                niri.monitors_active = true;
                self.set_monitors_active(true);
                niri.queue_redraw_all();
            }
        }
    }

    fn device_added(
        &mut self,
        device_id: dev_t,
        path: &Path,
        niri: &mut Niri,
    ) -> anyhow::Result<()> {
        debug!("device added: {device_id} {path:?}");

        let node = DrmNode::from_dev_id(device_id)?;
        let drm_lease_state = DrmLeaseState::new::<State>(&niri.display_handle, &node)
            .context("Couldn't create DrmLeaseState")?;

        let open_flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK;
        let fd = self.session.open(path, open_flags)?;
        let device_fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm, drm_notifier) = DrmDevice::new(device_fd.clone(), true)?;
        let gbm = GbmDevice::new(device_fd)?;

        let display = unsafe { EGLDisplay::new(gbm.clone())? };
        let egl_device = EGLDevice::device_for_display(&display)?;

        let render_node = egl_device
            .try_get_render_node()?
            .context("no render node")?;
        self.gpu_manager
            .as_mut()
            .add_node(render_node, gbm.clone())
            .context("error adding render node to GPU manager")?;

        if node == self.primary_node {
            debug!("this is the primary node");

            let mut renderer = self
                .gpu_manager
                .single_renderer(&render_node)
                .context("error creating renderer")?;

            renderer.bind_wl_display(&niri.display_handle)?;

            shaders::init(renderer.as_gles_renderer());

            // Create the dmabuf global.
            let primary_formats = renderer.dmabuf_formats().collect::<HashSet<_>>();
            let default_feedback =
                DmabufFeedbackBuilder::new(render_node.dev_id(), primary_formats.clone())
                    .build()
                    .context("error building default dmabuf feedback")?;
            let dmabuf_global = niri
                .dmabuf_state
                .create_global_with_default_feedback::<State>(
                    &niri.display_handle,
                    &default_feedback,
                );
            assert!(self.dmabuf_global.replace(dmabuf_global).is_none());

            // Update the dmabuf feedbacks for all surfaces.
            for device in self.devices.values_mut() {
                for surface in device.surfaces.values_mut() {
                    match surface_dmabuf_feedback(
                        &surface.compositor,
                        primary_formats.clone(),
                        self.primary_render_node,
                        device.render_node,
                    ) {
                        Ok(feedback) => {
                            surface.dmabuf_feedback = Some(feedback);
                        }
                        Err(err) => {
                            warn!("error building dmabuf feedback: {err:?}");
                        }
                    }
                }
            }
        }

        let token = niri
            .event_loop
            .insert_source(drm_notifier, move |event, meta, state| {
                let tty = state.backend.tty();
                match event {
                    DrmEvent::VBlank(crtc) => {
                        let meta = meta.expect("VBlank events must have metadata");
                        tty.on_vblank(&mut state.niri, node, crtc, meta);
                    }
                    DrmEvent::Error(error) => warn!("DRM error: {error}"),
                };
            })
            .unwrap();

        let device = OutputDevice {
            token,
            render_node,
            drm,
            gbm,
            drm_scanner: DrmScanner::new(),
            surfaces: HashMap::new(),
            drm_lease_state,
            active_leases: Vec::new(),
            non_desktop_connectors: HashSet::new(),
        };
        assert!(self.devices.insert(node, device).is_none());

        self.device_changed(device_id, niri);

        Ok(())
    }

    fn device_changed(&mut self, device_id: dev_t, niri: &mut Niri) {
        debug!("device changed: {device_id}");

        let Ok(node) = DrmNode::from_dev_id(device_id) else {
            warn!("error creating DrmNode");
            return;
        };

        let Some(device) = self.devices.get_mut(&node) else {
            warn!("no such device");
            return;
        };

        for event in device.drm_scanner.scan_connectors(&device.drm) {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    if let Err(err) = self.connector_connected(niri, node, connector, crtc) {
                        warn!("error connecting connector: {err:?}");
                    }
                }
                DrmScanEvent::Disconnected {
                    crtc: Some(crtc), ..
                } => self.connector_disconnected(niri, node, crtc),
                _ => (),
            }
        }

        self.refresh_ipc_outputs();
    }

    fn device_removed(&mut self, device_id: dev_t, niri: &mut Niri) {
        debug!("device removed: {device_id}");

        let Ok(node) = DrmNode::from_dev_id(device_id) else {
            warn!("error creating DrmNode");
            return;
        };

        let Some(device) = self.devices.get_mut(&node) else {
            warn!("no such device");
            return;
        };

        let crtcs: Vec<_> = device
            .drm_scanner
            .crtcs()
            .map(|(_info, crtc)| crtc)
            .collect();

        for crtc in crtcs {
            self.connector_disconnected(niri, node, crtc);
        }

        let device = self.devices.remove(&node).unwrap();

        if node == self.primary_node {
            match self.gpu_manager.single_renderer(&device.render_node) {
                Ok(mut renderer) => renderer.unbind_wl_display(),
                Err(err) => {
                    warn!("error creating renderer during device removal: {err}");
                }
            }

            // Disable and destroy the dmabuf global.
            let global = self.dmabuf_global.take().unwrap();
            niri.dmabuf_state
                .disable_global::<State>(&niri.display_handle, &global);
            niri.event_loop
                .insert_source(
                    Timer::from_duration(Duration::from_secs(10)),
                    move |_, _, state| {
                        state
                            .niri
                            .dmabuf_state
                            .destroy_global::<State>(&state.niri.display_handle, global);
                        TimeoutAction::Drop
                    },
                )
                .unwrap();

            // Clear the dmabuf feedbacks for all surfaces.
            for device in self.devices.values_mut() {
                for surface in device.surfaces.values_mut() {
                    surface.dmabuf_feedback = None;
                }
            }
        }

        self.gpu_manager.as_mut().remove_node(&device.render_node);
        niri.event_loop.remove(device.token);

        self.refresh_ipc_outputs();
    }

    fn connector_connected(
        &mut self,
        niri: &mut Niri,
        node: DrmNode,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) -> anyhow::Result<()> {
        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id(),
        );
        debug!("connecting connector: {output_name}");

        let device = self.devices.get_mut(&node).context("missing device")?;

        let non_desktop = device
            .drm
            .get_properties(connector.handle())
            .ok()
            .and_then(|props| {
                let (info, value) = props
                    .into_iter()
                    .filter_map(|(handle, value)| {
                        let info = device.drm.get_property(handle).ok()?;
                        Some((info, value))
                    })
                    .find(|(info, _)| info.name().to_str() == Ok("non-desktop"))?;

                info.value_type().convert_value(value).as_boolean()
            })
            .unwrap_or(false);

        if non_desktop {
            debug!("output is non desktop");
            let description = get_edid_info(&device.drm, connector.handle())
                .map(|info| truncate_to_nul(info.model))
                .unwrap_or_else(|| "Unknown".into());
            device.drm_lease_state.add_connector::<State>(
                connector.handle(),
                output_name,
                description,
            );
            device
                .non_desktop_connectors
                .insert((connector.handle(), crtc));
            return Ok(());
        }

        let config = self
            .config
            .borrow()
            .outputs
            .iter()
            .find(|o| o.name == output_name)
            .cloned()
            .unwrap_or_default();

        if config.off {
            debug!("output is disabled in the config");
            return Ok(());
        }

        for m in connector.modes() {
            trace!("{m:?}");
        }

        let (mode, fallback) =
            pick_mode(&connector, config.mode).ok_or_else(|| anyhow!("no mode"))?;
        if fallback {
            let target = config.mode.unwrap();
            warn!(
                "configured mode {}x{}{} could not be found, falling back to preferred",
                target.width,
                target.height,
                if let Some(refresh) = target.refresh {
                    format!("@{refresh}")
                } else {
                    String::new()
                },
            );
        }
        debug!("picking mode: {mode:?}");

        let surface = device
            .drm
            .create_surface(crtc, mode, &[connector.handle()])?;

        // Create GBM allocator.
        let gbm_flags = GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT;
        let allocator = GbmAllocator::new(device.gbm.clone(), gbm_flags);

        // Update the output mode.
        let (physical_width, physical_height) = connector.size().unwrap_or((0, 0));

        let (make, model) = get_edid_info(&device.drm, connector.handle())
            .map(|info| {
                (
                    truncate_to_nul(info.manufacturer),
                    truncate_to_nul(info.model),
                )
            })
            .unwrap_or_else(|| ("Unknown".into(), "Unknown".into()));

        let output = Output::new(
            output_name.clone(),
            PhysicalProperties {
                size: (physical_width as i32, physical_height as i32).into(),
                subpixel: connector.subpixel().into(),
                model,
                make,
            },
        );

        let wl_mode = Mode::from(mode);
        output.change_current_state(Some(wl_mode), None, None, None);
        output.set_preferred(wl_mode);

        output
            .user_data()
            .insert_if_missing(|| TtyOutputState { node, crtc });

        let mut planes = surface.planes().clone();

        let config = self.config.borrow();

        // Overlay planes are disabled by default as they cause weird performance issues on my
        // system.
        if !config.debug.enable_overlay_planes {
            planes.overlay.clear();
        }

        // Cursor planes have bugs on some systems.
        let cursor_plane_gbm = if config.debug.disable_cursor_plane {
            None
        } else {
            Some(device.gbm.clone())
        };

        let renderer = self.gpu_manager.single_renderer(&device.render_node)?;
        let egl_context = renderer.as_ref().egl_context();
        let render_formats = egl_context.dmabuf_render_formats();

        // Create the compositor.
        let mut compositor = DrmCompositor::new(
            OutputModeSource::Auto(output.clone()),
            surface,
            Some(planes),
            allocator,
            device.gbm.clone(),
            SUPPORTED_COLOR_FORMATS,
            // This is only used to pick a good internal format, so it can use the surface's render
            // formats, even though we only ever render on the primary GPU.
            render_formats.clone(),
            device.drm.cursor_size(),
            cursor_plane_gbm,
        )?;
        if self.debug_tint {
            compositor.set_debug_flags(DebugFlags::TINT);
        }

        let mut dmabuf_feedback = None;
        if let Ok(primary_renderer) = self.gpu_manager.single_renderer(&self.primary_render_node) {
            let primary_formats = primary_renderer.dmabuf_formats().collect::<HashSet<_>>();

            match surface_dmabuf_feedback(
                &compositor,
                primary_formats,
                self.primary_render_node,
                device.render_node,
            ) {
                Ok(feedback) => {
                    dmabuf_feedback = Some(feedback);
                }
                Err(err) => {
                    warn!("error building dmabuf feedback: {err:?}");
                }
            }
        }

        let vblank_frame_name =
            tracy_client::FrameName::new_leak(format!("vblank on {output_name}"));
        let time_since_presentation_plot_name =
            tracy_client::PlotName::new_leak(format!("{output_name} time since presentation, ms"));
        let presentation_misprediction_plot_name = tracy_client::PlotName::new_leak(format!(
            "{output_name} presentation misprediction, ms"
        ));
        let sequence_delta_plot_name =
            tracy_client::PlotName::new_leak(format!("{output_name} sequence delta"));

        let surface = Surface {
            name: output_name.clone(),
            compositor,
            dmabuf_feedback,
            vblank_frame: None,
            vblank_frame_name,
            time_since_presentation_plot_name,
            presentation_misprediction_plot_name,
            sequence_delta_plot_name,
        };
        let res = device.surfaces.insert(crtc, surface);
        assert!(res.is_none(), "crtc must not have already existed");

        niri.add_output(output.clone(), Some(refresh_interval(mode)));

        self.enabled_outputs
            .lock()
            .unwrap()
            .insert(output_name, output.clone());
        #[cfg(feature = "dbus")]
        niri.on_enabled_outputs_changed();

        // Power on all monitors if necessary and queue a redraw on the new one.
        niri.event_loop.insert_idle(move |state| {
            state.niri.activate_monitors(&mut state.backend);
            state.niri.queue_redraw(output);
        });

        Ok(())
    }

    fn connector_disconnected(&mut self, niri: &mut Niri, node: DrmNode, crtc: crtc::Handle) {
        let Some(device) = self.devices.get_mut(&node) else {
            debug!("disconnecting connector for crtc: {crtc:?}");
            error!("missing device");
            return;
        };

        let Some(surface) = device.surfaces.remove(&crtc) else {
            debug!("disconnecting connector for crtc: {crtc:?}");
            debug!("crtc wasn't enabled");
            return;
        };

        debug!("disconnecting connector: {:?}", surface.name);

        let output = niri
            .global_space
            .outputs()
            .find(|output| {
                let tty_state: &TtyOutputState = output.user_data().get().unwrap();
                tty_state.node == node && tty_state.crtc == crtc
            })
            .cloned();
        if let Some(output) = output {
            niri.remove_output(&output);
        } else {
            error!("missing output for crtc {crtc:?}");
        };

        self.enabled_outputs.lock().unwrap().remove(&surface.name);
        #[cfg(feature = "dbus")]
        niri.on_enabled_outputs_changed();
    }

    fn on_vblank(
        &mut self,
        niri: &mut Niri,
        node: DrmNode,
        crtc: crtc::Handle,
        meta: DrmEventMetadata,
    ) {
        let span = tracy_client::span!("Tty::on_vblank");

        let now = get_monotonic_time();

        let Some(device) = self.devices.get_mut(&node) else {
            // I've seen it happen.
            error!("missing device in vblank callback for crtc {crtc:?}");
            return;
        };

        let Some(surface) = device.surfaces.get_mut(&crtc) else {
            error!("missing surface in vblank callback for crtc {crtc:?}");
            return;
        };

        // Finish the Tracy frame, if any.
        drop(surface.vblank_frame.take());

        let name = &surface.name;
        trace!("vblank on {name} {meta:?}");
        span.emit_text(name);

        let presentation_time = match meta.time {
            DrmEventTime::Monotonic(time) => time,
            DrmEventTime::Realtime(_) => {
                // Not supported.

                // This value will be ignored in the frame clock code.
                Duration::ZERO
            }
        };
        let presentation_time = if niri.config.borrow().debug.emulate_zero_presentation_time {
            Duration::ZERO
        } else {
            presentation_time
        };

        let message = if presentation_time.is_zero() {
            format!("vblank on {name}, presentation time unknown")
        } else if presentation_time > now {
            let diff = presentation_time - now;
            tracy_client::Client::running().unwrap().plot(
                surface.time_since_presentation_plot_name,
                -diff.as_secs_f64() * 1000.,
            );
            format!("vblank on {name}, presentation is {diff:?} later")
        } else {
            let diff = now - presentation_time;
            tracy_client::Client::running().unwrap().plot(
                surface.time_since_presentation_plot_name,
                diff.as_secs_f64() * 1000.,
            );
            format!("vblank on {name}, presentation was {diff:?} ago")
        };
        tracy_client::Client::running()
            .unwrap()
            .message(&message, 0);

        let Some(output) = niri
            .global_space
            .outputs()
            .find(|output| {
                let tty_state: &TtyOutputState = output.user_data().get().unwrap();
                tty_state.node == node && tty_state.crtc == crtc
            })
            .cloned()
        else {
            error!("missing output in global space for {name}");
            return;
        };

        let Some(output_state) = niri.output_state.get_mut(&output) else {
            error!("missing output state for {name}");
            return;
        };

        // Mark the last frame as submitted.
        match surface.compositor.frame_submitted() {
            Ok(Some((mut feedback, target_presentation_time))) => {
                let refresh = output_state
                    .frame_clock
                    .refresh_interval()
                    .unwrap_or(Duration::ZERO);
                // FIXME: ideally should be monotonically increasing for a surface.
                let seq = meta.sequence as u64;
                let mut flags = wp_presentation_feedback::Kind::Vsync
                    | wp_presentation_feedback::Kind::HwCompletion;

                let time = if presentation_time.is_zero() {
                    now
                } else {
                    flags.insert(wp_presentation_feedback::Kind::HwClock);
                    presentation_time
                };

                feedback.presented::<_, smithay::utils::Monotonic>(time, refresh, seq, flags);

                if !presentation_time.is_zero() {
                    let misprediction_s =
                        presentation_time.as_secs_f64() - target_presentation_time.as_secs_f64();
                    tracy_client::Client::running().unwrap().plot(
                        surface.presentation_misprediction_plot_name,
                        misprediction_s * 1000.,
                    );
                }
            }
            Ok(None) => (),
            Err(err) => {
                warn!("error marking frame as submitted: {err}");
            }
        }

        if let Some(last_sequence) = output_state.last_drm_sequence {
            let delta = meta.sequence as f64 - last_sequence as f64;
            tracy_client::Client::running()
                .unwrap()
                .plot(surface.sequence_delta_plot_name, delta);
        }
        output_state.last_drm_sequence = Some(meta.sequence);

        output_state.frame_clock.presented(presentation_time);

        let redraw_needed = match mem::replace(&mut output_state.redraw_state, RedrawState::Idle) {
            RedrawState::Idle => unreachable!(),
            RedrawState::Queued(_) => unreachable!(),
            RedrawState::WaitingForVBlank { redraw_needed } => redraw_needed,
            RedrawState::WaitingForEstimatedVBlank(_) => unreachable!(),
            RedrawState::WaitingForEstimatedVBlankAndQueued(_) => unreachable!(),
        };

        if redraw_needed || output_state.unfinished_animations_remain {
            let vblank_frame = tracy_client::Client::running()
                .unwrap()
                .non_continuous_frame(surface.vblank_frame_name);
            surface.vblank_frame = Some(vblank_frame);

            niri.queue_redraw(output);
        } else {
            niri.send_frame_callbacks(&output);
        }
    }

    fn on_estimated_vblank_timer(&self, niri: &mut Niri, output: Output) {
        let span = tracy_client::span!("Tty::on_estimated_vblank_timer");

        let name = output.name();
        span.emit_text(&name);

        let Some(output_state) = niri.output_state.get_mut(&output) else {
            error!("missing output state for {name}");
            return;
        };

        // We waited for the timer, now we can send frame callbacks again.
        output_state.frame_callback_sequence = output_state.frame_callback_sequence.wrapping_add(1);

        match mem::replace(&mut output_state.redraw_state, RedrawState::Idle) {
            RedrawState::Idle => unreachable!(),
            RedrawState::Queued(_) => unreachable!(),
            RedrawState::WaitingForVBlank { .. } => unreachable!(),
            RedrawState::WaitingForEstimatedVBlank(_) => (),
            // The timer fired just in front of a redraw.
            RedrawState::WaitingForEstimatedVBlankAndQueued((_, idle)) => {
                output_state.redraw_state = RedrawState::Queued(idle);
                return;
            }
        }

        if output_state.unfinished_animations_remain {
            niri.queue_redraw(output);
        } else {
            niri.send_frame_callbacks(&output);
        }
    }

    pub fn seat_name(&self) -> String {
        self.session.seat()
    }

    pub fn with_primary_renderer<T>(
        &mut self,
        f: impl FnOnce(&mut GlesRenderer) -> T,
    ) -> Option<T> {
        let mut renderer = self
            .gpu_manager
            .single_renderer(&self.primary_render_node)
            .ok()?;
        Some(f(renderer.as_gles_renderer()))
    }

    pub fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        target_presentation_time: Duration,
    ) -> RenderResult {
        let span = tracy_client::span!("Tty::render");

        let mut rv = RenderResult::Skipped;

        let tty_state: &TtyOutputState = output.user_data().get().unwrap();
        let Some(device) = self.devices.get_mut(&tty_state.node) else {
            error!("missing output device");
            return rv;
        };

        let Some(surface) = device.surfaces.get_mut(&tty_state.crtc) else {
            error!("missing surface");
            return rv;
        };

        span.emit_text(&surface.name);

        if !device.drm.is_active() {
            warn!("device is inactive");
            return rv;
        }

        let mut renderer = match self.gpu_manager.renderer(
            &self.primary_render_node,
            &device.render_node,
            surface.compositor.format(),
        ) {
            Ok(renderer) => renderer,
            Err(err) => {
                warn!("error creating renderer for primary GPU: {err:?}");
                return rv;
            }
        };

        // Render the elements.
        let elements = niri.render::<TtyRenderer>(&mut renderer, output, true);

        // Hand them over to the DRM.
        let drm_compositor = &mut surface.compositor;
        match drm_compositor.render_frame::<_, _>(&mut renderer, &elements, [0.; 4]) {
            Ok(res) => {
                if self
                    .config
                    .borrow()
                    .debug
                    .wait_for_frame_completion_before_queueing
                {
                    if let PrimaryPlaneElement::Swapchain(element) = res.primary_element {
                        let _span = tracy_client::span!("wait for completion");
                        element.sync.wait();
                    }
                }

                niri.update_primary_scanout_output(output, &res.states);
                if let Some(dmabuf_feedback) = surface.dmabuf_feedback.as_ref() {
                    niri.send_dmabuf_feedbacks(output, dmabuf_feedback, &res.states);
                }

                if !res.is_empty {
                    let presentation_feedbacks =
                        niri.take_presentation_feedbacks(output, &res.states);
                    let data = (presentation_feedbacks, target_presentation_time);

                    match drm_compositor.queue_frame(data) {
                        Ok(()) => {
                            let output_state = niri.output_state.get_mut(output).unwrap();
                            let new_state = RedrawState::WaitingForVBlank {
                                redraw_needed: false,
                            };
                            match mem::replace(&mut output_state.redraw_state, new_state) {
                                RedrawState::Idle => unreachable!(),
                                RedrawState::Queued(_) => (),
                                RedrawState::WaitingForVBlank { .. } => unreachable!(),
                                RedrawState::WaitingForEstimatedVBlank(_) => unreachable!(),
                                RedrawState::WaitingForEstimatedVBlankAndQueued((token, _)) => {
                                    niri.event_loop.remove(token);
                                }
                            };

                            // We queued this frame successfully, so the current client buffers were
                            // latched. We can send frame callbacks now, since a new client commit
                            // will no longer overwrite this frame and will wait for a VBlank.
                            output_state.frame_callback_sequence =
                                output_state.frame_callback_sequence.wrapping_add(1);

                            return RenderResult::Submitted;
                        }
                        Err(err) => {
                            warn!("error queueing frame: {err}");
                        }
                    }
                } else {
                    rv = RenderResult::NoDamage;
                }
            }
            Err(err) => {
                // Can fail if we switched to a different TTY.
                warn!("error rendering frame: {err}");
            }
        }

        // We're not expecting a vblank right after this.
        drop(surface.vblank_frame.take());

        // Queue a timer to fire at the predicted vblank time.
        queue_estimated_vblank_timer(niri, output.clone(), target_presentation_time);

        rv
    }

    pub fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            warn!("error changing VT: {err}");
        }
    }

    pub fn suspend(&self) {
        #[cfg(feature = "dbus")]
        if let Err(err) = suspend() {
            warn!("error suspending: {err:?}");
        }
    }

    pub fn toggle_debug_tint(&mut self) {
        self.debug_tint = !self.debug_tint;

        for device in self.devices.values_mut() {
            for surface in device.surfaces.values_mut() {
                let compositor = &mut surface.compositor;

                let mut flags = compositor.debug_flags();
                flags.set(DebugFlags::TINT, self.debug_tint);
                compositor.set_debug_flags(flags);
            }
        }
    }

    pub fn import_dmabuf(&mut self, dmabuf: &Dmabuf) -> bool {
        let mut renderer = match self.gpu_manager.single_renderer(&self.primary_render_node) {
            Ok(renderer) => renderer,
            Err(err) => {
                debug!("error creating renderer for primary GPU: {err:?}");
                return false;
            }
        };

        match renderer.import_dmabuf(dmabuf, None) {
            Ok(_texture) => {
                dmabuf.set_node(Some(self.primary_render_node));
                true
            }
            Err(err) => {
                debug!("error importing dmabuf: {err:?}");
                false
            }
        }
    }

    pub fn early_import(&mut self, surface: &WlSurface) {
        if let Err(err) = self.gpu_manager.early_import(
            // We always render on the primary GPU.
            self.primary_render_node,
            surface,
        ) {
            warn!("error doing early import: {err:?}");
        }
    }

    fn refresh_ipc_outputs(&self) {
        let _span = tracy_client::span!("Tty::refresh_ipc_outputs");

        let mut ipc_outputs = HashMap::new();

        for device in self.devices.values() {
            for (connector, crtc) in device.drm_scanner.crtcs() {
                let connector_name = format!(
                    "{}-{}",
                    connector.interface().as_str(),
                    connector.interface_id(),
                );

                let physical_size = connector.size();

                let (make, model) = get_edid_info(&device.drm, connector.handle())
                    .map(|info| {
                        (
                            truncate_to_nul(info.manufacturer),
                            truncate_to_nul(info.model),
                        )
                    })
                    .unwrap_or_else(|| ("Unknown".into(), "Unknown".into()));

                let surface = device.surfaces.get(&crtc);
                let current_crtc_mode = surface.map(|surface| surface.compositor.pending_mode());
                let mut current_mode = None;

                let modes = connector
                    .modes()
                    .iter()
                    .filter(|m| !m.flags().contains(ModeFlags::INTERLACE))
                    .enumerate()
                    .map(|(idx, m)| {
                        if Some(*m) == current_crtc_mode {
                            current_mode = Some(idx);
                        }

                        niri_ipc::Mode {
                            width: m.size().0,
                            height: m.size().1,
                            refresh_rate: Mode::from(*m).refresh as u32,
                        }
                    })
                    .collect();

                if let Some(crtc_mode) = current_crtc_mode {
                    if current_mode.is_none() {
                        if crtc_mode.flags().contains(ModeFlags::INTERLACE) {
                            warn!("connector mode list missing current mode (interlaced)");
                        } else {
                            error!("connector mode list missing current mode");
                        }
                    }
                }

                let output = niri_ipc::Output {
                    name: connector_name.clone(),
                    make,
                    model,
                    physical_size,
                    modes,
                    current_mode,
                };

                ipc_outputs.insert(connector_name, output);
            }
        }

        self.ipc_outputs.replace(ipc_outputs);
    }

    pub fn ipc_outputs(&self) -> Rc<RefCell<HashMap<String, niri_ipc::Output>>> {
        self.ipc_outputs.clone()
    }

    pub fn enabled_outputs(&self) -> Arc<Mutex<HashMap<String, Output>>> {
        self.enabled_outputs.clone()
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn primary_gbm_device(&self) -> Option<GbmDevice<DrmDeviceFd>> {
        self.devices.get(&self.primary_node).map(|d| d.gbm.clone())
    }

    pub fn set_monitors_active(&mut self, active: bool) {
        // We only disable the CRTC here, this will also reset the
        // surface state so that the next call to `render_frame` will
        // always produce a new frame and `queue_frame` will change
        // the CRTC to active. This makes sure we always enable a CRTC
        // within an atomic operation.
        if active {
            return;
        }

        for device in self.devices.values_mut() {
            for (crtc, surface) in device.surfaces.iter_mut() {
                set_crtc_active(&device.drm, *crtc, false);
                if let Err(err) = surface.compositor.reset_state() {
                    warn!("error resetting surface state: {err:?}");
                }
            }
        }
    }

    pub fn on_output_config_changed(&mut self, niri: &mut Niri) {
        let _span = tracy_client::span!("Tty::on_output_config_changed");

        // If we're inactive, we can't do anything, so just set a flag for later.
        if !self.session.is_active() {
            self.update_output_config_on_resume = true;
            return;
        }
        self.update_output_config_on_resume = false;

        let mut to_disconnect = vec![];
        let mut to_connect = vec![];

        for (&node, device) in &mut self.devices {
            for surface in device.surfaces.values_mut() {
                let crtc = surface.compositor.crtc();

                let config = self
                    .config
                    .borrow()
                    .outputs
                    .iter()
                    .find(|o| o.name == surface.name)
                    .cloned()
                    .unwrap_or_default();
                if config.off {
                    to_disconnect.push((node, crtc));
                    continue;
                }

                // Check if we need to change the mode.
                let Some(connector) = surface.compositor.pending_connectors().into_iter().next()
                else {
                    error!("surface pending connectors is empty");
                    continue;
                };
                let Some(connector) = device.drm_scanner.connectors().get(&connector) else {
                    error!("missing enabled connector in drm_scanner");
                    continue;
                };

                let Some((mode, fallback)) = pick_mode(connector, config.mode) else {
                    warn!("couldn't pick mode for enabled connector");
                    continue;
                };

                if surface.compositor.pending_mode() == mode {
                    continue;
                }

                let output = niri
                    .global_space
                    .outputs()
                    .find(|output| {
                        let tty_state: &TtyOutputState = output.user_data().get().unwrap();
                        tty_state.node == node && tty_state.crtc == crtc
                    })
                    .cloned();
                let Some(output) = output else {
                    error!("missing output for crtc: {crtc:?}");
                    continue;
                };
                let Some(output_state) = niri.output_state.get_mut(&output) else {
                    error!("missing state for output {:?}", surface.name);
                    continue;
                };

                if fallback {
                    let target = config.mode.unwrap();
                    warn!(
                        "output {:?}: configured mode {}x{}{} could not be found, \
                         falling back to preferred",
                        surface.name,
                        target.width,
                        target.height,
                        if let Some(refresh) = target.refresh {
                            format!("@{refresh}")
                        } else {
                            String::new()
                        },
                    );
                }

                debug!("output {:?}: picking mode: {mode:?}", surface.name);
                if let Err(err) = surface.compositor.use_mode(mode) {
                    warn!("error changing mode: {err:?}");
                    continue;
                }

                let wl_mode = Mode::from(mode);
                output.change_current_state(Some(wl_mode), None, None, None);
                output.set_preferred(wl_mode);
                output_state.frame_clock = FrameClock::new(Some(refresh_interval(mode)));
                niri.output_resized(output);
            }

            for (connector, crtc) in device.drm_scanner.crtcs() {
                // Check if connected.
                if connector.state() != connector::State::Connected {
                    continue;
                }

                // Check if already enabled.
                if device.surfaces.contains_key(&crtc) {
                    continue;
                }

                let output_name = format!(
                    "{}-{}",
                    connector.interface().as_str(),
                    connector.interface_id(),
                );

                let config = self
                    .config
                    .borrow()
                    .outputs
                    .iter()
                    .find(|o| o.name == output_name)
                    .cloned()
                    .unwrap_or_default();

                if !config.off {
                    to_connect.push((node, connector.clone(), crtc));
                }
            }
        }

        for (node, crtc) in to_disconnect {
            self.connector_disconnected(niri, node, crtc);
        }

        for (node, connector, crtc) in to_connect {
            if let Err(err) = self.connector_connected(niri, node, connector, crtc) {
                warn!("error connecting connector: {err:?}");
            }
        }

        self.refresh_ipc_outputs();
    }

    pub fn get_device_from_node(&mut self, node: DrmNode) -> Option<&mut OutputDevice> {
        self.devices.get_mut(&node)
    }
}

fn primary_node_from_config(config: &Config) -> Option<(DrmNode, DrmNode)> {
    let path = config.debug.render_drm_device.as_ref()?;
    debug!("attempting to use render node from config: {path:?}");

    match DrmNode::from_path(path) {
        Ok(node) => {
            if node.ty() == NodeType::Render {
                match node.node_with_type(NodeType::Primary) {
                    Some(Ok(primary_node)) => {
                        return Some((primary_node, node));
                    }
                    Some(Err(err)) => {
                        warn!("error opening primary node for render node {path:?}: {err:?}");
                    }
                    None => {
                        warn!("error opening primary node for render node {path:?}");
                    }
                }
            } else {
                warn!("DRM node {path:?} is not a render node");
            }
        }
        Err(err) => {
            warn!("error opening {path:?} as DRM node: {err:?}");
        }
    }
    None
}

fn surface_dmabuf_feedback(
    compositor: &GbmDrmCompositor,
    primary_formats: HashSet<Format>,
    primary_render_node: DrmNode,
    surface_render_node: DrmNode,
) -> Result<SurfaceDmabufFeedback, io::Error> {
    let surface = compositor.surface();
    let planes = surface.planes();

    let plane_formats = planes
        .primary
        .formats
        .iter()
        .chain(planes.overlay.iter().flat_map(|p| p.formats.iter()))
        .copied()
        .collect::<HashSet<_>>();

    // We limit the scan-out trache to formats we can also render from so that there is always a
    // fallback render path available in case the supplied buffer can not be scanned out directly.
    let mut scanout_formats = plane_formats
        .intersection(&primary_formats)
        .copied()
        .collect::<Vec<_>>();

    // HACK: AMD iGPU + dGPU systems share some modifiers between the two, and yet cross-device
    // buffers produce a glitched scanout if the modifier is not Linear...
    if primary_render_node != surface_render_node {
        scanout_formats.retain(|f| f.modifier == Modifier::Linear);
    }

    let builder = DmabufFeedbackBuilder::new(primary_render_node.dev_id(), primary_formats);

    let scanout = builder
        .clone()
        .add_preference_tranche(
            surface_render_node.dev_id(),
            Some(TrancheFlags::Scanout),
            scanout_formats,
        )
        .build()?;

    // If this is the primary node surface, send scanout formats in both tranches to avoid
    // duplication.
    let render = if primary_render_node == surface_render_node {
        scanout.clone()
    } else {
        builder.build()?
    };

    Ok(SurfaceDmabufFeedback { render, scanout })
}

fn find_drm_property(drm: &DrmDevice, crtc: crtc::Handle, name: &str) -> Option<property::Handle> {
    let props = match drm.get_properties(crtc) {
        Ok(props) => props,
        Err(err) => {
            warn!("error getting CRTC properties: {err:?}");
            return None;
        }
    };

    let (handles, _) = props.as_props_and_values();
    handles.iter().find_map(|handle| {
        let info = drm.get_property(*handle).ok()?;
        let n = info.name().to_str().ok()?;

        (n == name).then_some(*handle)
    })
}

fn set_crtc_active(drm: &DrmDevice, crtc: crtc::Handle, active: bool) {
    let Some(prop) = find_drm_property(drm, crtc, "ACTIVE") else {
        return;
    };

    let value = property::Value::Boolean(active);
    if let Err(err) = drm.set_property(crtc, prop, value.into()) {
        warn!("error setting CRTC property: {err:?}");
    }
}

fn refresh_interval(mode: DrmMode) -> Duration {
    let clock = mode.clock() as u64;
    let htotal = mode.hsync().2 as u64;
    let vtotal = mode.vsync().2 as u64;

    let mut numerator = htotal * vtotal * 1_000_000;
    let mut denominator = clock;

    if mode.flags().contains(ModeFlags::INTERLACE) {
        denominator *= 2;
    }

    if mode.flags().contains(ModeFlags::DBLSCAN) {
        numerator *= 2;
    }

    if mode.vscan() > 1 {
        numerator *= mode.vscan() as u64;
    }

    let refresh_interval = (numerator + denominator / 2) / denominator;
    Duration::from_nanos(refresh_interval)
}

#[cfg(feature = "dbus")]
fn suspend() -> anyhow::Result<()> {
    let conn = zbus::blocking::Connection::system().context("error connecting to system bus")?;

    conn.call_method(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1",
        Some("org.freedesktop.login1.Manager"),
        "Suspend",
        &(true),
    )
    .context("error suspending")?;

    Ok(())
}

fn queue_estimated_vblank_timer(
    niri: &mut Niri,
    output: Output,
    target_presentation_time: Duration,
) {
    let output_state = niri.output_state.get_mut(&output).unwrap();
    match mem::take(&mut output_state.redraw_state) {
        RedrawState::Idle => unreachable!(),
        RedrawState::Queued(_) => (),
        RedrawState::WaitingForVBlank { .. } => unreachable!(),
        RedrawState::WaitingForEstimatedVBlank(token)
        | RedrawState::WaitingForEstimatedVBlankAndQueued((token, _)) => {
            output_state.redraw_state = RedrawState::WaitingForEstimatedVBlank(token);
            return;
        }
    }

    let now = get_monotonic_time();
    let mut duration = target_presentation_time.saturating_sub(now);

    // No use setting a zero timer, since we'll send frame callbacks anyway right after the call to
    // render(). This can happen for example with unknown presentation time from DRM.
    if duration.is_zero() {
        duration += output_state
            .frame_clock
            .refresh_interval()
            // Unknown refresh interval, i.e. winit backend. Would be good to estimate it somehow
            // but it's not that important for this code path.
            .unwrap_or(Duration::from_micros(16_667));
    }

    trace!("queueing estimated vblank timer to fire in {duration:?}");

    let timer = Timer::from_duration(duration);
    let token = niri
        .event_loop
        .insert_source(timer, move |_, _, data| {
            data.backend
                .tty()
                .on_estimated_vblank_timer(&mut data.niri, output.clone());
            TimeoutAction::Drop
        })
        .unwrap();
    output_state.redraw_state = RedrawState::WaitingForEstimatedVBlank(token);
}

fn pick_mode(
    connector: &connector::Info,
    target: Option<niri_config::Mode>,
) -> Option<(control::Mode, bool)> {
    let mut mode = None;
    let mut fallback = false;

    if let Some(target) = target {
        let refresh = target.refresh.map(|r| (r * 1000.).round() as i32);

        for m in connector.modes() {
            if m.size() != (target.width, target.height) {
                continue;
            }

            // Interlaced modes don't appear to work.
            if m.flags().contains(ModeFlags::INTERLACE) {
                continue;
            }

            if let Some(refresh) = refresh {
                // If refresh is set, only pick modes with matching refresh.
                let wl_mode = Mode::from(*m);
                if wl_mode.refresh == refresh {
                    mode = Some(m);
                }
            } else if let Some(curr) = mode {
                // If refresh isn't set, pick the mode with the highest refresh.
                if curr.vrefresh() < m.vrefresh() {
                    mode = Some(m);
                }
            } else {
                mode = Some(m);
            }
        }

        if mode.is_none() {
            fallback = true;
        }
    }

    if mode.is_none() {
        // Pick a preferred mode.
        for m in connector.modes() {
            if !m.mode_type().contains(ModeTypeFlags::PREFERRED) {
                continue;
            }

            if let Some(curr) = mode {
                if curr.vrefresh() < m.vrefresh() {
                    mode = Some(m);
                }
            } else {
                mode = Some(m);
            }
        }
    }

    if mode.is_none() {
        // Last attempt.
        mode = connector.modes().first();
    }

    mode.map(|m| (*m, fallback))
}

fn truncate_to_nul(mut s: String) -> String {
    if let Some(index) = s.find('\0') {
        s.truncate(index);
    }
    s
}

fn get_edid_info(device: &DrmDevice, connector: connector::Handle) -> Option<EdidInfo> {
    match catch_unwind(AssertUnwindSafe(move || {
        EdidInfo::for_connector(device, connector)
    })) {
        Ok(info) => info,
        Err(err) => {
            warn!("edid-rs panicked: {err:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check(input: &str, expected: &str) {
        let input = String::from(input);
        assert_eq!(truncate_to_nul(input), expected);
    }

    #[test]
    fn truncate_to_nul_works() {
        check("", "");
        check("qwer", "qwer");
        check("abc\0def", "abc");
        check("\0as", "");
        check("a\0\0\0b", "a");
        check("bb😁\0cc", "bb😁");
    }
}
