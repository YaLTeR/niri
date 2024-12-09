use std::os::unix::net::UnixStream;
use std::sync::Arc;

use smithay::reexports::wayland_server::DisplayHandle;
use zbus::interface;

use super::Start;
use crate::niri::ClientState;

pub struct ServiceChannel {
    display: DisplayHandle,
}

#[interface(name = "org.gnome.Mutter.ServiceChannel")]
impl ServiceChannel {
    async fn open_wayland_service_connection(
        &mut self,
        service_client_type: u32,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedFd> {
        if service_client_type != 1 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Invalid service client type".to_owned(),
            ));
        }

        let (sock1, sock2) = UnixStream::pair().unwrap();
        let data = Arc::new(ClientState {
            compositor_state: Default::default(),
            // Would be nice to thread config here but for now it's fine.
            can_view_decoration_globals: false,
            restricted: false,
            // FIXME: maybe you can get the PID from D-Bus somehow?
            credentials_unknown: true,
        });
        self.display.insert_client(sock2, data).unwrap();
        Ok(zbus::zvariant::OwnedFd::from(std::os::fd::OwnedFd::from(
            sock1,
        )))
    }
}

impl ServiceChannel {
    pub fn new(display: DisplayHandle) -> Self {
        Self { display }
    }
}

impl Start for ServiceChannel {
    fn start(self) -> anyhow::Result<zbus::blocking::Connection> {
        let conn = zbus::blocking::connection::Builder::session()?
            .name("org.gnome.Mutter.ServiceChannel")?
            .serve_at("/org/gnome/Mutter/ServiceChannel", self)?
            .build()?;
        Ok(conn)
    }
}
