use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{io, mem};

use anyhow::{anyhow, Context};
use libc::dev_t;
use niri_config::Config;
use smithay::backend::allocator::dmabuf::{Dmabuf, DmabufAllocator};
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::{Format, Fourcc};
use smithay::backend::drm::compositor::{DrmCompositor, PrimaryPlaneElement};
use smithay::backend::drm::{
    DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata, DrmEventTime, DrmNode, NodeType,
};
use smithay::backend::egl::context::ContextPriority;
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::{Capability, GlesRenderer, GlesTexture};
use smithay::backend::renderer::multigpu::gbm::GbmGlesBackend;
use smithay::backend::renderer::multigpu::{GpuManager, MultiFrame, MultiRenderer};
use smithay::backend::renderer::{DebugFlags, ImportDma, ImportEgl, Renderer};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{self, UdevBackend, UdevEvent};
use smithay::desktop::utils::OutputPresentationFeedback;
use smithay::output::{Mode, Output, OutputModeSource, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{Dispatcher, LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::{
    connector, crtc, property, Device, Mode as DrmMode, ModeFlags, ModeTypeFlags,
};
use smithay::reexports::gbm::Modifier;
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_protocols;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::DeviceFd;
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use smithay_drm_extras::edid::EdidInfo;
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags;
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;

use super::RenderResult;
use crate::niri::{RedrawState, State};
use crate::render_helpers::AsGlesRenderer;
use crate::utils::get_monotonic_time;
use crate::Niri;

const SUPPORTED_COLOR_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Abgr8888];

pub struct Tty {
    config: Rc<RefCell<Config>>,
    session: LibSeatSession,
    udev_dispatcher: Dispatcher<'static, UdevBackend, State>,
    libinput: Libinput,
    gpu_manager: GpuManager<GbmGlesBackend<GlesRenderer>>,
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
    // The allocator for the primary GPU. It is only `Some()` if we have a device corresponding to
    // the primary GPU.
    primary_allocator: Option<DmabufAllocator<GbmAllocator<DrmDeviceFd>>>,
    connectors: Arc<Mutex<HashMap<String, Output>>>,
}

pub type TtyRenderer<'render, 'alloc> = MultiRenderer<
    'render,
    'render,
    'alloc,
    GbmGlesBackend<GlesRenderer>,
    GbmGlesBackend<GlesRenderer>,
>;

pub type TtyFrame<'render, 'alloc, 'frame> = MultiFrame<
    'render,
    'render,
    'alloc,
    'frame,
    GbmGlesBackend<GlesRenderer>,
    GbmGlesBackend<GlesRenderer>,
>;

pub type TtyRendererError<'render, 'alloc> = <TtyRenderer<'render, 'alloc> as Renderer>::Error;

type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    (OutputPresentationFeedback, Duration),
    DrmDeviceFd,
>;

struct OutputDevice {
    token: RegistrationToken,
    render_node: DrmNode,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, Surface>,
    // SAFETY: drop after all the objects used with them are dropped.
    // See https://github.com/Smithay/smithay/issues/1102.
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
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
    pub fn new(config: Rc<RefCell<Config>>, event_loop: LoopHandle<'static, State>) -> Self {
        let (session, notifier) = LibSeatSession::new().unwrap();
        let seat_name = session.seat();

        let udev_backend = UdevBackend::new(session.seat()).unwrap();
        let udev_dispatcher = Dispatcher::new(udev_backend, move |event, _, state: &mut State| {
            state.backend.tty().on_udev_event(&mut state.niri, event);
        });
        event_loop
            .register_dispatcher(udev_dispatcher.clone())
            .unwrap();

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput.udev_assign_seat(&seat_name).unwrap();

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
        let gpu_manager = GpuManager::new(api).unwrap();

        let (primary_node, primary_render_node) = primary_node_from_config(&config.borrow())
            .unwrap_or_else(|| {
                let primary_gpu_path = udev::primary_gpu(&seat_name).unwrap().unwrap();
                let primary_node = DrmNode::from_path(primary_gpu_path).unwrap();
                let primary_render_node = primary_node
                    .node_with_type(NodeType::Render)
                    .unwrap()
                    .unwrap();
                (primary_node, primary_render_node)
            });

        let mut node_path = String::new();
        if let Some(path) = primary_render_node.dev_path() {
            write!(node_path, "{:?}", path).unwrap();
        } else {
            write!(node_path, "{}", primary_render_node).unwrap();
        }
        info!("using as the render node: {}", node_path);

        Self {
            config,
            session,
            udev_dispatcher,
            libinput,
            gpu_manager,
            primary_node,
            primary_render_node,
            devices: HashMap::new(),
            dmabuf_global: None,
            primary_allocator: None,
            connectors: Arc::new(Mutex::new(HashMap::new())),
        }
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

                for device in self.devices.values() {
                    device.drm.pause();
                }
            }
            SessionEvent::ActivateSession => {
                debug!("resuming session");

                if self.libinput.resume().is_err() {
                    error!("error resuming libinput");
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
                    let device = &self.devices[&node];
                    device.drm.activate();

                    // HACK: force reset the connectors to make resuming work across sleep.
                    let device = &self.devices[&node];
                    let crtcs: Vec<_> = device
                        .drm_scanner
                        .crtcs()
                        .map(|(conn, crtc)| (conn.clone(), crtc))
                        .collect();
                    for (conn, crtc) in crtcs {
                        self.connector_disconnected(niri, node, conn, crtc);
                    }

                    let device = self.devices.get_mut(&node).unwrap();
                    let _ = device.drm_scanner.scan_connectors(&device.drm);
                    let crtcs: Vec<_> = device
                        .drm_scanner
                        .crtcs()
                        .map(|(conn, crtc)| (conn.clone(), crtc))
                        .collect();
                    for (conn, crtc) in crtcs {
                        if let Err(err) = self.connector_connected(niri, node, conn, crtc) {
                            warn!("error connecting connector: {err:?}");
                        }
                    }

                    // // Refresh the connectors.
                    // self.device_changed(node.dev_id(), niri);

                    // // Refresh the state on unchanged connectors.
                    // let device = self.devices.get_mut(&node).unwrap();
                    // for surface in device.surfaces.values_mut() {
                    //     let compositor = &mut surface.compositor;
                    //     if let Err(err) = compositor.surface().reset_state() {
                    //         warn!("error resetting DRM surface state: {err}");
                    //     }
                    //     compositor.reset_buffers();
                    // }

                    // niri.queue_redraw_all();
                }

                // Add new devices.
                for (device_id, path) in device_list.into_iter() {
                    if let Err(err) = self.device_added(device_id, &path, niri) {
                        warn!("error adding device: {err:?}");
                    }
                }
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

        let open_flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK;
        let fd = self.session.open(path, open_flags)?;
        let device_fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm, drm_notifier) = DrmDevice::new(device_fd.clone(), true)?;
        let gbm = GbmDevice::new(device_fd)?;

        let display = EGLDisplay::new(gbm.clone())?;
        let egl_device = EGLDevice::device_for_display(&display)?;

        // HACK: There's an issue in Smithay where the display created by GpuManager will be the
        // same as the one we just created here, so when ours is dropped at the end of the scope,
        // it will also close the long-lived display in GpuManager. Thus, we need to drop ours
        // beforehand.
        drop(display);

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

            // Create the primary allocator.
            let primary_allocator =
                DmabufAllocator(GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING));
            assert!(self.primary_allocator.replace(primary_allocator).is_none());

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
                    DrmEvent::Error(error) => error!("DRM error: {error}"),
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
                    connector,
                    crtc: Some(crtc),
                } => self.connector_disconnected(niri, node, connector, crtc),
                _ => (),
            }
        }
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
            .map(|(info, crtc)| (info.clone(), crtc))
            .collect();

        for (connector, crtc) in crtcs {
            self.connector_disconnected(niri, node, connector, crtc);
        }

        let device = self.devices.remove(&node).unwrap();

        if node == self.primary_node {
            match self.gpu_manager.single_renderer(&device.render_node) {
                Ok(mut renderer) => renderer.unbind_wl_display(),
                Err(err) => {
                    error!("error creating renderer during device removal: {err}");
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

            self.primary_allocator = None;

            // Clear the dmabuf feedbacks for all surfaces.
            for device in self.devices.values_mut() {
                for surface in device.surfaces.values_mut() {
                    surface.dmabuf_feedback = None;
                }
            }
        }

        self.gpu_manager.as_mut().remove_node(&device.render_node);
        niri.event_loop.remove(device.token);
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

        let device = self.devices.get_mut(&node).context("missing device")?;

        // FIXME: print modes here until we have a better way to list all modes.
        for m in connector.modes() {
            let wl_mode = Mode::from(*m);
            debug!(
                "mode: {}x{}@{:.3}",
                m.size().0,
                m.size().1,
                wl_mode.refresh as f64 / 1000.,
            );

            trace!("{m:?}");
        }

        let mut mode = None;

        if let Some(target) = &config.mode {
            let refresh = target.refresh.map(|r| (r * 1000.).round() as i32);

            for m in connector.modes() {
                if m.size() != (target.width, target.height) {
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

        let mode = mode.ok_or_else(|| anyhow!("no mode"))?;
        debug!("picking mode: {mode:?}");

        let surface = device
            .drm
            .create_surface(crtc, *mode, &[connector.handle()])?;

        // Create GBM allocator.
        let gbm_flags = GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT;
        let allocator = GbmAllocator::new(device.gbm.clone(), gbm_flags);

        // Update the output mode.
        let (physical_width, physical_height) = connector.size().unwrap_or((0, 0));

        let (make, model) = EdidInfo::for_connector(&device.drm, connector.handle())
            .map(|info| (info.manufacturer, info.model))
            .unwrap_or_else(|| ("Unknown".into(), "Unknown".into()));

        let output = Output::new(
            output_name.clone(),
            PhysicalProperties {
                size: (physical_width as i32, physical_height as i32).into(),
                subpixel: Subpixel::Unknown,
                model,
                make,
            },
        );

        let wl_mode = Mode::from(*mode);
        let scale = config.scale.clamp(1., 10.).ceil() as i32;
        output.change_current_state(Some(wl_mode), None, Some(Scale::Integer(scale)), None);
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
        let compositor = DrmCompositor::new(
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

        self.connectors
            .lock()
            .unwrap()
            .insert(output_name.clone(), output.clone());

        let surface = Surface {
            name: output_name,
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

        niri.add_output(output.clone(), Some(refresh_interval(*mode)));

        // Power on all monitors if necessary and queue a redraw on the new one.
        niri.event_loop.insert_idle(move |state| {
            state.niri.activate_monitors(&state.backend);
            state.niri.queue_redraw(output);
        });

        Ok(())
    }

    fn connector_disconnected(
        &mut self,
        niri: &mut Niri,
        node: DrmNode,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        debug!("disconnecting connector: {connector:?}");

        let Some(device) = self.devices.get_mut(&node) else {
            error!("missing device");
            return;
        };

        let Some(surface) = device.surfaces.remove(&crtc) else {
            debug!("crtc wasn't enabled");
            return;
        };

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

        self.connectors.lock().unwrap().remove(&surface.name);
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
                let flags = wp_presentation_feedback::Kind::Vsync
                    | wp_presentation_feedback::Kind::HwClock
                    | wp_presentation_feedback::Kind::HwCompletion;

                feedback.presented::<_, smithay::utils::Monotonic>(
                    presentation_time,
                    refresh,
                    seq,
                    flags,
                );

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
                error!("error marking frame as submitted: {err}");
            }
        }

        if let Some(last_sequence) = output_state.current_estimated_sequence {
            let delta = meta.sequence as f64 - last_sequence as f64;
            tracy_client::Client::running()
                .unwrap()
                .plot(surface.sequence_delta_plot_name, delta);
        }

        output_state.frame_clock.presented(presentation_time);
        output_state.current_estimated_sequence = Some(meta.sequence);

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

        if let Some(sequence) = output_state.current_estimated_sequence.as_mut() {
            *sequence = sequence.wrapping_add(1);

            if output_state.unfinished_animations_remain {
                niri.queue_redraw(output);
            } else {
                niri.send_frame_callbacks(&output);
            }
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

        let Some(allocator) = self.primary_allocator.as_mut() else {
            warn!("no primary allocator");
            return rv;
        };

        let mut renderer = match self.gpu_manager.renderer(
            &self.primary_render_node,
            &device.render_node,
            allocator,
            surface.compositor.format(),
        ) {
            Ok(renderer) => renderer,
            Err(err) => {
                error!("error creating renderer for primary GPU: {err:?}");
                return rv;
            }
        };

        // Render the elements.
        let elements = niri.render::<TtyRenderer>(&mut renderer, output, true);

        // Hand them over to the DRM.
        let drm_compositor = &mut surface.compositor;
        match drm_compositor.render_frame::<_, _, GlesTexture>(&mut renderer, &elements, [0.; 4]) {
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

                if res.damage.is_some() {
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

                            return RenderResult::Submitted;
                        }
                        Err(err) => {
                            error!("error queueing frame: {err}");
                        }
                    }
                } else {
                    rv = RenderResult::NoDamage;
                }
            }
            Err(err) => {
                // Can fail if we switched to a different TTY.
                error!("error rendering frame: {err}");
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
            error!("error changing VT: {err}");
        }
    }

    pub fn suspend(&self) {
        #[cfg(feature = "dbus")]
        if let Err(err) = suspend() {
            warn!("error suspending: {err:?}");
        }
    }

    pub fn toggle_debug_tint(&mut self) {
        for device in self.devices.values_mut() {
            for surface in device.surfaces.values_mut() {
                let compositor = &mut surface.compositor;
                compositor.set_debug_flags(compositor.debug_flags() ^ DebugFlags::TINT);
            }
        }
    }

    pub fn import_dmabuf(&mut self, dmabuf: &Dmabuf) -> Result<(), ()> {
        let mut renderer = match self.gpu_manager.single_renderer(&self.primary_render_node) {
            Ok(renderer) => renderer,
            Err(err) => {
                debug!("error creating renderer for primary GPU: {err:?}");
                return Err(());
            }
        };

        match renderer.import_dmabuf(dmabuf, None) {
            Ok(_texture) => Ok(()),
            Err(err) => {
                debug!("error importing dmabuf: {err:?}");
                Err(())
            }
        }
    }

    pub fn early_import(&mut self, surface: &WlSurface) {
        if let Err(err) = self.gpu_manager.early_import(
            // We always advertise the primary GPU in dmabuf feedback.
            Some(self.primary_render_node),
            // We always render on the primary GPU.
            self.primary_render_node,
            surface,
        ) {
            warn!("error doing early import: {err:?}");
        }
    }

    pub fn connectors(&self) -> Arc<Mutex<HashMap<String, Output>>> {
        self.connectors.clone()
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn primary_gbm_device(&self) -> Option<GbmDevice<DrmDeviceFd>> {
        self.devices.get(&self.primary_node).map(|d| d.gbm.clone())
    }

    pub fn set_monitors_active(&self, active: bool) {
        for device in self.devices.values() {
            for crtc in device.surfaces.keys() {
                set_crtc_active(&device.drm, *crtc, active);
            }
        }
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
    let manager = logind_zbus::manager::ManagerProxyBlocking::new(&conn)
        .context("error creating login manager proxy")?;
    manager.suspend(true).context("error suspending")
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
    let timer = Timer::from_duration(target_presentation_time.saturating_sub(now));
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
