use zbus::blocking::Connection;
use zbus::Interface;

use crate::niri::State;

pub mod freedesktop_screensaver;
pub mod gnome_shell_screenshot;
pub mod mutter_display_config;
pub mod mutter_service_channel;

#[cfg(feature = "xdp-gnome-screencast")]
pub mod mutter_screen_cast;
#[cfg(feature = "xdp-gnome-screencast")]
use mutter_screen_cast::ScreenCast;

use self::freedesktop_screensaver::ScreenSaver;
use self::mutter_display_config::DisplayConfig;
use self::mutter_service_channel::ServiceChannel;

trait Start: Interface {
    fn start(self) -> anyhow::Result<zbus::blocking::Connection>;
}

#[derive(Default)]
pub struct DBusServers {
    pub conn_service_channel: Option<Connection>,
    pub conn_display_config: Option<Connection>,
    pub conn_screen_saver: Option<Connection>,
    pub conn_screen_shot: Option<Connection>,
    #[cfg(feature = "xdp-gnome-screencast")]
    pub conn_screen_cast: Option<Connection>,
}

impl DBusServers {
    pub fn start(state: &mut State, is_session_instance: bool) {
        let _span = tracy_client::span!("DBusServers::start");

        let backend = &state.backend;
        let niri = &mut state.niri;
        let config = niri.config.borrow();

        let mut dbus = Self::default();

        if is_session_instance {
            let service_channel = ServiceChannel::new(niri.display_handle.clone());
            dbus.conn_service_channel = try_start(service_channel);
        }

        if is_session_instance || config.debug.dbus_interfaces_in_non_session_instances {
            let display_config = DisplayConfig::new(backend.enabled_outputs());
            dbus.conn_display_config = try_start(display_config);

            let screen_saver = ScreenSaver::new(niri.is_fdo_idle_inhibited.clone());
            dbus.conn_screen_saver = try_start(screen_saver);

            let (to_niri, from_screenshot) = calloop::channel::channel();
            let (to_screenshot, from_niri) = async_channel::unbounded();
            niri.event_loop
                .insert_source(from_screenshot, move |event, _, state| match event {
                    calloop::channel::Event::Msg(msg) => {
                        state.on_screen_shot_msg(&to_screenshot, msg)
                    }
                    calloop::channel::Event::Closed => (),
                })
                .unwrap();
            let screenshot = gnome_shell_screenshot::Screenshot::new(to_niri, from_niri);
            dbus.conn_screen_shot = try_start(screenshot);

            #[cfg(feature = "xdp-gnome-screencast")]
            if niri.pipewire.is_some() {
                let (to_niri, from_screen_cast) = calloop::channel::channel();
                niri.event_loop
                    .insert_source(from_screen_cast, {
                        let to_niri = to_niri.clone();
                        move |event, _, state| match event {
                            calloop::channel::Event::Msg(msg) => {
                                state.on_screen_cast_msg(&to_niri, msg)
                            }
                            calloop::channel::Event::Closed => (),
                        }
                    })
                    .unwrap();
                let screen_cast = ScreenCast::new(backend.enabled_outputs(), to_niri);
                dbus.conn_screen_cast = try_start(screen_cast);
            } else {
                warn!("disabling screencast support because we couldn't start PipeWire");
            }
        }

        niri.dbus = Some(dbus);
    }
}

fn try_start<I: Start>(iface: I) -> Option<Connection> {
    match iface.start() {
        Ok(conn) => Some(conn),
        Err(err) => {
            warn!("error starting {}: {err:?}", I::name());
            None
        }
    }
}
