use std::collections::{HashMap, HashSet};
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Arc};
use std::time::Duration;

use anyhow::{anyhow, Context};
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::{Format as DrmFormat, Fourcc};
use smithay::backend::drm::compositor::DrmCompositor;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmEventTime};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, DebugFlags, ImportDma, ImportEgl};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{self, UdevBackend, UdevEvent};
use smithay::desktop::utils::OutputPresentationFeedback;
use smithay::output::{Mode, Output, OutputModeSource, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::{Dispatcher, LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::{
    connector, crtc, Mode as DrmMode, ModeFlags, ModeTypeFlags,
};
use smithay::reexports::input::Libinput;
use smithay::reexports::nix::fcntl::OFlag;
use smithay::reexports::nix::libc::dev_t;
use smithay::reexports::wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::TrancheFlags;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::utils::DeviceFd;
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState, DmabufFeedback};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use smithay_drm_extras::edid::EdidInfo;

use crate::niri::{OutputRenderElements, State};
use crate::utils::get_monotonic_time;
use crate::{LoopData, Niri};

const BACKGROUND_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.];
const SUPPORTED_COLOR_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Abgr8888];

pub struct Tty {
    session: LibSeatSession,
    udev_dispatcher: Dispatcher<'static, UdevBackend, LoopData>,
    primary_gpu_path: PathBuf,
    output_device: Option<OutputDevice>,
    connectors: Arc<Mutex<HashMap<String, Output>>>,
}

type GbmDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmDevice<DrmDeviceFd>,
    OutputPresentationFeedback,
    DrmDeviceFd,
>;

struct OutputDevice {
    id: dev_t,
    token: RegistrationToken,
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
    gles: GlesRenderer,
    formats: HashSet<DrmFormat>,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, Surface>,
    dmabuf_state: DmabufState,
    dmabuf_global: DmabufGlobal,
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
    /// Frame name for the VBlank frame that unfortunately has to be leaked.
    vblank_frame_name: tracy_client::FrameName,
}

