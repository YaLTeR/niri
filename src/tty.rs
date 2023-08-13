use std::os::fd::FromRawFd;
use std::path::PathBuf;

use anyhow::anyhow;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::compositor::DrmCompositor;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ImportEgl};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{self, UdevBackend, UdevEvent};
use smithay::output::{Mode, Output, OutputModeSource, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::connector::{
    Interface as ConnectorInterface, State as ConnectorState,
};
use smithay::reexports::drm::control::{Device, ModeTypeFlags};
use smithay::reexports::input::Libinput;
use smithay::reexports::nix::fcntl::OFlag;
use smithay::reexports::nix::libc::dev_t;
use smithay::utils::DeviceFd;
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
    path: PathBuf,
    token: RegistrationToken,
    drm: DrmDevice,
    gles: GlesRenderer,
    drm_compositor: GbmDrmCompositor,
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
        elements: &[OutputRenderElements<
            GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>,
        >],
    ) {
        let _span = tracy_client::span!("Tty::render");

        let output_device = self.output_device.as_mut().unwrap();
        let drm_compositor = &mut output_device.drm_compositor;

        match drm_compositor.render_frame::<_, _, GlesTexture>(
            &mut output_device.gles,
            elements,
            BACKGROUND_COLOR,
        ) {
            Ok(res) => {
                assert!(!res.needs_sync());
                if res.damage.is_some() {
                    match output_device.drm_compositor.queue_frame(()) {
                        Ok(()) => niri.waiting_for_vblank = true,
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

                            if let Err(err) = output_device.drm_compositor.surface().reset_state() {
                                warn!("error resetting DRM surface state: {err}");
                            }
                            output_device.drm_compositor.reset_buffers();
                        }

                        niri.queue_redraw();
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
            if let Err(err) = self.device_added(device_id, path.to_owned(), niri) {
                warn!("error adding device: {err:?}");
            }
        }

        niri.event_loop
            .insert_source(backend, move |event, _, data| {
                let tty = data.tty.as_mut().unwrap();
                let niri = &mut data.niri;

                match event {
                    UdevEvent::Added { device_id, path } => {
                        if let Err(err) = tty.device_added(device_id, path, niri) {
                            warn!("error adding device: {err:?}");
                        }
                        niri.queue_redraw();
                    }
                    UdevEvent::Changed { device_id } => tty.device_changed(device_id, niri),
                    UdevEvent::Removed { device_id } => tty.device_removed(device_id, niri),
                }
            })
            .unwrap();

        niri.queue_redraw();
    }

    fn device_added(
        &mut self,
        device_id: dev_t,
        path: PathBuf,
        niri: &mut Niri,
    ) -> anyhow::Result<()> {
        if path != self.primary_gpu_path {
            debug!("skipping non-primary device {path:?}");
            return Ok(());
        }

        debug!("adding device {path:?}");
        assert!(self.output_device.is_none());

        let open_flags = OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NOCTTY | OFlag::O_NONBLOCK;
        let fd = self.session.open(&path, open_flags)?;
        let device_fd = unsafe { DrmDeviceFd::new(DeviceFd::from_raw_fd(fd)) };

        let (drm, drm_notifier) = DrmDevice::new(device_fd.clone(), true)?;
        let gbm = GbmDevice::new(device_fd)?;

        let display = EGLDisplay::new(gbm.clone())?;
        let egl_context = EGLContext::new(&display)?;

        let mut gles = unsafe { GlesRenderer::new(egl_context)? };
        gles.bind_wl_display(&niri.display_handle)?;

        let drm_compositor = self.create_drm_compositor(&drm, &gbm, &gles, niri)?;

        let token = niri
            .event_loop
            .insert_source(drm_notifier, move |event, metadata, data| {
                let tty = data.tty.as_mut().unwrap();
                match event {
                    DrmEvent::VBlank(_crtc) => {
                        tracy_client::Client::running()
                            .unwrap()
                            .message("vblank", 0);
                        trace!("vblank {metadata:?}");

                        let output_device = tty.output_device.as_mut().unwrap();

                        // Mark the last frame as submitted.
                        if let Err(err) = output_device.drm_compositor.frame_submitted() {
                            error!("error marking frame as submitted: {err}");
                        }

                        // Send presentation time feedback.
                        // catacomb
                        //     .windows
                        //     .mark_presented(&output_device.last_render_states, metadata);

                        data.niri.waiting_for_vblank = false;
                        data.niri.queue_redraw();
                    }
                    DrmEvent::Error(error) => error!("DRM error: {error}"),
                };
            })
            .unwrap();

        self.output_device = Some(OutputDevice {
            id: device_id,
            path,
            token,
            drm,
            gles,
            drm_compositor,
        });

        Ok(())
    }

    fn device_changed(&mut self, device_id: dev_t, niri: &mut Niri) {
        if let Some(output_device) = &self.output_device {
            if output_device.id == device_id {
                debug!("output device changed");

                let path = output_device.path.clone();
                self.device_removed(device_id, niri);
                if let Err(err) = self.device_added(device_id, path, niri) {
                    warn!("error adding device: {err:?}");
                }
            }
        }
    }

    fn device_removed(&mut self, device_id: dev_t, niri: &mut Niri) {
        if let Some(mut output_device) = self.output_device.take() {
            if output_device.id != device_id {
                self.output_device = Some(output_device);
                return;
            }

            // FIXME: remove wl_output.
            niri.event_loop.remove(output_device.token);
            niri.output = None;
            output_device.gles.unbind_wl_display();
        }
    }

    fn create_drm_compositor(
        &mut self,
        drm: &DrmDevice,
        gbm: &GbmDevice<DrmDeviceFd>,
        gles: &GlesRenderer,
        niri: &mut Niri,
    ) -> anyhow::Result<GbmDrmCompositor> {
        let formats = Bind::<Dmabuf>::supported_formats(gles)
            .ok_or_else(|| anyhow!("no supported formats"))?;
        let resources = drm.resource_handles()?;

        let mut connector = None;
        let mut edp_connector = None;
        resources
            .connectors()
            .iter()
            .filter_map(|conn| match drm.get_connector(*conn, true) {
                Ok(info) => Some(info),
                Err(err) => {
                    warn!("error probing connector: {err}");
                    None
                }
            })
            .inspect(|conn| {
                debug!(
                    "connector: {}-{}, {:?}, {} modes",
                    conn.interface().as_str(),
                    conn.interface_id(),
                    conn.state(),
                    conn.modes().len(),
                );
            })
            .filter(|conn| conn.state() == ConnectorState::Connected)
            .for_each(|conn| {
                connector = Some(conn.clone());

                if conn.interface() == ConnectorInterface::EmbeddedDisplayPort {
                    edp_connector = Some(conn);
                }
            });
        // Since we're only using one output at the moment, prefer eDP.
        let connector = edp_connector
            .or(connector)
            .ok_or_else(|| anyhow!("no compatible connector"))?;
        info!(
            "picking connector: {}-{}",
            connector.interface().as_str(),
            connector.interface_id(),
        );

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
        info!("picking mode: {mode:?}");

        let surface = connector
            .encoders()
            .iter()
            .filter_map(|enc| match drm.get_encoder(*enc) {
                Ok(info) => Some(info),
                Err(err) => {
                    warn!("error probing encoder: {err}");
                    None
                }
            })
            .flat_map(|enc| {
                // Get all CRTCs compatible with the encoder.
                let mut crtcs = resources.filter_crtcs(enc.possible_crtcs());

                // Sort by maximum number of overlay planes.
                crtcs.sort_by_cached_key(|crtc| match drm.planes(crtc) {
                    Ok(planes) => -(planes.overlay.len() as isize),
                    Err(err) => {
                        warn!("error probing planes for CRTC: {err}");
                        0
                    }
                });

                crtcs
            })
            .find_map(
                |crtc| match drm.create_surface(crtc, *mode, &[connector.handle()]) {
                    Ok(surface) => Some(surface),
                    Err(err) => {
                        warn!("error creating DRM surface: {err}");
                        None
                    }
                },
            );
        let surface = surface.ok_or_else(|| anyhow!("no surface"))?;

        // Create GBM allocator.
        let gbm_flags = GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT;
        let allocator = GbmAllocator::new(gbm.clone(), gbm_flags);

        // Update the output mode.
        let (physical_width, physical_height) = connector.size().unwrap_or((0, 0));
        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id(),
        );

        let (make, model) = EdidInfo::for_connector(drm, connector.handle())
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

        // FIXME: store this somewhere to remove on disconnect, etc.
        let _global = output.create_global::<Niri>(&niri.display_handle);
        niri.space.map_output(&output, (0, 0));
        niri.output = Some(output.clone());
        // windows.set_output();

        // Create the compositor.
        let compositor = DrmCompositor::new(
            OutputModeSource::Auto(output),
            surface,
            None,
            allocator,
            gbm.clone(),
            SUPPORTED_COLOR_FORMATS,
            formats,
            drm.cursor_size(),
            Some(gbm.clone()),
        )?;
        Ok(compositor)
    }

    fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            error!("error changing VT: {err}");
        }
    }
}
