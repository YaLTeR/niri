use std::cmp::{max, min};
use std::ffi::{CString, OsStr};
use std::io::Write;
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{ensure, Context};
use bitflags::bitflags;
use directories::UserDirs;
use git_version::git_version;
use niri_config::{Config, OutputName};
use smithay::input::pointer::CursorIcon;
use smithay::output::{self, Output};
use smithay::reexports::rustix::time::{clock_gettime, ClockId};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{DisplayHandle, Resource as _};
use smithay::utils::{Coordinate, Logical, Point, Rectangle, Size, Transform};
use smithay::wayland::compositor::{send_surface_state, with_states, SurfaceData};
use smithay::wayland::fractional_scale::with_fractional_scale;
use smithay::wayland::shell::xdg::{
    ToplevelSurface, XdgToplevelSurfaceData, XdgToplevelSurfaceRoleAttributes,
};
use wayland_backend::server::Credentials;

use crate::niri::ClientState;

pub mod id;
pub mod scale;
pub mod spawning;
pub mod transaction;
pub mod watcher;

pub static IS_SYSTEMD_SERVICE: AtomicBool = AtomicBool::new(false);

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const TOP          = 0b0001;
        const BOTTOM       = 0b0010;
        const LEFT         = 0b0100;
        const RIGHT        = 0b1000;

        const TOP_LEFT     = Self::TOP.bits() | Self::LEFT.bits();
        const BOTTOM_LEFT  = Self::BOTTOM.bits() | Self::LEFT.bits();

        const TOP_RIGHT    = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();

        const LEFT_RIGHT   = Self::LEFT.bits() | Self::RIGHT.bits();
        const TOP_BOTTOM   = Self::TOP.bits() | Self::BOTTOM.bits();
    }
}

impl From<xdg_toplevel::ResizeEdge> for ResizeEdge {
    #[inline]
    fn from(x: xdg_toplevel::ResizeEdge) -> Self {
        Self::from_bits(x as u32).unwrap()
    }
}

impl ResizeEdge {
    pub fn cursor_icon(self) -> CursorIcon {
        match self {
            Self::LEFT => CursorIcon::WResize,
            Self::RIGHT => CursorIcon::EResize,
            Self::TOP => CursorIcon::NResize,
            Self::BOTTOM => CursorIcon::SResize,
            Self::TOP_LEFT => CursorIcon::NwResize,
            Self::TOP_RIGHT => CursorIcon::NeResize,
            Self::BOTTOM_RIGHT => CursorIcon::SeResize,
            Self::BOTTOM_LEFT => CursorIcon::SwResize,
            _ => CursorIcon::Default,
        }
    }
}

pub fn version() -> String {
    format!(
        "{} ({})",
        env!("CARGO_PKG_VERSION"),
        git_version!(fallback = "unknown commit"),
    )
}

