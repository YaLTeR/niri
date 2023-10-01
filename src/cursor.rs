use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use anyhow::{anyhow, Context};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::TextureBuffer;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::utils::{Physical, Point, Transform};
use xcursor::parser::{parse_xcursor, Image};
use xcursor::CursorTheme;

const CURSOR_SIZE: i32 = 24;
static FALLBACK_CURSOR_DATA: &[u8] = include_bytes!("../resources/cursor.rgba");

pub struct Cursor {
    images: Vec<Image>,
    cache: HashMap<i32, (TextureBuffer<GlesTexture>, Point<i32, Physical>)>,
}

impl Cursor {
    pub fn load() -> Self {
        let images = match load_xcursor() {
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

                let size = CURSOR_SIZE * scale;

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
}

fn load_xcursor() -> anyhow::Result<Vec<Image>> {
    let _span = tracy_client::span!();

    let theme = CursorTheme::load("default");
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
