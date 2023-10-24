use std::ffi::OsStr;
use std::io::{self, Write};
use std::os::unix::prelude::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::ptr::null_mut;
use std::time::Duration;

use anyhow::{ensure, Context};
use directories::UserDirs;
use smithay::reexports::rustix::time::{clock_gettime, ClockId};
use smithay::utils::{Logical, Point, Rectangle};

pub fn get_monotonic_time() -> Duration {
    let ts = clock_gettime(ClockId::Monotonic);
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

pub fn center(rect: Rectangle<i32, Logical>) -> Point<i32, Logical> {
    rect.loc + rect.size.downscale(2).to_point()
}

pub fn make_screenshot_path() -> anyhow::Result<PathBuf> {
    let dirs = UserDirs::new().context("error retrieving home directory")?;
    let mut path = dirs.picture_dir().map(|p| p.to_owned()).unwrap_or_else(|| {
        let mut dir = dirs.home_dir().to_owned();
        dir.push("Pictures");
        dir
    });
    path.push("Screenshots");

    let mut buf = [0u8; 256];
    let name;
    unsafe {
        let time = libc::time(null_mut());
        ensure!(time != -1, "error in time()");

        let tm = libc::localtime(&time);
        ensure!(!tm.is_null(), "error in localtime()");

        let format = b"Screenshot from %Y-%m-%d %H-%M-%S.png\0";
        let rv = libc::strftime(
            buf.as_mut_ptr().cast(),
            buf.len(),
            format.as_ptr().cast(),
            tm,
        );
        ensure!(rv != 0, "error formatting time");

        name = OsStr::from_bytes(&buf[..rv]);
    }

    path.push(name);

    Ok(path)
}

/// Spawns the command to run independently of the compositor.
pub fn spawn(command: impl AsRef<OsStr>, args: impl IntoIterator<Item = impl AsRef<OsStr>>) {
    let _span = tracy_client::span!();

    let command = command.as_ref();

    let mut process = Command::new(command);
    process
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Double-fork to avoid having to waitpid the child.
    unsafe {
        process.pre_exec(|| {
            match libc::fork() {
                -1 => return Err(io::Error::last_os_error()),
                0 => (),
                _ => libc::_exit(0),
            }

            Ok(())
        });
    }

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(err) => {
            warn!("error spawning {command:?}: {err:?}");
            return;
        }
    };

    match child.wait() {
        Ok(status) => {
            if !status.success() {
                warn!("child did not exit successfully: {status:?}");
            }
        }
        Err(err) => {
            warn!("error waiting for child: {err:?}");
        }
    }
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
