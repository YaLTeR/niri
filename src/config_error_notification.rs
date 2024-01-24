use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use pangocairo::cairo::{self, ImageSurface};
use pangocairo::pango::FontDescription;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::{Element, Kind};
use smithay::output::Output;
use smithay::reexports::gbm::Format as Fourcc;
use smithay::utils::Transform;

use crate::animation::Animation;
use crate::render_helpers::NiriRenderer;

const TEXT: &str = "Failed to parse the config file. \
                    Please run <span face='monospace' bgcolor='#000000'>niri validate</span> \
                    to see the errors.";
const PADDING: i32 = 8;
const FONT: &str = "sans 14px";
const BORDER: i32 = 4;

pub struct ConfigErrorNotification {
    state: State,
    buffers: RefCell<HashMap<i32, Option<MemoryRenderBuffer>>>,
}

enum State {
    Hidden,
    Showing(Animation),
    Shown(Duration),
    Hiding(Animation),
}

pub type ConfigErrorNotificationRenderElement<R> =
    RelocateRenderElement<MemoryRenderBufferRenderElement<R>>;

impl ConfigErrorNotification {
    pub fn new() -> Self {
        Self {
            state: State::Hidden,
            buffers: RefCell::new(HashMap::new()),
        }
    }

    pub fn show(&mut self) {
        // Show from scratch even if already showing to bring attention.
        self.state = State::Showing(Animation::new(0., 1., Duration::from_millis(250)));
    }

    pub fn hide(&mut self) {
        if matches!(self.state, State::Hidden) {
            return;
        }

        self.state = State::Hiding(Animation::new(1., 0., Duration::from_millis(250)));
    }

    pub fn advance_animations(&mut self, target_presentation_time: Duration) {
        match &mut self.state {
            State::Hidden => (),
            State::Showing(anim) => {
                anim.set_current_time(target_presentation_time);
                if anim.is_done() {
                    self.state = State::Shown(target_presentation_time + Duration::from_secs(4));
                }
            }
            State::Shown(deadline) => {
                if target_presentation_time >= *deadline {
                    self.hide();
                }
            }
            State::Hiding(anim) => {
                anim.set_current_time(target_presentation_time);
                if anim.is_done() {
                    self.state = State::Hidden;
                }
            }
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        !matches!(self.state, State::Hidden)
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
    ) -> Option<ConfigErrorNotificationRenderElement<R>> {
        if matches!(self.state, State::Hidden) {
            return None;
        }

        let scale = output.current_scale().integer_scale();

        let mut buffers = self.buffers.borrow_mut();
        let buffer = buffers
            .entry(scale)
            .or_insert_with_key(move |&scale| render(scale).ok());
        let buffer = buffer.as_ref()?;

        let elem = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (0., 0.),
            buffer,
            Some(0.9),
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

        let y_range = buffer_size.h + PADDING * 2 * scale;

        let x = (output_size.w / 2 - buffer_size.w / 2).max(0);
        let y = match &self.state {
            State::Hidden => unreachable!(),
            State::Showing(anim) | State::Hiding(anim) => {
                (-buffer_size.h as f64 + anim.value() * y_range as f64).round() as i32
            }
            State::Shown(_) => PADDING * 2 * scale,
        };
        let elem = RelocateRenderElement::from_element(elem, (x, y), Relocate::Absolute);

        Some(elem)
    }
}

fn render(scale: i32) -> anyhow::Result<MemoryRenderBuffer> {
    let _span = tracy_client::span!("config_error_notification::render");

    let padding = PADDING * scale;

    let mut font = FontDescription::from_string(FONT);
    font.set_absolute_size((font.size() * scale).into());

    let surface = ImageSurface::create(cairo::Format::ARgb32, 0, 0)?;
    let cr = cairo::Context::new(&surface)?;
    let layout = pangocairo::create_layout(&cr);
    layout.set_font_description(Some(&font));
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
    let layout = pangocairo::create_layout(&cr);
    layout.set_font_description(Some(&font));
    layout.set_markup(TEXT);

    cr.set_source_rgb(1., 1., 1.);
    pangocairo::show_layout(&cr, &layout);

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
