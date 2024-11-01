use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use pango::{Alignment, FontDescription};
use pangocairo::cairo::{self, ImageSurface};
use smithay::backend::renderer::element::Kind;
use smithay::input::keyboard::{Keysym, ModifiersState};
use smithay::output::{Output, WeakOutput};
use smithay::reexports::gbm::Format as Fourcc;
use smithay::utils::{Scale, Transform};

use crate::render_helpers::memory::MemoryBuffer;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::utils::{output_size, to_physical_precise_round};

const PADDING: i32 = 16;
const FONT: &str = "sans 14px";
const BORDER: i32 = 8;

pub struct AccessDialog {
    requests: RefCell<VecDeque<AccessDialogRequest>>,
    buffers: RefCell<HashMap<WeakOutput, Option<MemoryBuffer>>>,
}

impl AccessDialog {
    pub fn new() -> Self {
        Self {
            requests: Default::default(),
            buffers: Default::default(),
        }
    }

    pub fn enque_request(&self, request: AccessDialogRequest) {
        self.requests.borrow_mut().push_back(request);
    }

    pub fn is_visible(&self) -> bool {
        !self.requests.borrow().is_empty()
    }

    pub fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
    ) -> Option<PrimaryGpuTextureRenderElement> {
        let requests = self.requests.borrow();
        let Some(current_request) = requests.front() else {
            return None;
        };

        let scale = output.current_scale().fractional_scale();
        let output_size = output_size(output);
        let weak_output = output.downgrade();

        let mut buffers = self.buffers.borrow_mut();
        buffers.retain(|output, buffer| {
            output.is_alive()
                && (*output != weak_output
                    || buffer
                        .as_ref()
                        .map(|buffer| Scale::from(scale) == buffer.scale())
                        .unwrap_or(false))
        });

        let Some(buffer) = buffers
            .entry(weak_output)
            .or_insert_with(|| render(scale, current_request).ok())
        else {
            buffers.clear();
            return None;
        };

        let size = buffer.logical_size();
        let buffer = TextureBuffer::from_memory_buffer(renderer.as_gles_renderer(), buffer).ok()?;

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
        Some(PrimaryGpuTextureRenderElement(elem))
    }

    pub fn handle_key(&self, raw: Option<Keysym>, _mods: ModifiersState) {
        if let Some(Keysym::Escape) = raw {
            if let Some(request) = self.requests.borrow_mut().pop_front() {
                let _ = request.deny();
            }
            self.buffers.borrow_mut().clear();
        }

        if let Some(Keysym::Return) = raw {
            if let Some(request) = self.requests.borrow_mut().pop_front() {
                // FIXME: Allow to select choices
                let _ = request.grant();
            }
            self.buffers.borrow_mut().clear();
        }
    }
}

#[derive(Debug)]
pub struct AccessDialogRequest {
    pub app_id: String,
    pub parent_window: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub body: Option<String>,
    pub options: AccessDialogOptions,

    response_channel_sender: async_channel::Sender<AccessDialogResponse>,
}

/// Options that might be used for a [`AccessDialogRequest`]
#[derive(Debug, Clone)]
pub struct AccessDialogOptions {
    /// Whether to make the dialog modal. Defaults to true.
    pub modal: bool,
    /// Label for the Deny button.
    pub deny_label: Option<String>,
    /// Label for the Grant button.
    pub grant_label: Option<String>,
    /// Icon name for an icon to show in the dialog. This should be a symbolic icon name.
    pub icon: Option<String>,
}

impl Default for AccessDialogOptions {
    fn default() -> Self {
        Self {
            modal: true,
            deny_label: None,
            grant_label: None,
            icon: None,
        }
    }
}

impl AccessDialogRequest {
    pub fn new(
        app_id: String,
        parent_window: Option<String>,
        title: String,
        subtitle: String,
        body: Option<String>,
        options: AccessDialogOptions,
    ) -> (Self, async_channel::Receiver<AccessDialogResponse>) {
        let (response_channel_sender, response_channel_receiver) = async_channel::bounded(1);
        let request = AccessDialogRequest {
            app_id,
            parent_window,
            title,
            subtitle,
            body,
            options,

            response_channel_sender,
        };
        (request, response_channel_receiver)
    }

    pub fn grant(self) -> anyhow::Result<()> {
        self.response_channel_sender
            .send_blocking(AccessDialogResponse::Grant)?;
        Ok(())
    }

    pub fn deny(self) -> anyhow::Result<()> {
        self.response_channel_sender
            .send_blocking(AccessDialogResponse::Deny)?;
        Ok(())
    }
}

pub enum AccessDialogResponse {
    Grant,
    Deny,
}

fn render(scale: f64, request: &AccessDialogRequest) -> anyhow::Result<MemoryBuffer> {
    let _span = tracy_client::span!("access_dialog_ui::render");

    let padding: i32 = to_physical_precise_round(scale, PADDING);

    let mut font = FontDescription::from_string(FONT);
    font.set_absolute_size(to_physical_precise_round(scale, font.size()));

    let text = format!(
        "<span weight='bold' underline='single'>{}</span>\n\
        {}\n\n\
        {}\
        Press <span face='mono' bgcolor='#2C2C2C'> Enter </span> to {} or <span face='mono' bgcolor='#2C2C2C'> Escape </span> to {}.",
        request.title,
        request.subtitle,
        request.body.as_deref().map(|body| format!("{}\n\n", body)).unwrap_or("".to_string()),
        request.options.grant_label.as_deref().unwrap_or("grant"),
        request.options.deny_label.as_deref().unwrap_or("deny"),
    );

    let surface = ImageSurface::create(cairo::Format::ARgb32, 0, 0)?;
    let cr = cairo::Context::new(&surface)?;
    let layout = pangocairo::functions::create_layout(&cr);
    layout.context().set_round_glyph_positions(false);
    layout.set_font_description(Some(&font));
    layout.set_alignment(Alignment::Center);
    layout.set_markup(&text);

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
    layout.set_markup(&text);

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