pub fn get_monotonic_time() -> Duration {
    let ts = clock_gettime(ClockId::Monotonic);
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

pub fn center(rect: Rectangle<i32, Logical>) -> Point<i32, Logical> {
    rect.loc + rect.size.downscale(2).to_point()
}

pub fn center_f64(rect: Rectangle<f64, Logical>) -> Point<f64, Logical> {
    rect.loc + rect.size.downscale(2.0).to_point()
}

/// Convert logical pixels to physical, rounding to physical pixels.
pub fn to_physical_precise_round<N: Coordinate>(scale: f64, logical: impl Coordinate) -> N {
    N::from_f64((logical.to_f64() * scale).round())
}

pub fn round_logical_in_physical(scale: f64, logical: f64) -> f64 {
    (logical * scale).round() / scale
}

pub fn round_logical_in_physical_max1(scale: f64, logical: f64) -> f64 {
    if logical == 0. {
        return 0.;
    }

    (logical * scale).max(1.).round() / scale
}

pub fn output_size(output: &Output) -> Size<f64, Logical> {
    let output_scale = output.current_scale().fractional_scale();
    let output_transform = output.current_transform();
    let output_mode = output.current_mode().unwrap();
    let logical_size = output_mode.size.to_f64().to_logical(output_scale);
    output_transform.transform_size(logical_size)
}

pub fn logical_output(output: &Output) -> niri_ipc::LogicalOutput {
    let loc = output.current_location();
    let size = output_size(output);
    let transform = match output.current_transform() {
        Transform::Normal => niri_ipc::Transform::Normal,
        Transform::_90 => niri_ipc::Transform::_90,
        Transform::_180 => niri_ipc::Transform::_180,
        Transform::_270 => niri_ipc::Transform::_270,
        Transform::Flipped => niri_ipc::Transform::Flipped,
        Transform::Flipped90 => niri_ipc::Transform::Flipped90,
        Transform::Flipped180 => niri_ipc::Transform::Flipped180,
        Transform::Flipped270 => niri_ipc::Transform::Flipped270,
    };
    niri_ipc::LogicalOutput {
        x: loc.x,
        y: loc.y,
        width: size.w as u32,
        height: size.h as u32,
        scale: output.current_scale().fractional_scale(),
        transform,
    }
}

pub fn ipc_transform_to_smithay(transform: niri_ipc::Transform) -> Transform {
    match transform {
        niri_ipc::Transform::Normal => Transform::Normal,
        niri_ipc::Transform::_90 => Transform::_90,
        niri_ipc::Transform::_180 => Transform::_180,
        niri_ipc::Transform::_270 => Transform::_270,
        niri_ipc::Transform::Flipped => Transform::Flipped,
        niri_ipc::Transform::Flipped90 => Transform::Flipped90,
        niri_ipc::Transform::Flipped180 => Transform::Flipped180,
        niri_ipc::Transform::Flipped270 => Transform::Flipped270,
    }
}

pub fn send_scale_transform(
    surface: &WlSurface,
    data: &SurfaceData,
    scale: output::Scale,
    transform: Transform,
) {
    send_surface_state(surface, data, scale.integer_scale(), transform);
    with_fractional_scale(data, |fractional| {
        fractional.set_preferred_scale(scale.fractional_scale());
    });
}

pub fn expand_home(path: &Path) -> anyhow::Result<Option<PathBuf>> {
    if let Ok(rest) = path.strip_prefix("~") {
        let dirs = UserDirs::new().context("error retrieving home directory")?;
        Ok(Some([dirs.home_dir(), rest].iter().collect()))
    } else {
        Ok(None)
    }
}

pub fn make_screenshot_path(config: &Config) -> anyhow::Result<Option<PathBuf>> {
    let Some(path) = &config.screenshot_path else {
        return Ok(None);
    };

    let format = CString::new(path.clone()).context("path must not contain nul bytes")?;

    let mut buf = [0u8; 2048];
    let mut path;
    unsafe {
        let time = libc::time(null_mut());
        ensure!(time != -1, "error in time()");

        let tm = libc::localtime(&time);
        ensure!(!tm.is_null(), "error in localtime()");

        let rv = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), format.as_ptr(), tm);
        ensure!(rv != 0, "error formatting time");

        path = PathBuf::from(OsStr::from_bytes(&buf[..rv]));
    }

    if let Some(expanded) = expand_home(&path).context("error expanding ~")? {
        path = expanded;
    }

    Ok(Some(path))
}

pub fn write_png_rgba8(
    w: impl Write,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), png::EncodingError> {
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)
}

pub fn output_matches_name(output: &Output, target: &str) -> bool {
    let name = output.user_data().get::<OutputName>().unwrap();
    name.matches(target)
}

pub fn is_laptop_panel(connector: &str) -> bool {
    matches!(connector.get(..4), Some("eDP-" | "LVDS" | "DSI-"))
}

pub fn with_toplevel_role<T>(
    toplevel: &ToplevelSurface,
    f: impl FnOnce(&mut XdgToplevelSurfaceRoleAttributes) -> T,
) -> T {
    with_states(toplevel.wl_surface(), |states| {
        let mut role = states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .unwrap()
            .lock()
            .unwrap();

        f(&mut role)
    })
}

pub fn get_credentials_for_surface(surface: &WlSurface) -> Option<Credentials> {
    let handle = surface.handle().upgrade()?;
    let dh = DisplayHandle::from(handle);

    let client = dh.get_client(surface.id()).ok()?;
    let data = client.get_data::<ClientState>().unwrap();
    if data.credentials_unknown {
        return None;
    }

    client.get_credentials(&dh).ok()
}

