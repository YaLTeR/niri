//! Module containing the parts of the implementation of remote desktop that need global state.

use std::collections::HashMap;

use calloop::{LoopHandle, RegistrationToken};
use reis::calloop::{EisRequestSource, EisRequestSourceEvent};
use reis::eis;
use reis::request::EisRequest;

use crate::dbus::mutter_remote_desktop::RemoteDesktopDBusToCalloop;
use crate::input::remote_desktop_backend::{
    EisEventAdapter, PressedCount, RemoteDesktopInputBackend,
};
use crate::niri::State;

type InputEvent = smithay::backend::input::InputEvent<RemoteDesktopInputBackend>;

/// Child struct of the global `State` struct
#[derive(Default)]
pub struct RemoteDesktopState {
    active_sessions: HashMap<usize, RegistrationToken>
}

#[derive(Default)]
struct ContextState {
    seat: Option<reis::request::Seat>,
    /// The number of keys pressed on all devices in the seat.
    key_counter: u32,
}

impl RemoteDesktopState {
    pub fn on_msg_from_dbus(
        &mut self,
        msg: RemoteDesktopDBusToCalloop,
        event_loop: &LoopHandle<'static, State>,
    ) {
        match msg {
            RemoteDesktopDBusToCalloop::StopSession { session_id } => {
                if let Some(token) = self.active_sessions.remove(&session_id) {
                    event_loop.remove(token);
                } else {
                    warn!("RemoteDesktop StopSession: Invalid session ID")
                }
            }
            RemoteDesktopDBusToCalloop::NewEisContext {
                session_id,
                ctx,
                exposed_device_types,
            } => {
                // This lives in the closure
                let mut context_state = ContextState::default();

                let token = event_loop
                    .insert_source(
                        EisRequestSource::new(ctx, 1),
                        move |event, connection, state| {
                            Ok(match event {
                                Ok(event) => match event {
                                    EisRequestSourceEvent::Connected => {
                                        if !connection.has_interface("ei_seat")
                                            || !connection.has_interface("ei_device")
                                        {
                                            connection.disconnected(
                                                eis::connection::DisconnectReason::Protocol,
                                                "Need `ei_seat` and `ei_device`",
                                            );
                                            if let Err(err) = connection.flush() {
                                                warn!("Error while flushing connection: {err}");
                                            }
                                            return Ok(calloop::PostAction::Remove);
                                        }

                                        let seat = connection.add_seat(
                                            Some("default"),
                                            &exposed_device_types.to_reis_capabilities(),
                                        );

                                        context_state.seat = Some(seat);

                                        if let Err(err) = connection.flush() {
                                            warn!("Error while flushing connection: {err}");
                                            return Ok(calloop::PostAction::Remove);
                                        }
                                        calloop::PostAction::Continue
                                    }
                                    EisRequestSourceEvent::Request(request) => {
                                        let res = Self::handle_eis_request(
                                            state,
                                            request,
                                            connection,
                                            &mut context_state,
                                        );
                                        if let Err(err) = connection.flush() {
                                            warn!("Error while flushing connection: {err}");
                                            return Ok(calloop::PostAction::Remove);
                                        }
                                        res
                                    }
                                    EisRequestSourceEvent::InvalidObject(object_id) => {
                                        // Only send if object ID is in range?
                                        connection
                                            .connection()
                                            .invalid_object(connection.last_serial(), object_id);
                                        if let Err(err) = connection.flush() {
                                            warn!("Error while flushing connection: {err}");
                                            return Ok(calloop::PostAction::Remove);
                                        }
                                        calloop::PostAction::Continue
                                    }
                                },
                                Err(err) => {
                                    warn!("EIS protocol error: {err}");
                                    connection.disconnected(
                                        eis::connection::DisconnectReason::Protocol,
                                        &err.to_string(),
                                    );
                                    if let Err(err) = connection.flush() {
                                        warn!("Error while flushing connection: {err}");
                                        return Ok(calloop::PostAction::Remove);
                                    }
                                    calloop::PostAction::Remove
                                }
                            })
                        },
                    )
                    .unwrap();
                self.active_sessions.insert(session_id, token);
            }
        }
    }

    fn handle_eis_request(
        global_state: &mut State,
        request: reis::request::EisRequest,
        connection: &mut reis::request::Connection,
        context_state: &mut ContextState,
    ) -> calloop::PostAction {
        match request {
            EisRequest::Disconnect => {
                return calloop::PostAction::Remove;
            }
            EisRequest::Bind(reis::request::Bind { seat, capabilities }) => {
                // May be called multiple times
                // Remember to filter this for MutterXdpDeviceTypes
                // Oh and remember to disable input capture

                // TODO: virtual or physical devices?? see https://libinput.pages.freedesktop.org/libei/interfaces/ei_device/index.html#ei_devicedevice_type
            }

            EisRequest::KeyboardKey(inner) => {
                match inner.state {
                    reis::ei::keyboard::KeyState::Released => {
                        context_state.key_counter = context_state.key_counter.saturating_sub(1)
                    }
                    reis::ei::keyboard::KeyState::Press => context_state.key_counter += 1,
                }

                global_state.process_input_event(InputEvent::Keyboard {
                    event: EisEventAdapter {
                        device: todo!(),
                        inner,
                        extra: PressedCount(context_state.key_counter),
                    },
                });
            }
            _ => {}
        }

        calloop::PostAction::Continue
    }
}
