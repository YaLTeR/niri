use std::cell::RefCell;
use std::collections::HashMap;

use pangocairo::cairo::{self, ImageSurface};
use pangocairo::pango::{Alignment, FontDescription};
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::{Element, Kind};
use smithay::output::Output;
use smithay::reexports::gbm::Format as Fourcc;
use smithay::utils::Transform;

use crate::render_helpers::NiriRenderer;

const TEXT: &str = "Are you sure you want to exit niri?\n\n\
                    Press <span face='mono' bgcolor='#2C2C2C'> Enter </span> to confirm.";
const PADDING: i32 = 16;
const FONT: &str = "sans 14px";
const BORDER: i32 = 8;

pub struct ExitConfirmDialog {
    is_open: bool,
    buffers: RefCell<HashMap<i32, Option<MemoryRenderBuffer>>>,
}

pub type ExitConfirmDialogRenderElement<R> =
    RelocateRenderElement<MemoryRenderBufferRenderElement<R>>;

impl ExitConfirmDialog {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            is_open: false,
            buffers: RefCell::new(HashMap::from([(1, Some(render(1)?))])),
        })
    }

    pub fn show(&mut self) -> bool {
        if !self.is_open {
            self.is_open = true;
            true
        } else {
            false
        }
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
    ) -> Option<ExitConfirmDialogRenderElement<R>> {
        if !self.is_open {
            return None;
        }

        let scale = output.current_scale().integer_scale();

        let mut buffers = self.buffers.borrow_mut();
        let fallback = buffers[&1].clone().unwrap();
        let buffer = buffers.entry(scale).or_insert_with(|| render(scale).ok());
        let buffer = buffer.as_ref().unwrap_or(&fallback);

        let elem = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (0., 0.),
            buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .ok()?;

        let output_transform = output.current_transform();
        let output_mode = output.current_mode().unwrap();
        let output_size = output_transform.transform_size(output_mode.size);

        let buffer_size = elem
            .geometry(output.current_scale().fractional_scale().into())
            .size;

        let x = (output_size.w / 2 - buffer_size.w / 2).max(0);
        let y = (output_size.h / 2 - buffer_size.h / 2).max(0);
        let elem = RelocateRenderElement::from_element(elem, (x, y), Relocate::Absolute);

        Some(elem)
    }
}

fn render(scale: i32) -> anyhow::Result<MemoryRenderBuffer> {
    let _span = tracy_client::span!("exit_confirm_dialog::render");

    let padding = PADDING * scale;

    let mut font = FontDescription::from_string(FONT);
    font.set_absolute_size((font.size() * scale).into());

    let surface = ImageSurface::create(cairo::Format::ARgb32, 0, 0)?;
    let cr = cairo::Context::new(&surface)?;
    let layout = pangocairo::functions::create_layout(&cr);
    layout.set_font_description(Some(&font));
    layout.set_alignment(Alignment::Center);
    layout.set_markup(TEXT);

    let (mut width, mut height) = layout.pixel_size();
    width += padding * 2;
    height += padding * 2;

    // FIXME: fix bug in Smithay that rounds pixel sizes down to scale.
    width = (width + scale - 1) / scale * scale;
    height = (height + scale - 1) / scale * scale;

    let surface = ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    let cr = cairo::Context::new(&surface)?;
    cr.set_source_rgb(0.1, 0.1, 0.1);
    cr.paint()?;

    cr.move_to(padding.into(), padding.into());
    let layout = pangocairo::functions::create_layout(&cr);
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
    cr.set_line_width((BORDER * scale).into());
    cr.stroke()?;
    drop(cr);

    let data = surface.take_data().unwrap();
    let buffer = MemoryRenderBuffer::from_slice(
        &data,
        Fourcc::Argb8888,
        (width, height),
        scale,
        Transform::Normal,
        None,
    );

    Ok(buffer)
}