pub fn ensure_min_max_size(mut x: i32, min_size: i32, max_size: i32) -> i32 {
    if max_size > 0 {
        x = min(x, max_size);
    }
    if min_size > 0 {
        x = max(x, min_size);
    }
    x
}

pub fn ensure_min_max_size_maybe_zero(x: i32, min_size: i32, max_size: i32) -> i32 {
    if x != 0 {
        ensure_min_max_size(x, min_size, max_size)
    } else if min_size > 0 && min_size == max_size {
        min_size
    } else {
        0
    }
}

pub fn clamp_preferring_top_left_in_area(
    area: Rectangle<f64, Logical>,
    rect: &mut Rectangle<f64, Logical>,
) {
    rect.loc.x = f64::min(rect.loc.x, area.loc.x + area.size.w - rect.size.w);
    rect.loc.y = f64::min(rect.loc.y, area.loc.y + area.size.h - rect.size.h);

    // Clamp by top and left last so it takes precedence.
    rect.loc.x = f64::max(rect.loc.x, area.loc.x);
    rect.loc.y = f64::max(rect.loc.y, area.loc.y);
}

pub fn center_preferring_top_left_in_area(
    area: Rectangle<f64, Logical>,
    size: Size<f64, Logical>,
) -> Point<f64, Logical> {
    let area_size = area.size.to_point();
    let size = size.to_point();
    let mut offset = (area_size - size).downscale(2.);
    offset.x = f64::max(offset.x, 0.);
    offset.y = f64::max(offset.y, 0.);
    area.loc + offset
}

#[cfg(feature = "dbus")]
pub fn show_screenshot_notification(image_path: Option<PathBuf>) {
    let mut notification = notify_rust::Notification::new();
    notification
        .summary("Screenshot captured")
        .body("You can paste the image from the clipboard.")
        .urgency(notify_rust::Urgency::Normal)
        .hint(notify_rust::Hint::Transient(true));

    // Try to add the screenshot as an image if possible.
    if let Some(path) = image_path {
        match path.canonicalize() {
            Ok(path) => match url::Url::from_file_path(path) {
                Ok(url) => {
                    notification.image_path(url.as_str());
                }
                Err(err) => {
                    warn!("error converting screenshot path to file url: {err:?}");
                }
            },
            Err(err) => {
                warn!("error canonicalizing screenshot path: {err:?}");
            }
        }
    }

    if let Err(err) = notification.show() {
        warn!("error showing screenshot notification: {err:?}");
    }
}

#[inline(never)]
pub fn cause_panic() {
    let a = Duration::from_secs(1);
    let b = Duration::from_secs(2);
    let _ = a - b;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_preferring_top_left() {
        fn check(
            (ax, ay, aw, ah): (i32, i32, i32, i32),
            (rx, ry, rw, rh): (i32, i32, i32, i32),
            (ex, ey): (i32, i32),
        ) {
            let area = Rectangle::from_loc_and_size((ax, ay), (aw, ah)).to_f64();
            let mut rect = Rectangle::from_loc_and_size((rx, ry), (rw, rh)).to_f64();
            clamp_preferring_top_left_in_area(area, &mut rect);
            assert_eq!(rect.loc, Point::from((ex, ey)).to_f64());
        }

        check((0, 0, 10, 20), (2, 3, 4, 5), (2, 3));
        check((0, 0, 10, 20), (-2, 3, 4, 5), (0, 3));
        check((0, 0, 10, 20), (2, -3, 4, 5), (2, 0));
        check((0, 0, 10, 20), (-2, -3, 4, 5), (0, 0));

        check((1, 1, 10, 20), (2, 3, 4, 5), (2, 3));
        check((1, 1, 10, 20), (-2, 3, 4, 5), (1, 3));
        check((1, 1, 10, 20), (2, -3, 4, 5), (2, 1));
        check((1, 1, 10, 20), (-2, -3, 4, 5), (1, 1));

        check((0, 0, 10, 20), (20, 3, 4, 5), (6, 3));
        check((0, 0, 10, 20), (2, 30, 4, 5), (2, 15));
        check((0, 0, 10, 20), (20, 30, 4, 5), (6, 15));

        check((0, 0, 10, 20), (20, 30, 40, 5), (0, 15));
        check((0, 0, 10, 20), (20, 30, 4, 50), (6, 0));
        check((0, 0, 10, 20), (20, 30, 40, 50), (0, 0));
    }
}
