use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

use arrayvec::ArrayVec;
use ordered_float::NotNan;
use pangocairo::cairo::{self, ImageSurface};
use pangocairo::pango::{Alignment, FontDescription};
use smithay::backend::renderer::element::Kind;
use smithay::output::Output;
use smithay::reexports::gbm::Format as Fourcc;
use smithay::utils::{Point, Transform};

use crate::niri_render_elements;
use crate::render_helpers::memory::MemoryBuffer;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::utils::{output_size, to_physical_precise_round};

const TEXT: &str = "Are you sure you want to exit niri?\n\n\
                    Press <span face='mono' bgcolor='#2C2C2C'> Enter </span> to confirm.";
const PADDING: i32 = 16;
const FONT: &str = "sans 14px";
const BORDER: i32 = 8;
const BACKDROP_COLOR: [f32; 4] = [0., 0., 0., 0.4];

pub struct ExitConfirmDialog {
    is_open: bool,
    buffers: RefCell<HashMap<NotNan<f64>, Option<MemoryBuffer>>>,
}

niri_render_elements! {
    ExitConfirmDialogRenderElement => {
        Texture = PrimaryGpuTextureRenderElement,
        SolidColor = SolidColorRenderElement,
    }
}

struct OutputData {
    backdrop: SolidColorBuffer,
}

impl ExitConfirmDialog {
    pub fn new() -> Self {
        let buffer = match render(1.) {
            Ok(x) => Some(x),
            Err(err) => {
                warn!("error creating the exit confirm dialog: {err:?}");
                None
            }
        };

        Self {
            is_open: false,
            buffers: RefCell::new(HashMap::from([(NotNan::new(1.).unwrap(), buffer)])),
        }
    }

    pub fn can_show(&self) -> bool {
        let buffers = self.buffers.borrow();
        let fallback = &buffers[&NotNan::new(1.).unwrap()];
        fallback.is_some()
    }

    pub fn show(&mut self) -> bool {
        if !self.can_show() {
            return false;
        }

        self.is_open = true;
        true
    }

    pub fn hide(&mut self) -> bool {
        if self.is_open {
            self.is_open = false;
            true
        } else {
            false
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
    ) -> ArrayVec<ExitConfirmDialogRenderElement, 2> {
        let mut rv = ArrayVec::new();

        if !self.is_open {
            return rv;
        }

        let scale = output.current_scale().fractional_scale();
        let output_size = output_size(output);

        let mut buffers = self.buffers.borrow_mut();
        let Some(fallback) = buffers[&NotNan::new(1.).unwrap()].clone() else {
            error!("exit confirm dialog opened without fallback buffer");
            return rv;
        };

        let buffer = buffers
            .entry(NotNan::new(scale).unwrap())
            .or_insert_with(|| render(scale).ok());
        let buffer = buffer.as_ref().unwrap_or(&fallback);

        let size = buffer.logical_size();
        let Ok(buffer) = TextureBuffer::from_memory_buffer(renderer.as_gles_renderer(), buffer)
        else {
            return rv;
        };

        let location = (output_size.to_f64().to_point() - size.to_point()).downscale(2.);
        let mut location = location.to_physical_precise_round(scale).to_logical(scale);
        location.x = f64::max(0., location.x);
        location.y = f64::max(0., location.y);

        let elem = TextureRenderElement::from_texture_buffer(
            buffer,
            location,
            1.,
            None,
            None,
            Kind::Unspecified,
        );
        rv.push(ExitConfirmDialogRenderElement::Texture(
            PrimaryGpuTextureRenderElement(elem),
        ));

        // Backdrop.
        let data = output.user_data().get_or_insert(|| {
            Mutex::new(OutputData {
                backdrop: SolidColorBuffer::new(output_size, BACKDROP_COLOR),
            })
        });
        let mut data = data.lock().unwrap();
        data.backdrop.resize(output_size);

        let elem = SolidColorRenderElement::from_buffer(
            &data.backdrop,
            Point::new(0., 0.),
            1.,
            Kind::Unspecified,
        );
        rv.push(ExitConfirmDialogRenderElement::SolidColor(elem));

        rv
    }
}

fn render(scale: f64) -> anyhow::Result<MemoryBuffer> {
    let _span = tracy_client::span!("exit_confirm_dialog::render");

    let padding: i32 = to_physical_precise_round(scale, PADDING);

    let mut font = FontDescription::from_string(FONT);
    font.set_absolute_size(to_physical_precise_round(scale, font.size()));

    let surface = ImageSurface::create(cairo::Format::ARgb32, 0, 0)?;
    let cr = cairo::Context::new(&surface)?;
    let layout = pangocairo::functions::create_layout(&cr);
    layout.context().set_round_glyph_positions(false);
    layout.set_font_description(Some(&font));
    layout.set_alignment(Alignment::Center);
    layout.set_markup(TEXT);

    let (mut width, mut height) = layout.pixel_size();
    width += padding * 2;
    height += padding * 2;

    let surface = ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    let cr = cairo::Context::new(&surface)?;
    cr.set_source_rgb(0.1, 0.1, 0.1);
    cr.paint()?;

    cr.move_to(padding.into(), padding.into());
    let layout = pangocairo::functions::create_layout(&cr);
    layout.context().set_round_glyph_positions(false);
    layout.set_font_description(Some(&font));
    layout.set_alignment(Alignment::Center);
    layout.set_markup(TEXT);

    cr.set_source_rgb(1., 1., 1.);
    pangocairo::functions::show_layout(&cr, &layout);

    cr.move_to(0., 0.);
    cr.line_to(width.into(), 0.);
    cr.line_to(width.into(), height.into());
    cr.line_to(0., height.into());
    cr.line_to(0., 0.);
    cr.set_source_rgb(1., 0.3, 0.3);
    // Keep the border width even to avoid blurry edges.
    cr.set_line_width((f64::from(BORDER) / 2. * scale).round() * 2.);
    cr.stroke()?;
    drop(cr);

    let data = surface.take_data().unwrap();
    let buffer = MemoryBuffer::new(
        data.to_vec(),
        Fourcc::Argb8888,
        (width, height),
        scale,
        Transform::Normal,
    );

    Ok(buffer)
}
