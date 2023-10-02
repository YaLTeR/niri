use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;

use anyhow::{anyhow, Context};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::TextureBuffer;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::utils::{Physical, Point, Transform};
use xcursor::parser::{parse_xcursor, Image};
use xcursor::CursorTheme;

static FALLBACK_CURSOR_DATA: &[u8] = include_bytes!("../resources/cursor.rgba");

pub struct Cursor {
    images: Vec<Image>,
    size: i32,
    cache: HashMap<i32, (TextureBuffer<GlesTexture>, Point<i32, Physical>)>,
}

impl Cursor {
    /// Load the said theme as well as set the `XCURSOR_THEME` and `XCURSOR_SIZE`
    /// env variables.
    pub fn load(theme: &str, size: u8) -> Self {
        env::set_var("XCURSOR_THEME", theme);
        env::set_var("XCURSOR_SIZE", size.to_string());

        let images = match load_xcursor(theme) {
            Ok(images) => images,
            Err(err) => {
                warn!("error loading xcursor default cursor: {err:?}");

                vec![Image {
                    size: 32,
                    width: 64,
                    height: 64,
                    xhot: 1,
                    yhot: 1,
                    delay: 1,
                    pixels_rgba: Vec::from(FALLBACK_CURSOR_DATA),
                    pixels_argb: vec![],
                }]
            }
        };

        Self {
            images,
            size: size as i32,
            cache: HashMap::new(),
        }
    }

    pub fn get(
        &mut self,
        renderer: &mut GlesRenderer,
        scale: i32,
    ) -> (TextureBuffer<GlesTexture>, Point<i32, Physical>) {
        self.cache
            .entry(scale)
            .or_insert_with_key(|scale| {
                let _span = tracy_client::span!("create cursor texture");

                let size = self.size * scale;

                let nearest_image = self
                    .images
                    .iter()
                    .min_by_key(|image| (size - image.size as i32).abs())
                    .unwrap();
                let frame = self
                    .images
                    .iter()
                    .find(move |image| {
                        image.width == nearest_image.width && image.height == nearest_image.height
                    })
                    .unwrap();

                let texture = TextureBuffer::from_memory(
                    renderer,
                    &frame.pixels_rgba,
                    Fourcc::Abgr8888,
                    (frame.width as i32, frame.height as i32),
                    false,
                    *scale,
                    Transform::Normal,
                    None,
                )
                .unwrap();
                (texture, (frame.xhot as i32, frame.yhot as i32).into())
            })
            .clone()
    }

    pub fn get_cached_hotspot(&self, scale: i32) -> Option<Point<i32, Physical>> {
        self.cache.get(&scale).map(|(_, hotspot)| *hotspot)
    }
}

fn load_xcursor(theme: &str) -> anyhow::Result<Vec<Image>> {
    let _span = tracy_client::span!();

    let theme = CursorTheme::load(theme);
    let path = theme
        .load_icon("default")
        .ok_or_else(|| anyhow!("no default icon"))?;
    let mut file = File::open(path).context("error opening cursor icon file")?;
    let mut buf = vec![];
    file.read_to_end(&mut buf)
        .context("error reading cursor icon file")?;
    let images = parse_xcursor(&buf).context("error parsing cursor icon file")?;

    Ok(images)
}