impl Tty {
    pub fn new(event_loop: LoopHandle<'static, LoopData>) -> Self {
        let (session, notifier) = LibSeatSession::new().unwrap();
        let seat_name = session.seat();

        let udev_backend = UdevBackend::new(session.seat()).unwrap();
        let udev_dispatcher =
            Dispatcher::new(udev_backend, move |event, _, data: &mut LoopData| {
                let tty = data.state.backend.tty();
                let niri = &mut data.state.niri;

                match event {
                    UdevEvent::Added { device_id, path } => {
                        if !tty.session.is_active() {
                            debug!("skipping UdevEvent::Added as session is inactive");
                            return;
                        }

                        if let Err(err) = tty.device_added(device_id, &path, niri) {
                            warn!("error adding device: {err:?}");
                        }
                    }
                    UdevEvent::Changed { device_id } => {
                        if !tty.session.is_active() {
                            debug!("skipping UdevEvent::Changed as session is inactive");
                            return;
                        }

                        tty.device_changed(device_id, niri)
                    }
                    UdevEvent::Removed { device_id } => {
                        if !tty.session.is_active() {
                            debug!("skipping UdevEvent::Removed as session is inactive");
                            return;
                        }

                        tty.device_removed(device_id, niri)
                    }
                }
            });
        event_loop
            .register_dispatcher(udev_dispatcher.clone())
            .unwrap();

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput.udev_assign_seat(&seat_name).unwrap();

        let input_backend = LibinputInputBackend::new(libinput.clone());
        event_loop
            .insert_source(input_backend, |mut event, _, data| {
                data.state.process_libinput_event(&mut event);
                data.state.process_input_event(event);
            })
            .unwrap();

        let udev_dispatcher_c = udev_dispatcher.clone();
        event_loop
            .insert_source(notifier, move |event, _, data| {
                let tty = data.state.backend.tty();
                let niri = &mut data.state.niri;

                match event {
                    SessionEvent::PauseSession => {
                        debug!("pausing session");

                        libinput.suspend();

                        if let Some(output_device) = &tty.output_device {
                            output_device.drm.pause();
                        }
                    }
                    SessionEvent::ActivateSession => {
                        debug!("resuming session");

                        if libinput.resume().is_err() {
                            error!("error resuming libinput");
                        }

                        if let Some(output_device) = &mut tty.output_device {
                            // We had an output device, check if it's been removed.
                            let output_device_id = output_device.id;
                            if !udev_dispatcher_c
                                .as_source_ref()
                                .device_list()
                                .any(|(device_id, _)| device_id == output_device_id)
                            {
                                // The output device, if we had any, has been removed.
                                tty.device_removed(output_device_id, niri);
                            } else {
                                // It hasn't been removed, update its state as usual.
                                output_device.drm.activate();

                                // HACK: force reset the connectors to make resuming work across
                                // sleep.
                                let output_device = tty.output_device.as_mut().unwrap();
                                let crtcs: Vec<_> = output_device
                                    .drm_scanner
                                    .crtcs()
                                    .map(|(conn, crtc)| (conn.clone(), crtc))
                                    .collect();
                                for (conn, crtc) in crtcs {
                                    tty.connector_disconnected(niri, conn, crtc);
                                }

                                let output_device = tty.output_device.as_mut().unwrap();
                                let _ = output_device
                                    .drm_scanner
                                    .scan_connectors(&output_device.drm);
                                let crtcs: Vec<_> = output_device
                                    .drm_scanner
                                    .crtcs()
                                    .map(|(conn, crtc)| (conn.clone(), crtc))
                                    .collect();
                                for (conn, crtc) in crtcs {
                                    if let Err(err) = tty.connector_connected(niri, conn, crtc) {
                                        warn!("error connecting connector: {err:?}");
                                    }
                                }

                                // // Refresh the connectors.
                                // tty.device_changed(output_device_id, niri);

                                // // Refresh the state on unchanged connectors.
                                // let output_device = tty.output_device.as_mut().unwrap();
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
                            for (device_id, path) in udev_dispatcher_c.as_source_ref().device_list()
                            {
                                if let Err(err) = tty.device_added(device_id, path, niri) {
                                    warn!("error adding device: {err:?}");
                                }
                            }
                        }
                    }
                }
            })
            .unwrap();

        let primary_gpu_path = udev::primary_gpu(&seat_name).unwrap().unwrap();

        Self {
            session,
            udev_dispatcher,
            primary_gpu_path,
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

        let open_flags = OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NOCTTY | OFlag::O_NONBLOCK;
        let fd = self.session.open(path, open_flags)?;
        let device_fd = unsafe { DrmDeviceFd::new(DeviceFd::from_raw_fd(fd)) };

        let (drm, drm_notifier) = DrmDevice::new(device_fd.clone(), true)?;
        let gbm = GbmDevice::new(device_fd)?;

        let display = EGLDisplay::new(gbm.clone())?;
        let egl_context = EGLContext::new(&display)?;

        // let capabilities = unsafe { GlesRenderer::supported_capabilities(&egl_context) }?
        //     .into_iter()
        //     .filter(|c| *c != Capability::ColorTransformations);
        // let mut gles = unsafe { GlesRenderer::with_capabilities(egl_context, capabilities)? };
        let mut gles = unsafe { GlesRenderer::new(egl_context)? };
        gles.bind_wl_display(&niri.display_handle)?;

        let token = niri
            .event_loop
            .insert_source(drm_notifier, move |event, metadata, data| {
                let tty = data.state.backend.tty();
                match event {
                    DrmEvent::VBlank(crtc) => {
                        let now = get_monotonic_time();

                        let device = tty.output_device.as_mut().unwrap();
                        let surface = device.surfaces.get_mut(&crtc).unwrap();
                        let name = &surface.name;
                        trace!("vblank on {name} {metadata:?}");

                        drop(surface.vblank_frame.take()); // Drop the old one first.
                        let vblank_frame = tracy_client::Client::running()
                            .unwrap()
                            .non_continuous_frame(surface.vblank_frame_name);
                        surface.vblank_frame = Some(vblank_frame);

                        let presentation_time = match metadata.as_mut().unwrap().time {
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
                            format!("vblank on {name}, presentation is {diff:?} later")
                        } else {
                            let diff = now - presentation_time;
                            format!("vblank on {name}, presentation was {diff:?} ago")
                        };
                        tracy_client::Client::running()
                            .unwrap()
                            .message(&message, 0);

                        let output = data
                            .state
                            .niri
                            .global_space
                            .outputs()
                            .find(|output| {
                                let tty_state: &TtyOutputState = output.user_data().get().unwrap();
                                tty_state.device_id == device.id && tty_state.crtc == crtc
                            })
                            .unwrap()
                            .clone();
                        let output_state = data.state.niri.output_state.get_mut(&output).unwrap();

                        // Mark the last frame as submitted.
                        match surface.compositor.frame_submitted() {
                            Ok(Some(mut feedback)) => {
                                let refresh = output_state
                                    .frame_clock
                                    .refresh_interval()
                                    .unwrap_or(Duration::ZERO);
                                // FIXME: ideally should be monotonically increasing for a surface.
                                let seq = metadata.as_ref().unwrap().sequence as u64;
                                let flags = wp_presentation_feedback::Kind::Vsync
                                    | wp_presentation_feedback::Kind::HwClock
                                    | wp_presentation_feedback::Kind::HwCompletion;

                                feedback.presented::<_, smithay::utils::Monotonic>(
                                    presentation_time,
                                    refresh,
                                    seq,
                                    flags,
                                );
                            }
                            Ok(None) => (),
                            Err(err) => {
                                error!("error marking frame as submitted: {err}");
                            }
                        }

                        output_state.waiting_for_vblank = false;
                        output_state.frame_clock.presented(presentation_time);
                        data.state.niri.queue_redraw(output);
                    }
                    DrmEvent::Error(error) => error!("DRM error: {error}"),
                };
            })
            .unwrap();

        let formats = Bind::<Dmabuf>::supported_formats(&gles).unwrap_or_default();

        let mut dmabuf_state = DmabufState::new();
        let default_feedback = DmabufFeedbackBuilder::new(device_id, gles.dmabuf_formats())
            .build()
            .unwrap();
        let dmabuf_global = dmabuf_state
            .create_global_with_default_feedback::<State>(&niri.display_handle, &default_feedback);

        self.output_device = Some(OutputDevice {
            id: device_id,
            token,
            drm,
            gbm,
            gles,
            formats,
            drm_scanner: DrmScanner::new(),
            surfaces: HashMap::new(),
            dmabuf_state,
            dmabuf_global,
        });

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
        device
            .dmabuf_state
            .destroy_global::<State>(&niri.display_handle, device.dmabuf_global);
        device.gles.unbind_wl_display();

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

        let device = self.output_device.as_mut().unwrap();

        let mut mode = connector.modes().get(0);
        connector.modes().iter().for_each(|m| {
            debug!("mode: {m:?}");

            if m.mode_type().contains(ModeTypeFlags::PREFERRED) {
                // Pick the highest refresh rate.
                if mode
                    .map(|curr| curr.vrefresh() < m.vrefresh())
                    .unwrap_or(true)
                {
                    mode = Some(m);
                }
            }
        });
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
        output.change_current_state(Some(wl_mode), None, None, Some((0, 0).into()));
        output.set_preferred(wl_mode);

        output.user_data().insert_if_missing(|| TtyOutputState {
            device_id: device.id,
            crtc,
        });

        let mut planes = surface.planes().clone();
        // Disable overlay planes as they cause weird performance issues on my system.
        planes.overlay.clear();
        let scanout_formats = planes
            .primary
            .formats
            .iter()
            .chain(planes.overlay.iter().flat_map(|p| &p.formats))
            .copied()
            .collect::<HashSet<_>>();
        let scanout_formats = scanout_formats.intersection(&device.formats).copied();

        // Create the compositor.
        let compositor = DrmCompositor::new(
            OutputModeSource::Auto(output.clone()),
            surface,
            Some(planes),
            allocator,
            device.gbm.clone(),
            SUPPORTED_COLOR_FORMATS,
            device.formats.clone(),
            device.drm.cursor_size(),
            Some(device.gbm.clone()),
        )?;

        let dmabuf_feedback = DmabufFeedbackBuilder::new(device.id, device.formats.clone())
            .add_preference_tranche(device.id, Some(TrancheFlags::Scanout), scanout_formats)
            .build()
            .unwrap();

        let vblank_frame_name = unsafe {
            tracy_client::internal::create_frame_name(format!("vblank on {output_name}\0").leak())
        };

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
        };
        let res = device.surfaces.insert(crtc, surface);
        assert!(res.is_none(), "crtc must not have already existed");

        niri.add_output(output.clone(), Some(refresh_interval(*mode)));
        niri.queue_redraw(output);

        Ok(())
    }

    fn connector_disconnected(
        &mut self,
        niri: &mut Niri,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) {
        debug!("disconnecting connector: {connector:?}");
        let device = self.output_device.as_mut().unwrap();

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
            .unwrap()
            .clone();

        niri.remove_output(&output);

        self.connectors.lock().unwrap().remove(&surface.name);
    }

    pub fn seat_name(&self) -> String {
        self.session.seat()
    }

    pub fn renderer(&mut self) -> &mut GlesRenderer {
        &mut self.output_device.as_mut().unwrap().gles
    }

    pub fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
    ) -> Option<&DmabufFeedback> {
        let _span = tracy_client::span!("Tty::render");

        let device = self.output_device.as_mut().unwrap();
        let tty_state: &TtyOutputState = output.user_data().get().unwrap();
        let surface = device.surfaces.get_mut(&tty_state.crtc).unwrap();
        let drm_compositor = &mut surface.compositor;

        match drm_compositor.render_frame::<_, _, GlesTexture>(
            &mut device.gles,
            elements,
            BACKGROUND_COLOR,
        ) {
            Ok(res) => {
                assert!(!res.needs_sync());

                // if let PrimaryPlaneElement::Swapchain(element) = res.primary_element {
                //     let _span = tracy_client::span!("wait for sync");
                //     element.sync.wait();
                // }

                if res.damage.is_some() {
                    let presentation_feedbacks =
                        niri.take_presentation_feedbacks(output, &res.states);

                    match drm_compositor.queue_frame(presentation_feedbacks) {
                        Ok(()) => {
                            niri.output_state
                                .get_mut(output)
                                .unwrap()
                                .waiting_for_vblank = true;

                            return Some(&surface.dmabuf_feedback);
                        }
                        Err(err) => {
                            error!("error queueing frame: {err}");
                        }
                    }
                }
            }
            Err(err) => {
                // Can fail if we switched to a different TTY.
                error!("error rendering frame: {err}");
            }
        }

        // We're not expecting a vblank right after this.
        drop(surface.vblank_frame.take());
        None
    }

    pub fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            error!("error changing VT: {err}");
        }
    }

    pub fn suspend(&self) {
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

    pub fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.output_device.as_mut().unwrap().dmabuf_state
    }

    pub fn connectors(&self) -> Arc<Mutex<HashMap<String, Output>>> {
        self.connectors.clone()
    }

    pub fn gbm_device(&self) -> Option<GbmDevice<DrmDeviceFd>> {
        self.output_device.as_ref().map(|d| d.gbm.clone())
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

fn suspend() -> anyhow::Result<()> {
    let conn = zbus::blocking::Connection::system().context("error connecting to system bus")?;
    let manager = logind_zbus::manager::ManagerProxyBlocking::new(&conn)
        .context("error creating login manager proxy")?;
    manager.suspend(true).context("error suspending")
}
