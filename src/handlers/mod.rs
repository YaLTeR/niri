mod compositor;
mod layer_shell;
mod xdg_shell;

use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_data_source::WlDataSource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
    ServerDndGrabHandler,
};
use smithay::{
    delegate_data_device, delegate_output, delegate_pointer_gestures, delegate_presentation,
    delegate_seat, delegate_tablet_manager,
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
        set_data_device_focus(dh, seat, client);
    }
}
delegate_seat!(State);
delegate_tablet_manager!(State);
delegate_pointer_gestures!(State);

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

delegate_output!(State);

delegate_presentation!(State);
