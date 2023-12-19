mod compositor;
mod layer_shell;
mod xdg_shell;

use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::thread;

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::desktop::{PopupKind, PopupManager};
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_data_source::WlDataSource;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Logical, Rectangle, Size};
use smithay::wayland::compositor::{send_surface_state, with_states};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::input_method::{InputMethodHandler, PopupSurface};
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
    ServerDndGrabHandler,
};
use smithay::wayland::selection::primary_selection::{
    set_primary_focus, PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::wayland::selection::wlr_data_control::{DataControlHandler, DataControlState};
use smithay::wayland::selection::{SelectionHandler, SelectionTarget};
use smithay::wayland::session_lock::{
    LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
};
use smithay::{
    delegate_cursor_shape, delegate_data_control, delegate_data_device, delegate_dmabuf,
    delegate_input_method_manager, delegate_output, delegate_pointer_gestures,
    delegate_presentation, delegate_primary_selection, delegate_relative_pointer, delegate_seat,
    delegate_session_lock, delegate_tablet_manager, delegate_text_input_manager,
    delegate_virtual_keyboard_manager,
};

use crate::layout::output_size;
use crate::niri::State;

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<State> {
        &mut self.niri.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, mut image: CursorImageStatus) {
        // FIXME: this hack should be removable once the screenshot UI is tracked with a
        // PointerFocus properly.
        if self.niri.screenshot_ui.is_open() {
            image = CursorImageStatus::Named(CursorIcon::Crosshair);
        }
        self.niri.cursor_manager.set_cursor_image(image);
        // FIXME: more granular
        self.niri.queue_redraw_all();
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.niri.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client.clone());
        set_primary_focus(dh, seat, client);
    }
}
delegate_seat!(State);
delegate_cursor_shape!(State);
delegate_tablet_manager!(State);
delegate_pointer_gestures!(State);
delegate_relative_pointer!(State);
delegate_text_input_manager!(State);

impl InputMethodHandler for State {
    fn new_popup(&mut self, surface: PopupSurface) {
        let popup = PopupKind::from(surface.clone());
        if let Some(output) = self.output_for_popup(&popup) {
            let scale = output.current_scale().integer_scale();
            let transform = output.current_transform();
            let wl_surface = surface.wl_surface();
            with_states(wl_surface, |data| {
                send_surface_state(wl_surface, data, scale, transform);
            });
        }
        if let Err(err) = self.niri.popups.track_popup(popup) {
            warn!("error tracking ime popup {err:?}");
        }
    }
    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }
    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.niri
            .layout
            .find_window_and_output(parent)
            .map(|(window, _)| window.geometry())
            .unwrap_or_default()
    }
}

delegate_input_method_manager!(State);
delegate_virtual_keyboard_manager!(State);

impl SelectionHandler for State {
    type SelectionUserData = Arc<[u8]>;

    fn send_selection(
        &mut self,
        _ty: SelectionTarget,
        _mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        user_data: &Self::SelectionUserData,
    ) {
        let _span = tracy_client::span!("send_selection");

        let buf = user_data.clone();
        thread::spawn(move || {
            if let Err(err) = File::from(fd).write_all(&buf) {
                warn!("error writing selection: {err:?}");
            }
        });
    }
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.niri.data_device_state
    }
}

impl ClientDndGrabHandler for State {
    fn started(
        &mut self,
        _source: Option<WlDataSource>,
        icon: Option<WlSurface>,
        _seat: Seat<Self>,
    ) {
        self.niri.dnd_icon = icon;
        // FIXME: more granular
        self.niri.queue_redraw_all();
    }

    fn dropped(&mut self, _seat: Seat<Self>) {
        self.niri.dnd_icon = None;
        // FIXME: more granular
        self.niri.queue_redraw_all();
    }
}

impl ServerDndGrabHandler for State {}

delegate_data_device!(State);

impl PrimarySelectionHandler for State {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.niri.primary_selection_state
    }
}
delegate_primary_selection!(State);

impl DataControlHandler for State {
    fn data_control_state(&self) -> &DataControlState {
        &self.niri.data_control_state
    }
}

delegate_data_control!(State);

delegate_output!(State);

delegate_presentation!(State);

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        self.backend.tty().dmabuf_state()
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let renderer = self.backend.renderer().expect(
            "the dmabuf global must be created and destroyed together with the output device",
        );
        match renderer.import_dmabuf(&dmabuf, None) {
            Ok(_texture) => {
                let _ = notifier.successful::<State>();
            }
            Err(err) => {
                debug!("error importing dmabuf: {err:?}");
                notifier.failed();
            }
        }
    }
}
delegate_dmabuf!(State);

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.niri.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        self.niri.lock(confirmation);
    }

    fn unlock(&mut self) {
        self.niri.unlock();
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        let Some(output) = Output::from_resource(&output) else {
            error!("no Output matching WlOutput");
            return;
        };

        configure_lock_surface(&surface, &output);
        self.niri.new_lock_surface(surface, &output);
    }
}
delegate_session_lock!(State);

pub fn configure_lock_surface(surface: &LockSurface, output: &Output) {
    surface.with_pending_state(|states| {
        let size = output_size(output);
        states.size = Some(Size::from((size.w as u32, size.h as u32)));
    });
    let scale = output.current_scale().integer_scale();
    let transform = output.current_transform();
    let wl_surface = surface.wl_surface();
    with_states(wl_surface, |data| {
        send_surface_state(wl_surface, data, scale, transform);
    });
    surface.send_configure();
}
