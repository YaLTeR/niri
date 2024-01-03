use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context};
use libc::dev_t;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::compositor::{DrmCompositor, PrimaryPlaneElement};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata, DrmEventTime};
use smithay::backend::egl::context::ContextPriority;
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::{Capability, GlesRenderer, GlesTexture};
use smithay::backend::renderer::{DebugFlags, ImportDma, ImportEgl};
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
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_protocols;
use smithay::utils::DeviceFd;
use smithay::wayland::dmabuf::{DmabufFeedback, DmabufFeedbackBuilder, DmabufGlobal};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use smithay_drm_extras::edid::EdidInfo;
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags;
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;

use super::RenderResult;
use crate::config::Config;
use crate::niri::{RedrawState, State};
use crate::utils::get_monotonic_time;
use crate::Niri;

const SUPPORTED_COLOR_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Abgr8888];

pub struct Tty {
    config: Rc<RefCell<Config>>,
    session: LibSeatSession,
    udev_dispatcher: Dispatcher<'static, UdevBackend, State>,
    libinput: Libinput,
    primary_gpu_path: PathBuf,
    // The dma-buf global corresponds to the output device (the primary GPU). It is only `Some()`
    // if we have a device corresponding to the primary GPU.
    dmabuf_global: Option<DmabufGlobal>,
    output_device: Option<OutputDevice>,
    connectors: Arc<Mutex<HashMap<String, Output>>>,
}

type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    (OutputPresentationFeedback, Duration),
    DrmDeviceFd,
>;

struct OutputDevice {
    id: dev_t,
    token: RegistrationToken,
    gles: GlesRenderer,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, Surface>,
    // SAFETY: drop after all the objects used with them are dropped.
    // See https://github.com/Smithay/smithay/issues/1102.
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
}

#[derive(Debug, Clone, Copy)]
struct TtyOutputState {
    device_id: dev_t,
    crtc: crtc::Handle,
}

struct Surface {
    name: String,
    compositor: GbmDrmCompositor,
    dmabuf_feedback: DmabufFeedback,
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

        let primary_gpu_path = udev::primary_gpu(&seat_name).unwrap().unwrap();

