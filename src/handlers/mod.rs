mod compositor;
mod layer_shell;
mod xdg_shell;

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::desktop::PopupKind;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_data_source::WlDataSource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
    ServerDndGrabHandler,
};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportError};
use smithay::wayland::input_method::{InputMethodHandler, PopupSurface};
use smithay::wayland::primary_selection::{
    set_primary_focus, PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::{
    delegate_data_device, delegate_dmabuf, delegate_input_method_manager, delegate_output,
    delegate_pointer_gestures, delegate_presentation, delegate_primary_selection, delegate_seat,
    delegate_tablet_manager, delegate_text_input_manager, delegate_virtual_keyboard_manager,
};

use crate::niri::State;

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<State> {
        &mut self.niri.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.niri.cursor_image = image;
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
delegate_tablet_manager!(State);
delegate_pointer_gestures!(State);
delegate_text_input_manager!(State);

impl InputMethodHandler for State {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(err) = self.niri.popups.track_popup(PopupKind::from(surface)) {
            warn!("error tracking ime popup {err:?}");
        }
    }
    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.niri
            .monitor_set
            .find_window_and_output(parent)
            .map(|(window, _)| window.geometry())
            .unwrap_or_default()
    }
}

delegate_input_method_manager!(State);
delegate_virtual_keyboard_manager!(State);

impl DataDeviceHandler for State {
    type SelectionUserData = ();
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
    type SelectionUserData = ();

    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.niri.primary_selection_state
    }
}
delegate_primary_selection!(State);

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
    ) -> Result<(), ImportError> {
        let renderer = self.backend.renderer().expect(
            "the dmabuf global must be created and destroyed together with the output device",
        );
        match renderer.import_dmabuf(&dmabuf, None) {
            Ok(_texture) => Ok(()),
            Err(err) => {
                debug!("error importing dmabuf: {err:?}");
                Err(ImportError::Failed)
            }
        }
    }
}
delegate_dmabuf!(State);
