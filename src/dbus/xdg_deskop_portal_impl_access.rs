use std::collections::HashMap;
use std::ops::Not;

use zbus::dbus_interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use super::Start;
use crate::ui::access_dialog::{AccessDialogOptions, AccessDialogRequest, AccessDialogResponse};

pub struct AccessPortalImpl {
    to_niri: calloop::channel::Sender<AccessDialogRequest>,
}

#[dbus_interface(name = "org.freedesktop.impl.portal.Access")]
impl AccessPortalImpl {
    /// AccessDialog method
    #[allow(clippy::too_many_arguments)]
    async fn access_dialog(
        &self,
        _handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        subtitle: &str,
        body: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::fdo::Result<(u32, HashMap<String, OwnedValue>)> {
        let options = AccessDialogOptions::from_dbus_options(options);
        let (request, response_channel_receiver) = AccessDialogRequest::new(
            app_id.to_string(),
            parent_window
                .is_empty()
                .not()
                .then(|| parent_window.to_string()),
            title.to_string(),
            subtitle.to_string(),
            body.is_empty().not().then(|| body.to_string()),
            options,
        );

        if let Err(err) = self.to_niri.send(request) {
            tracing::warn!(?err, "failed to send access dialog request");
            return Err(zbus::fdo::Error::Failed(format!(
                "error creating access dialog: {err:?}"
            )));
        };

        let result = HashMap::<String, OwnedValue>::new();
        let response = match response_channel_receiver.recv().await {
            Ok(AccessDialogResponse::Grant) => {
                // FIXME: Add selected choices to the result
                0
            }
            Ok(AccessDialogResponse::Deny) => 1,
            Err(err) => {
                tracing::warn!(?err, "failed to receive response for access dialog request");
                return Ok((2, Default::default()));
            }
        };

        return Ok((response, result));
    }
}

impl AccessPortalImpl {
    pub fn new(to_niri: calloop::channel::Sender<AccessDialogRequest>) -> Self {
        Self { to_niri }
    }
}

impl Start for AccessPortalImpl {
    fn start(self) -> anyhow::Result<zbus::blocking::Connection> {
        let conn = zbus::blocking::ConnectionBuilder::session()?
            .name("org.freedesktop.impl.portal.desktop.niri")?
            .serve_at("/org/freedesktop/portal/desktop", self)?
            .build()?;
        Ok(conn)
    }
}

impl AccessDialogOptions {
    fn from_dbus_options(options: HashMap<&str, Value<'_>>) -> Self {
        let modal: bool = options
            .get("modal")
            .and_then(|option| option.downcast_ref())
            .copied()
            .unwrap_or(true);
        let deny_label: Option<String> = options
            .get("deny_label")
            .and_then(|option| option.clone().downcast());
        let grant_label: Option<String> = options
            .get("grant_label")
            .and_then(|option| option.clone().downcast());
        let icon: Option<String> = options
            .get("icon")
            .and_then(|option| option.clone().downcast());

        // FIXME: Add support for choices

        Self {
            modal,
            deny_label,
            grant_label,
            icon,
        }
    }
}
