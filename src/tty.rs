use std::collections::{HashMap, HashSet};
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::{Format as DrmFormat, Fourcc};
use smithay::backend::drm::compositor::DrmCompositor;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ImportEgl};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{self, UdevBackend, UdevEvent};
use smithay::output::{Mode, Output, OutputModeSource, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::{connector, crtc, ModeTypeFlags};
use smithay::reexports::input::Libinput;
use smithay::reexports::nix::fcntl::OFlag;
use smithay::reexports::nix::libc::dev_t;
use smithay::utils::DeviceFd;
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use smithay_drm_extras::edid::EdidInfo;

use crate::backend::Backend;
use crate::input::CompositorMod;
use crate::niri::OutputRenderElements;
use crate::{LoopData, Niri};

const BACKGROUND_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.];
const SUPPORTED_COLOR_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Abgr8888];

pub struct Tty {
    session: LibSeatSession,
    primary_gpu_path: PathBuf,
    output_device: Option<OutputDevice>,
}

type GbmDrmCompositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmDevice<DrmDeviceFd>, (), DrmDeviceFd>;

struct OutputDevice {
    id: dev_t,
    token: RegistrationToken,
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
    gles: GlesRenderer,
    formats: HashSet<DrmFormat>,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, GbmDrmCompositor>,
}

#[derive(Debug, Clone, Copy)]
struct TtyOutputState {
    device_id: dev_t,
    crtc: crtc::Handle,
}

impl Backend for Tty {
    fn seat_name(&self) -> String {
        self.session.seat()
    }

    fn renderer(&mut self) -> &mut GlesRenderer {
        &mut self.output_device.as_mut().unwrap().gles
    }

    fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
    ) {
        let _span = tracy_client::span!("Tty::render");

        let device = self.output_device.as_mut().unwrap();
        let tty_state: &TtyOutputState = output.user_data().get().unwrap();
        let drm_compositor = device.surfaces.get_mut(&tty_state.crtc).unwrap();

        match drm_compositor.render_frame::<_, _, GlesTexture>(
            &mut device.gles,
            elements,
            BACKGROUND_COLOR,
        ) {
            Ok(res) => {
                assert!(!res.needs_sync());
                // debug!("{:?}", res);
                if res.damage.is_some() {
                    match drm_compositor.queue_frame(()) {
                        Ok(()) => {
                            niri.output_state
                                .get_mut(output)
                                .unwrap()
                                .waiting_for_vblank = true
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
    }
}

impl Tty {
    pub fn new(event_loop: LoopHandle<LoopData>) -> Self {
        let (session, notifier) = LibSeatSession::new().unwrap();
        let seat_name = session.seat();

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput.udev_assign_seat(&seat_name).unwrap();

        let input_backend = LibinputInputBackend::new(libinput.clone());
        event_loop
            .insert_source(input_backend, |event, _, data| {
                let tty = data.tty.as_mut().unwrap();
                let mut change_vt = |vt| tty.change_vt(vt);
                data.niri
                    .process_input_event(&mut change_vt, CompositorMod::Super, event);
            })
            .unwrap();

        event_loop
            .insert_source(notifier, move |event, _, data| {
                let tty = data.tty.as_mut().unwrap();
                let niri = &mut data.niri;

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
                            output_device.drm.activate();

                            for drm_compositor in output_device.surfaces.values_mut() {
                                if let Err(err) = drm_compositor.surface().reset_state() {
                                    warn!("error resetting DRM surface state: {err}");
                                }
                                drm_compositor.reset_buffers();
                            }
                        }

                        niri.queue_redraw_all();
                    }
                }
            })
            .unwrap();

        let primary_gpu_path = udev::primary_gpu(&seat_name).unwrap().unwrap();

        Self {
            session,
            primary_gpu_path,
            output_device: None,
        }
    }

    pub fn init(&mut self, niri: &mut Niri) {
        let backend = UdevBackend::new(&self.session.seat()).unwrap();
        for (device_id, path) in backend.device_list() {
            if let Err(err) = self.device_added(device_id, path, niri) {
                warn!("error adding device: {err:?}");
            }
        }

        niri.event_loop
            .insert_source(backend, move |event, _, data| {
                let tty = data.tty.as_mut().unwrap();
                let niri = &mut data.niri;

                match event {
                    UdevEvent::Added { device_id, path } => {
                        if let Err(err) = tty.device_added(device_id, &path, niri) {
                            warn!("error adding device: {err:?}");
                        }
                    }
                    UdevEvent::Changed { device_id } => tty.device_changed(device_id, niri),
                    UdevEvent::Removed { device_id } => tty.device_removed(device_id, niri),
                }
            })
            .unwrap();
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

        let mut gles = unsafe { GlesRenderer::new(egl_context)? };
        gles.bind_wl_display(&niri.display_handle)?;

        let token = niri
            .event_loop
            .insert_source(drm_notifier, move |event, metadata, data| {
                let tty = data.tty.as_mut().unwrap();
                match event {
                    DrmEvent::VBlank(crtc) => {
                        tracy_client::Client::running()
                            .unwrap()
                            .message("vblank", 0);
                        trace!("vblank {metadata:?}");

                        let device = tty.output_device.as_mut().unwrap();
                        let drm_compositor = device.surfaces.get_mut(&crtc).unwrap();

                        // Mark the last frame as submitted.
                        if let Err(err) = drm_compositor.frame_submitted() {
                            error!("error marking frame as submitted: {err}");
                        }

                        // Send presentation time feedback.
                        // catacomb
                        //     .windows
                        //     .mark_presented(&output_device.last_render_states, metadata);

                        let output = data
                            .niri
                            .global_space
                            .outputs()
                            .find(|output| {
                                let tty_state: &TtyOutputState = output.user_data().get().unwrap();
                                tty_state.device_id == device.id && tty_state.crtc == crtc
                            })
                            .unwrap()
                            .clone();
                        data.niri
                            .output_state
                            .get_mut(&output)
                            .unwrap()
                            .waiting_for_vblank = false;
                        data.niri.queue_redraw(output);
                    }
                    DrmEvent::Error(error) => error!("DRM error: {error}"),
                };
            })
            .unwrap();

        let formats = Bind::<Dmabuf>::supported_formats(&gles).unwrap_or_default();

        self.output_device = Some(OutputDevice {
            id: device_id,
            token,
            drm,
            gbm,
            gles,
            formats,
            drm_scanner: DrmScanner::new(),
            surfaces: HashMap::new(),
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
        let Some(device) = self.output_device.take() else {
            return;
        };
        if device.id != device_id {
            // It wasn't the output device, put it back in.
            self.output_device = Some(device);
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
            output_name,
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

        // Create the compositor.
        let compositor = DrmCompositor::new(
            OutputModeSource::Auto(output.clone()),
            surface,
            None,
            allocator,
            device.gbm.clone(),
            SUPPORTED_COLOR_FORMATS,
            device.formats.clone(),
            device.drm.cursor_size(),
            Some(device.gbm.clone()),
        )?;

        let res = device.surfaces.insert(crtc, compositor);
        assert!(res.is_none(), "crtc must not have already existed");

        niri.add_output(output.clone());
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

        if device.surfaces.remove(&crtc).is_none() {
            debug!("crts wasn't enabled");
            return;
        }

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
    }

    fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            error!("error changing VT: {err}");
        }
    }
}