        Self {
            config,
            session,
            udev_dispatcher,
            libinput,
            primary_gpu_path,
            dmabuf_global: None,
            output_device: None,
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

                if let Some(output_device) = &self.output_device {
                    output_device.drm.pause();
                }
            }
            SessionEvent::ActivateSession => {
                debug!("resuming session");

                if self.libinput.resume().is_err() {
                    error!("error resuming libinput");
                }

                if let Some(output_device) = &mut self.output_device {
                    // We had an output device, check if it's been removed.
                    let output_device_id = output_device.id;
                    if !self
                        .udev_dispatcher
                        .as_source_ref()
                        .device_list()
                        .any(|(device_id, _)| device_id == output_device_id)
                    {
                        // The output device, if we had any, has been removed.
                        self.device_removed(output_device_id, niri);
                    } else {
                        // It hasn't been removed, update its state as usual.
                        output_device.drm.activate();

                        // HACK: force reset the connectors to make resuming work across
                        // sleep.
                        let output_device = self.output_device.as_mut().unwrap();
                        let crtcs: Vec<_> = output_device
                            .drm_scanner
                            .crtcs()
                            .map(|(conn, crtc)| (conn.clone(), crtc))
                            .collect();
                        for (conn, crtc) in crtcs {
                            self.connector_disconnected(niri, conn, crtc);
                        }

                        let output_device = self.output_device.as_mut().unwrap();
                        let _ = output_device
                            .drm_scanner
                            .scan_connectors(&output_device.drm);
                        let crtcs: Vec<_> = output_device
                            .drm_scanner
                            .crtcs()
                            .map(|(conn, crtc)| (conn.clone(), crtc))
                            .collect();
                        for (conn, crtc) in crtcs {
                            if let Err(err) = self.connector_connected(niri, conn, crtc) {
                                warn!("error connecting connector: {err:?}");
                            }
                        }

                        // // Refresh the connectors.
                        // self.device_changed(output_device_id, niri);

                        // // Refresh the state on unchanged connectors.
                        // let output_device = self.output_device.as_mut().unwrap();
                        // for drm_compositor in output_device.surfaces.values_mut() {
                        //     if let Err(err) = drm_compositor.surface().reset_state() {
                        //         warn!("error resetting DRM surface state: {err}");
                        //     }
                        //     drm_compositor.reset_buffers();
                        // }

                        // niri.queue_redraw_all();
                    }
                } else {
                    // We didn't have an output device, check if it's been added.
                    let udev_dispatcher = self.udev_dispatcher.clone();
                    for (device_id, path) in udev_dispatcher.as_source_ref().device_list() {
                        if let Err(err) = self.device_added(device_id, path, niri) {
                            warn!("error adding device: {err:?}");
                        }
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
        if path != self.primary_gpu_path {
            debug!("skipping non-primary device {path:?}");
            return Ok(());
        }

        debug!("adding device {path:?}");
        assert!(self.output_device.is_none());

        let open_flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK;
        let fd = self.session.open(path, open_flags)?;
        let device_fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm, drm_notifier) = DrmDevice::new(device_fd.clone(), true)?;
        let gbm = GbmDevice::new(device_fd)?;

        let display = EGLDisplay::new(gbm.clone())?;
        let egl_context = EGLContext::new_with_priority(&display, ContextPriority::High)?;

        // ColorTransformations is disabled by default as it makes rendering slightly slower.
        let mut gles = if self
            .config
            .borrow()
            .debug
            .enable_color_transformations_capability
        {
            unsafe { GlesRenderer::new(egl_context)? }
        } else {
            let capabilities = unsafe { GlesRenderer::supported_capabilities(&egl_context) }?
                .into_iter()
                .filter(|c| *c != Capability::ColorTransformations);
            unsafe { GlesRenderer::with_capabilities(egl_context, capabilities)? }
        };

        gles.bind_wl_display(&niri.display_handle)?;

        let token = niri
            .event_loop
            .insert_source(drm_notifier, move |event, meta, state| {
                let tty = state.backend.tty();
                match event {
                    DrmEvent::VBlank(crtc) => {
                        let meta = meta.expect("VBlank events must have metadata");
                        tty.on_vblank(&mut state.niri, crtc, meta);
                    }
                    DrmEvent::Error(error) => error!("DRM error: {error}"),
                };
            })
            .unwrap();

        let default_feedback = DmabufFeedbackBuilder::new(device_id, gles.dmabuf_formats())
            .build()
            .context("error building default dmabuf feedback")?;
        let dmabuf_global = niri
            .dmabuf_state
            .create_global_with_default_feedback::<State>(&niri.display_handle, &default_feedback);

        self.output_device = Some(OutputDevice {
            id: device_id,
            token,
            drm,
            gbm,
            gles,
            drm_scanner: DrmScanner::new(),
            surfaces: HashMap::new(),
        });
        self.dmabuf_global = Some(dmabuf_global);

        self.device_changed(device_id, niri);

        Ok(())
    }

    fn device_changed(&mut self, device_id: dev_t, niri: &mut Niri) {
        let Some(device) = &mut self.output_device else {
            return;
        };
        if device.id != device_id {
            return;
        }
        debug!("output device changed");

        for event in device.drm_scanner.scan_connectors(&device.drm) {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    if let Err(err) = self.connector_connected(niri, connector, crtc) {
                        warn!("error connecting connector: {err:?}");
                    }
                }
                DrmScanEvent::Disconnected {
                    connector,
                    crtc: Some(crtc),
                } => self.connector_disconnected(niri, connector, crtc),
                _ => (),
            }
        }
    }

    fn device_removed(&mut self, device_id: dev_t, niri: &mut Niri) {
        let Some(device) = &mut self.output_device else {
            return;
        };
        if device_id != device.id {
            return;
        }

        let crtcs: Vec<_> = device
            .drm_scanner
            .crtcs()
            .map(|(info, crtc)| (info.clone(), crtc))
            .collect();

        for (connector, crtc) in crtcs {
            self.connector_disconnected(niri, connector, crtc);
        }

        let mut device = self.output_device.take().unwrap();
        device.gles.unbind_wl_display();

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

        niri.event_loop.remove(device.token);
    }

    fn connector_connected(
        &mut self,
        niri: &mut Niri,
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

        let device = self
            .output_device
            .as_mut()
            .context("missing output device")?;

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

        output.user_data().insert_if_missing(|| TtyOutputState {
            device_id: device.id,
            crtc,
        });

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

        let egl_context = device.gles.egl_context();
        let texture_formats = egl_context.dmabuf_texture_formats();
        let render_formats = egl_context.dmabuf_render_formats();

        let scanout_formats = planes
            .primary
            .formats
            .iter()
            .chain(planes.overlay.iter().flat_map(|p| &p.formats))
            .copied()
            .collect::<HashSet<_>>();
        let scanout_formats = scanout_formats.intersection(texture_formats).copied();

        // Create the compositor.
        let compositor = DrmCompositor::new(
            OutputModeSource::Auto(output.clone()),
            surface,
            Some(planes),
            allocator,
            device.gbm.clone(),
            SUPPORTED_COLOR_FORMATS,
            render_formats.clone(),
            device.drm.cursor_size(),
            cursor_plane_gbm,
        )?;

        let dmabuf_feedback = DmabufFeedbackBuilder::new(device.id, texture_formats.clone())
            .add_preference_tranche(device.id, Some(TrancheFlags::Scanout), scanout_formats)
            .build()
            .context("error building dmabuf feedback")?;

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
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        debug!("disconnecting connector: {connector:?}");
        let Some(device) = self.output_device.as_mut() else {
            error!("missing output device");
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
                tty_state.device_id == device.id && tty_state.crtc == crtc
            })
            .cloned();
        if let Some(output) = output {
            niri.remove_output(&output);
        } else {
            error!("missing output for crtc {crtc:?}");
        };

        self.connectors.lock().unwrap().remove(&surface.name);
    }

    fn on_vblank(&mut self, niri: &mut Niri, crtc: crtc::Handle, meta: DrmEventMetadata) {
        let span = tracy_client::span!("Tty::on_vblank");

        let now = get_monotonic_time();

        let Some(device) = self.output_device.as_mut() else {
            // I've seen it happen.
            error!("missing output device in vblank callback for crtc {crtc:?}");
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
                tty_state.device_id == device.id && tty_state.crtc == crtc
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

    pub fn renderer(&mut self) -> Option<&mut GlesRenderer> {
        self.output_device.as_mut().map(|d| &mut d.gles)
    }

    pub fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        target_presentation_time: Duration,
    ) -> RenderResult {
        let span = tracy_client::span!("Tty::render");

        let mut rv = RenderResult::Skipped;

        let Some(device) = self.output_device.as_mut() else {
            error!("missing output device");
            return rv;
        };

        if !device.drm.is_active() {
            warn!("device is inactive");
            return rv;
        }

        let tty_state: &TtyOutputState = output.user_data().get().unwrap();
        let Some(surface) = device.surfaces.get_mut(&tty_state.crtc) else {
            error!("missing surface");
            return rv;
        };

        span.emit_text(&surface.name);

        let renderer = &mut device.gles;

        // Render the elements.
        let elements = niri.render(renderer, output, true);

        // Hand them over to the DRM.
        let drm_compositor = &mut surface.compositor;
        match drm_compositor.render_frame::<_, _, GlesTexture>(renderer, &elements, [0.; 4]) {
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
                niri.send_dmabuf_feedbacks(output, &surface.dmabuf_feedback);

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
        if let Some(device) = self.output_device.as_mut() {
            for surface in device.surfaces.values_mut() {
                let compositor = &mut surface.compositor;
                compositor.set_debug_flags(compositor.debug_flags() ^ DebugFlags::TINT);
            }
        }
    }

    pub fn import_dmabuf(&mut self, dmabuf: &Dmabuf) -> Result<(), ()> {
        let device = self.output_device.as_mut().ok_or(())?;
        match device.gles.import_dmabuf(dmabuf, None) {
            Ok(_texture) => Ok(()),
            Err(err) => {
                debug!("error importing dmabuf: {err:?}");
                Err(())
            }
        }
    }

    pub fn connectors(&self) -> Arc<Mutex<HashMap<String, Output>>> {
        self.connectors.clone()
    }

    #[cfg(feature = "xdp-gnome-screencast")]
    pub fn gbm_device(&self) -> Option<GbmDevice<DrmDeviceFd>> {
        self.output_device.as_ref().map(|d| d.gbm.clone())
    }

    pub fn set_monitors_active(&self, active: bool) {
        let Some(device) = &self.output_device else {
            return;
        };

        for crtc in device.surfaces.keys() {
            set_crtc_active(&device.drm, *crtc, active);
        }
    }
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
