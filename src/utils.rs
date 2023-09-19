use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context};
use directories::UserDirs;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::TextureBuffer;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::reexports::nix::time::{clock_gettime, ClockId};
use smithay::utils::{Logical, Physical, Point, Rectangle, Transform};
use time::OffsetDateTime;
use xcursor::parser::parse_xcursor;
use xcursor::CursorTheme;

const CURSOR_SIZE: u32 = 24;
static FALLBACK_CURSOR_DATA: &[u8] = include_bytes!("../resources/cursor.rgba");

pub fn get_monotonic_time() -> Duration {
    Duration::from(clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap())
}

pub fn center(rect: Rectangle<i32, Logical>) -> Point<i32, Logical> {
    rect.loc + rect.size.downscale(2).to_point()
}

fn load_xcursor() -> anyhow::Result<xcursor::parser::Image> {
    let theme = CursorTheme::load("default");
    let path = theme
        .load_icon("default")
        .ok_or_else(|| anyhow!("no default icon"))?;
    let mut file = File::open(path).context("error opening cursor icon file")?;
    let mut buf = vec![];
    file.read_to_end(&mut buf)
        .context("error reading cursor icon file")?;
    let images = parse_xcursor(&buf).context("error parsing cursor icon file")?;

    let nearest_image = images
        .iter()
        .min_by_key(|image| (CURSOR_SIZE as i32 - image.size as i32).abs())
        .unwrap();
    let frame = images
        .iter()
        .find(move |image| {
            image.width == nearest_image.width && image.height == nearest_image.height
        })
        .unwrap();
    Ok(frame.clone())
}

pub fn load_default_cursor(
    renderer: &mut GlesRenderer,
) -> (TextureBuffer<GlesTexture>, Point<i32, Physical>) {
    let frame = match load_xcursor() {
        Ok(frame) => frame,
        Err(err) => {
            warn!("error loading xcursor default cursor: {err:?}");

            xcursor::parser::Image {
                size: 32,
                width: 64,
                height: 64,
                xhot: 1,
                yhot: 1,
                delay: 1,
                pixels_rgba: Vec::from(FALLBACK_CURSOR_DATA),
                pixels_argb: vec![],
            }
        }
    };

    let texture = TextureBuffer::from_memory(
        renderer,
        &frame.pixels_rgba,
        Fourcc::Abgr8888,
        (frame.width as i32, frame.height as i32),
        false,
        1,
        Transform::Normal,
        None,
    )
    .unwrap();
    (texture, (frame.xhot as i32, frame.yhot as i32).into())
}

pub fn make_screenshot_path() -> anyhow::Result<PathBuf> {
    let dirs = UserDirs::new().context("error retrieving home directory")?;
    let mut path = dirs.picture_dir().map(|p| p.to_owned()).unwrap_or_else(|| {
        let mut dir = dirs.home_dir().to_owned();
        dir.push("Pictures");
        dir
    });
    path.push("Screenshots");

    unsafe {
        // are you kidding me
        time::util::local_offset::set_soundness(time::util::local_offset::Soundness::Unsound);
    };

    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let desc = time::macros::format_description!(
        "Screenshot from [year]-[month]-[day] [hour]-[minute]-[second].png"
    );
    let name = now.format(desc).context("error formatting time")?;
    path.push(name);

    Ok(path)
}
