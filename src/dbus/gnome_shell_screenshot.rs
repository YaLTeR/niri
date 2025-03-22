use std::collections::HashMap;
use std::path::PathBuf;

use niri_ipc::PickedColor;
use zbus::fdo::{self, RequestNameFlags};
use zbus::interface;
use zbus::zvariant::OwnedValue;

use super::Start;

pub struct Screenshot {
    to_niri: calloop::channel::Sender<ScreenshotToNiri>,
    from_niri: async_channel::Receiver<NiriToScreenshot>,
    to_niri_color: calloop::channel::Sender<ColorPickToNiri>,
    from_niri_color: async_channel::Receiver<NiriToColorPick>,
}

pub enum ScreenshotToNiri {
    TakeScreenshot { include_cursor: bool },
}

pub enum NiriToScreenshot {
    ScreenshotResult(Option<PathBuf>),
}

pub enum ColorPickToNiri {
    PickColor,
}

pub enum NiriToColorPick {
    ColorResult(Option<PickedColor>),
}

#[interface(name = "org.gnome.Shell.Screenshot")]
impl Screenshot {
    async fn screenshot(
        &self,
        include_cursor: bool,
        _flash: bool,
        _filename: PathBuf,
    ) -> fdo::Result<(bool, PathBuf)> {
        if let Err(err) = self
            .to_niri
            .send(ScreenshotToNiri::TakeScreenshot { include_cursor })
        {
            warn!("error sending message to niri: {err:?}");
            return Err(fdo::Error::Failed("internal error".to_owned()));
        }

        let filename = match self.from_niri.recv().await {
            Ok(NiriToScreenshot::ScreenshotResult(Some(filename))) => filename,
            Ok(NiriToScreenshot::ScreenshotResult(None)) => {
                return Err(fdo::Error::Failed("internal error".to_owned()));
            }
            Err(err) => {
                warn!("error receiving message from niri: {err:?}");
                return Err(fdo::Error::Failed("internal error".to_owned()));
            }
        };

        Ok((true, filename))
    }

    async fn pick_color(&self) -> fdo::Result<HashMap<String, OwnedValue>> {
        if let Err(err) = self.to_niri_color.send(ColorPickToNiri::PickColor) {
            warn!("error sending pick color message to niri: {err:?}");
            return Err(fdo::Error::Failed("internal error".to_owned()));
        }

        let color = match self.from_niri_color.recv().await {
            Ok(NiriToColorPick::ColorResult(Some(color))) => color,
            Ok(NiriToColorPick::ColorResult(None)) => {
                return Err(fdo::Error::Failed("no color picked".to_owned()));
            }
            Err(err) => {
                warn!("error receiving message from niri: {err:?}");
                return Err(fdo::Error::Failed("internal error".to_owned()));
            }
        };

        let rgb = [color.rgba[0], color.rgba[1], color.rgba[2]];

        let mut result = HashMap::new();
        let rgb_slice: &[f64] = &rgb;
        result.insert(
            "color".to_string(),
            zbus::zvariant::Value::from(rgb_slice).try_into().unwrap(),
        );

        Ok(result)
    }
}

impl Screenshot {
    pub fn new(
        to_niri: calloop::channel::Sender<ScreenshotToNiri>,
        from_niri: async_channel::Receiver<NiriToScreenshot>,
        to_niri_color: calloop::channel::Sender<ColorPickToNiri>,
        from_niri_color: async_channel::Receiver<NiriToColorPick>,
    ) -> Self {
        Self {
            to_niri,
            from_niri,
            to_niri_color,
            from_niri_color,
        }
    }
}

impl Start for Screenshot {
    fn start(self) -> anyhow::Result<zbus::blocking::Connection> {
        let conn = zbus::blocking::Connection::session()?;
        let flags = RequestNameFlags::AllowReplacement
            | RequestNameFlags::ReplaceExisting
            | RequestNameFlags::DoNotQueue;

        conn.object_server()
            .at("/org/gnome/Shell/Screenshot", self)?;
        conn.request_name_with_flags("org.gnome.Shell.Screenshot", flags)?;

        Ok(conn)
    }
}
