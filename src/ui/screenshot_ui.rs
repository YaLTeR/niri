use std::cell::RefCell;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::iter::zip;
use std::rc::Rc;

use anyhow::Context;
use arrayvec::ArrayVec;
use niri_config::{Action, Config};
use pango::{Alignment, FontDescription};
use pangocairo::cairo::{self, ImageSurface};
use smithay::backend::allocator::Fourcc;
use smithay::backend::input::{ButtonState, MouseButton};
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{ExportMem, Texture as _};
use smithay::input::keyboard::{Keysym, ModifiersState};
use smithay::output::{Output, WeakOutput};
use smithay::utils::{Physical, Point, Rectangle, Scale, Size, Transform};

use crate::animation::{Animation, Clock};
use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::render_helpers::{render_to_texture, RenderTarget};
use crate::utils::to_physical_precise_round;

const SELECTION_BORDER: i32 = 2;

const PADDING: i32 = 8;
const FONT: &str = "sans 14px";
const BORDER: i32 = 4;
const TEXT_HIDE_P: &str =
    "Press <span face='mono' bgcolor='#2C2C2C'> Space </span> to save the screenshot.\n\
     Press <span face='mono' bgcolor='#2C2C2C'> P </span> to hide the pointer.";
const TEXT_SHOW_P: &str =
    "Press <span face='mono' bgcolor='#2C2C2C'> Space </span> to save the screenshot.\n\
     Press <span face='mono' bgcolor='#2C2C2C'> P </span> to show the pointer.";

/// How many pixels the move commands move the selection.
const DIRECTIONAL_MOVE_PX: i32 = 50;

/// Minimum size for screenshot selection in pixels, only applies to keybinds.
const MIN_SELECTION_SIZE: i32 = 10;

// Ideally the screenshot UI should support cross-output selections. However, that poses some
// technical challenges when the outputs have different scales and such. So, this implementation
// allows only single-output selections for now.
//
// As a consequence of this, selection coordinates are in output-local coordinate space.
#[allow(clippy::large_enum_variant)]
pub enum ScreenshotUi {
    Closed {
        last_selection: Option<(WeakOutput, Rectangle<i32, Physical>)>,
        clock: Clock,
        config: Rc<RefCell<Config>>,
    },
    Open {
        selection: (Output, Point<i32, Physical>, Point<i32, Physical>),
        output_data: HashMap<Output, OutputData>,
        mouse_down: bool,
        show_pointer: bool,
        open_anim: Animation,
        clock: Clock,
        config: Rc<RefCell<Config>>,
    },
}

pub struct OutputData {
    size: Size<i32, Physical>,
    scale: f64,
    transform: Transform,
    // Output, screencast, screen capture.
    screenshot: [OutputScreenshot; 3],
    buffers: [SolidColorBuffer; 8],
    locations: [Point<i32, Physical>; 8],
    panel: Option<(TextureBuffer<GlesTexture>, TextureBuffer<GlesTexture>)>,
}

pub struct OutputScreenshot {
    texture: GlesTexture,
    buffer: PrimaryGpuTextureRenderElement,
    pointer: Option<PrimaryGpuTextureRenderElement>,
}

niri_render_elements! {
    ScreenshotUiRenderElement => {
        Screenshot = PrimaryGpuTextureRenderElement,
        SolidColor = SolidColorRenderElement,
    }
}

impl ScreenshotUi {
    pub fn new(clock: Clock, config: Rc<RefCell<Config>>) -> Self {
        Self::Closed {
            last_selection: None,
            clock,
            config,
        }
    }

    pub fn open(
        &mut self,
        renderer: &mut GlesRenderer,
        // Output, screencast, screen capture.
        screenshots: HashMap<Output, [OutputScreenshot; 3]>,
        default_output: Output,
        show_pointer: bool,
    ) -> bool {
        if screenshots.is_empty() {
            return false;
        }

        let Self::Closed {
            last_selection,
            clock,
            config,
        } = self
        else {
            return false;
        };

        let last_selection = last_selection
            .take()
            .and_then(|(weak, sel)| weak.upgrade().map(|output| (output, sel)));
        let selection = match last_selection {
            Some(selection) if screenshots.contains_key(&selection.0) => selection,
            _ => {
                let output = default_output;
                let output_transform = output.current_transform();
                let output_mode = output.current_mode().unwrap();
                let size = output_transform.transform_size(output_mode.size);
                (
                    output,
                    Rectangle::new(
                        Point::from((size.w / 4, size.h / 4)),
                        Size::from((size.w / 2, size.h / 2)),
                    ),
                )
            }
        };

        let selection = (
            selection.0,
            selection.1.loc,
            selection.1.loc + selection.1.size - Size::from((1, 1)),
        );

        let output_data = screenshots
            .into_iter()
            .map(|(output, screenshot)| {
                let transform = output.current_transform();
                let output_mode = output.current_mode().unwrap();
                let size = transform.transform_size(output_mode.size);
                let scale = output.current_scale().fractional_scale();
                let buffers = [
                    SolidColorBuffer::new((0., 0.), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0., 0.), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0., 0.), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0., 0.), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0., 0.), [0., 0., 0., 0.5]),
                    SolidColorBuffer::new((0., 0.), [0., 0., 0., 0.5]),
                    SolidColorBuffer::new((0., 0.), [0., 0., 0., 0.5]),
                    SolidColorBuffer::new((0., 0.), [0., 0., 0., 0.5]),
                ];
                let locations = [Default::default(); 8];

                let mut render_panel_ = |text| {
                    render_panel(renderer, scale, text)
                        .map_err(|err| warn!("error rendering help panel: {err:?}"))
                        .ok()
                };
                let panel_show = render_panel_(TEXT_SHOW_P);
                let panel_hide = render_panel_(TEXT_HIDE_P);
                let panel = Option::zip(panel_show, panel_hide);

                let data = OutputData {
                    size,
                    scale,
                    transform,
                    screenshot,
                    buffers,
                    locations,
                    panel,
                };
                (output, data)
            })
            .collect();

        let open_anim = {
            let c = config.borrow();
            Animation::new(clock.clone(), 0., 1., 0., c.animations.screenshot_ui_open.0)
        };

        *self = Self::Open {
            selection,
            output_data,
            mouse_down: false,
            show_pointer,
            open_anim,
            clock: clock.clone(),
            config: config.clone(),
        };

        self.update_buffers();

        true
    }

    pub fn close(&mut self) -> bool {
        let Self::Open {
            selection,
            clock,
            config,
            ..
        } = self
        else {
            return false;
        };

        let last_selection = Some((
            selection.0.downgrade(),
            rect_from_corner_points(selection.1, selection.2),
        ));

        *self = Self::Closed {
            last_selection,
            clock: clock.clone(),
            config: config.clone(),
        };

        true
    }

    pub fn toggle_pointer(&mut self) {
        if let Self::Open { show_pointer, .. } = self {
            *show_pointer = !*show_pointer;
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self, ScreenshotUi::Open { .. })
    }

    pub fn move_left(&mut self) {
        if let Self::Open { selection, .. } = self {
            selection.1.x -= DIRECTIONAL_MOVE_PX;
            selection.2.x -= DIRECTIONAL_MOVE_PX;
            self.update_buffers();
        }
    }

    pub fn move_right(&mut self) {
        if let Self::Open { selection, .. } = self {
            selection.1.x += DIRECTIONAL_MOVE_PX;
            selection.2.x += DIRECTIONAL_MOVE_PX;
            self.update_buffers();
        }
    }

    pub fn move_up(&mut self) {
        if let Self::Open { selection, .. } = self {
            selection.1.y -= DIRECTIONAL_MOVE_PX;
            selection.2.y -= DIRECTIONAL_MOVE_PX;
            self.update_buffers();
        }
    }

    pub fn move_down(&mut self) {
        if let Self::Open { selection, .. } = self {
            selection.1.y += DIRECTIONAL_MOVE_PX;
            selection.2.y += DIRECTIONAL_MOVE_PX;
            self.update_buffers();
        }
    }

    pub fn resize_left(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_x = selection.1.x - DIRECTIONAL_MOVE_PX;
            if selection.2.x - new_x >= MIN_SELECTION_SIZE {
                selection.1.x = new_x;
                self.update_buffers();
            }
        }
    }

    pub fn resize_right(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_x = selection.2.x + DIRECTIONAL_MOVE_PX;
            if new_x - selection.1.x >= MIN_SELECTION_SIZE {
                selection.2.x = new_x;
                self.update_buffers();
            }
        }
    }

    pub fn resize_up(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_y = selection.1.y - DIRECTIONAL_MOVE_PX;
            if selection.2.y - new_y >= MIN_SELECTION_SIZE {
                selection.1.y = new_y;
                self.update_buffers();
            }
        }
    }

    pub fn resize_down(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_y = selection.2.y + DIRECTIONAL_MOVE_PX;
            if new_y - selection.1.y >= MIN_SELECTION_SIZE {
                selection.2.y = new_y;
                self.update_buffers();
            }
        }
    }

    pub fn resize_inward_left(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_x = selection.2.x - DIRECTIONAL_MOVE_PX;
            if new_x >= selection.1.x + MIN_SELECTION_SIZE {
                selection.2.x = new_x;
                self.update_buffers();
            }
        }
    }

    pub fn resize_inward_right(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_x = selection.1.x + DIRECTIONAL_MOVE_PX;
            if new_x <= selection.2.x - MIN_SELECTION_SIZE {
                selection.1.x = new_x;
                self.update_buffers();
            }
        }
    }

    pub fn resize_inward_up(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_y = selection.2.y - DIRECTIONAL_MOVE_PX;
            if new_y >= selection.1.y + MIN_SELECTION_SIZE {
                selection.2.y = new_y;
                self.update_buffers();
            }
        }
    }

    pub fn resize_inward_down(&mut self) {
        if let Self::Open { selection, .. } = self {
            let new_y = selection.1.y + DIRECTIONAL_MOVE_PX;
            if new_y <= selection.2.y - MIN_SELECTION_SIZE {
                selection.1.y = new_y;
                self.update_buffers();
            }
        }
    }

    pub fn advance_animations(&mut self) {}

    pub fn are_animations_ongoing(&self) -> bool {
        let Self::Open { open_anim, .. } = self else {
            return false;
        };

        !open_anim.is_done()
    }

    fn update_buffers(&mut self) {
        let Self::Open {
            selection,
            output_data,
            ..
        } = self
        else {
            panic!("screenshot UI must be open to update buffers");
        };

        let (selection_output, a, b) = selection;
        let mut rect = rect_from_corner_points(*a, *b);

        for (output, data) in output_data {
            let buffers = &mut data.buffers;
            let locations = &mut data.locations;
            let size = data.size;
            let scale = data.scale;

            if output == selection_output {
                // Check if the selection is still valid. If not, reset it back to default.
                if !Rectangle::from_size(size).contains_rect(rect) {
                    rect = Rectangle::new(
                        Point::from((size.w / 4, size.h / 4)),
                        Size::from((size.w / 2, size.h / 2)),
                    );
                    *a = rect.loc;
                    *b = rect.loc + rect.size - Size::from((1, 1));
                }

                let border = to_physical_precise_round(scale, SELECTION_BORDER);

                let resize = move |buffer: &mut SolidColorBuffer, w: i32, h: i32| {
                    let size = Size::<_, Physical>::from((w, h));
                    buffer.resize(size.to_f64().to_logical(scale));
                };

                resize(&mut buffers[0], rect.size.w + border * 2, border);
                resize(&mut buffers[1], rect.size.w + border * 2, border);
                resize(&mut buffers[2], border, rect.size.h);
                resize(&mut buffers[3], border, rect.size.h);

                resize(&mut buffers[4], size.w, rect.loc.y);
                resize(&mut buffers[5], size.w, size.h - rect.loc.y - rect.size.h);
                resize(&mut buffers[6], rect.loc.x, rect.size.h);
                resize(
                    &mut buffers[7],
                    size.w - rect.loc.x - rect.size.w,
                    rect.size.h,
                );

                locations[0] = Point::from((rect.loc.x - border, rect.loc.y - border));
                locations[1] = Point::from((rect.loc.x - border, rect.loc.y + rect.size.h));
                locations[2] = Point::from((rect.loc.x - border, rect.loc.y));
                locations[3] = Point::from((rect.loc.x + rect.size.w, rect.loc.y));

                locations[5] = Point::from((0, rect.loc.y + rect.size.h));
                locations[6] = Point::from((0, rect.loc.y));
                locations[7] = Point::from((rect.loc.x + rect.size.w, rect.loc.y));
            } else {
                buffers[0].resize((0., 0.));
                buffers[1].resize((0., 0.));
                buffers[2].resize((0., 0.));
                buffers[3].resize((0., 0.));

                buffers[4].resize(size.to_f64().to_logical(data.scale));
                buffers[5].resize((0., 0.));
                buffers[6].resize((0., 0.));
                buffers[7].resize((0., 0.));
            }
        }
    }

    pub fn render_output(
        &self,
        output: &Output,
        target: RenderTarget,
    ) -> ArrayVec<ScreenshotUiRenderElement, 11> {
        let _span = tracy_client::span!("ScreenshotUi::render_output");

        let Self::Open {
            output_data,
            show_pointer,
            mouse_down,
            open_anim,
            ..
        } = self
        else {
            panic!("screenshot UI must be open to render it");
        };

        let mut elements = ArrayVec::new();

        let Some(output_data) = output_data.get(output) else {
            return elements;
        };

        let scale = output_data.scale;
        let progress = open_anim.clamped_value().clamp(0., 1.) as f32;

        // The help panel goes on top.
        if let Some((show, hide)) = &output_data.panel {
            let buffer = if *show_pointer { hide } else { show };

            let size = buffer.texture().size();
            let padding: i32 = to_physical_precise_round(scale, PADDING);
            let x = max(0, (output_data.size.w - size.w) / 2);
            let y = max(0, output_data.size.h - size.h - padding * 2);
            let location = Point::<_, Physical>::from((x, y))
                .to_f64()
                .to_logical(scale);

            let alpha = if *mouse_down { 0.3 } else { 0.9 };

            let elem = PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
                buffer.clone(),
                location,
                alpha * progress,
                None,
                None,
                Kind::Unspecified,
            ));
            elements.push(elem.into());
        }

        let buf_loc = zip(&output_data.buffers, &output_data.locations);
        elements.extend(buf_loc.map(|(buffer, loc)| {
            SolidColorRenderElement::from_buffer(
                buffer,
                loc.to_f64().to_logical(scale),
                progress,
                Kind::Unspecified,
            )
            .into()
        }));

        // The screenshot itself goes last.
        let index = match target {
            RenderTarget::Output => 0,
            RenderTarget::Screencast => 1,
            RenderTarget::ScreenCapture => 2,
        };
        let screenshot = &output_data.screenshot[index];

        if *show_pointer {
            if let Some(pointer) = screenshot.pointer.clone() {
                elements.push(pointer.into());
            }
        }
        elements.push(screenshot.buffer.clone().into());

        elements
    }

    pub fn capture(
        &self,
        renderer: &mut GlesRenderer,
    ) -> anyhow::Result<(Size<i32, Physical>, Vec<u8>)> {
        let _span = tracy_client::span!("ScreenshotUi::capture");

        let Self::Open {
            selection,
            output_data,
            show_pointer,
            ..
        } = self
        else {
            panic!("screenshot UI must be open to capture");
        };

        let data = &output_data[&selection.0];
        let rect = rect_from_corner_points(selection.1, selection.2);

        let screenshot = &data.screenshot[0];

        // Composite the pointer on top if needed.
        let mut tex_rect = None;
        if *show_pointer {
            if let Some(pointer) = screenshot.pointer.clone() {
                let scale = pointer.0.buffer().texture_scale();
                let offset = rect.loc.upscale(-1);

                let mut elements = ArrayVec::<_, 2>::new();
                elements.push(pointer);
                elements.push(screenshot.buffer.clone());
                let elements = elements.iter().rev().map(|elem| {
                    RelocateRenderElement::from_element(elem, offset, Relocate::Relative)
                });

                let res = render_to_texture(
                    renderer,
                    rect.size,
                    scale,
                    Transform::Normal,
                    Fourcc::Abgr8888,
                    elements,
                );
                match res {
                    Ok((texture, _)) => {
                        tex_rect = Some((texture, Rectangle::from_size(rect.size)));
                    }
                    Err(err) => {
                        warn!("error compositing pointer onto screenshot: {err:?}");
                    }
                }
            }
        }

        let (texture, rect) = tex_rect.unwrap_or_else(|| (screenshot.texture.clone(), rect));
        // The size doesn't actually matter because we're not transforming anything.
        let buf_rect = rect
            .to_logical(1)
            .to_buffer(1, Transform::Normal, &Size::from((1, 1)));

        let mapping = renderer
            .copy_texture(&texture, buf_rect, Fourcc::Abgr8888)
            .context("error copying texture")?;
        let copy = renderer
            .map_texture(&mapping)
            .context("error mapping texture")?;

        Ok((rect.size, copy.to_vec()))
    }

    pub fn action(&self, raw: Keysym, mods: ModifiersState) -> Option<Action> {
        if !matches!(self, Self::Open { .. }) {
            return None;
        }

        action(raw, mods)
    }

    pub fn selection_output(&self) -> Option<&Output> {
        if let Self::Open {
            selection: (output, _, _),
            ..
        } = self
        {
            Some(output)
        } else {
            None
        }
    }

    pub fn output_size(&self, output: &Output) -> Option<(Size<i32, Physical>, f64, Transform)> {
        if let Self::Open { output_data, .. } = self {
            let data = output_data.get(output)?;
            Some((data.size, data.scale, data.transform))
        } else {
            None
        }
    }

    /// The pointer has moved to `point` relative to the current selection output.
    pub fn pointer_motion(&mut self, point: Point<i32, Physical>) {
        let Self::Open {
            selection,
            mouse_down: true,
            ..
        } = self
        else {
            return;
        };

        selection.2 = point;
        self.update_buffers();
    }

    pub fn pointer_button(
        &mut self,
        output: Output,
        point: Point<i32, Physical>,
        button: MouseButton,
        state: ButtonState,
    ) -> bool {
        let Self::Open {
            selection,
            output_data,
            mouse_down,
            ..
        } = self
        else {
            return false;
        };

        if button != MouseButton::Left {
            return false;
        }

        let down = state == ButtonState::Pressed;
        if *mouse_down == down {
            return false;
        }

        if down && !output_data.contains_key(&output) {
            return false;
        }

        *mouse_down = down;

        if down {
            *selection = (output, point, point);
        } else {
            // Check if the resulting selection is zero-sized, and try to come up with a small
            // default rectangle.
            let (output, a, b) = selection;
            let mut rect = rect_from_corner_points(*a, *b);
            if rect.size.is_empty() || rect.size == Size::from((1, 1)) {
                let data = &output_data[output];
                rect = Rectangle::new(
                    Point::from((rect.loc.x - 16, rect.loc.y - 16)),
                    Size::from((32, 32)),
                )
                .intersection(Rectangle::from_size(data.size))
                .unwrap_or_default();
                *a = rect.loc;
                *b = rect.loc + rect.size - Size::from((1, 1));
            }
        }

        self.update_buffers();

        true
    }
}

impl OutputScreenshot {
    pub fn from_textures(
        renderer: &mut GlesRenderer,
        scale: Scale<f64>,
        texture: GlesTexture,
        pointer: Option<(GlesTexture, Rectangle<i32, Physical>)>,
    ) -> Self {
        let buffer = PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
            TextureBuffer::from_texture(
                renderer,
                texture.clone(),
                scale,
                Transform::Normal,
                Vec::new(),
            ),
            (0., 0.),
            1.,
            None,
            None,
            Kind::Unspecified,
        ));

        let pointer = pointer.map(|(texture, geo)| {
            PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
                TextureBuffer::from_texture(
                    renderer,
                    texture,
                    scale,
                    Transform::Normal,
                    Vec::new(),
                ),
                geo.to_f64().to_logical(scale).loc,
                1.,
                None,
                None,
                Kind::Unspecified,
            ))
        });

        Self {
            texture,
            buffer,
            pointer,
        }
    }
}

fn action(raw: Keysym, mods: ModifiersState) -> Option<Action> {
    if raw == Keysym::Escape {
        return Some(Action::CancelScreenshot);
    }

    if mods.alt || mods.shift {
        return None;
    }

    if !mods.ctrl && (raw == Keysym::space || raw == Keysym::Return) {
        return Some(Action::ConfirmScreenshot {
            write_to_disk: true,
        });
    }
    if mods.ctrl && raw == Keysym::c {
        return Some(Action::ConfirmScreenshot {
            write_to_disk: false,
        });
    }

    if !mods.ctrl && raw == Keysym::p {
        return Some(Action::ScreenshotTogglePointer);
    }

    match raw {
        // Move.
        Keysym::Left if mods.ctrl => return Some(Action::MoveScreenshotLeft),
        Keysym::Right if mods.ctrl => return Some(Action::MoveScreenshotRight),
        Keysym::Up if mods.ctrl => return Some(Action::MoveScreenshotUp),
        Keysym::Down if mods.ctrl => return Some(Action::MoveScreenshotDown),

        // Resize.
        Keysym::Left if mods.logo => return Some(Action::ResizeScreenshotLeft),
        Keysym::Right if mods.logo => return Some(Action::ResizeScreenshotRight),
        Keysym::Up if mods.logo => return Some(Action::ResizeScreenshotUp),
        Keysym::Down if mods.logo => return Some(Action::ResizeScreenshotDown),

        // Resize inward.
        Keysym::Left if mods.alt => return Some(Action::ResizeScreenshotInwardLeft),
        Keysym::Right if mods.alt => return Some(Action::ResizeScreenshotInwardRight),
        Keysym::Up if mods.alt => return Some(Action::ResizeScreenshotInwardUp),
        Keysym::Down if mods.alt => return Some(Action::ResizeScreenshotInwardDown),

        _ => {}
    }

    None
}

pub fn rect_from_corner_points(
    a: Point<i32, Physical>,
    b: Point<i32, Physical>,
) -> Rectangle<i32, Physical> {
    let x1 = min(a.x, b.x);
    let y1 = min(a.y, b.y);
    let x2 = max(a.x, b.x);
    let y2 = max(a.y, b.y);
    // We're adding + 1 because the pointer is clamped to output size - 1, so to get the full
    // screen worth of selection we must add back that + 1.
    Rectangle::from_extremities((x1, y1), (x2 + 1, y2 + 1))
}

fn render_panel(
    renderer: &mut GlesRenderer,
    scale: f64,
    text: &str,
) -> anyhow::Result<TextureBuffer<GlesTexture>> {
    let _span = tracy_client::span!("screenshot_ui::render_panel");

    let padding: i32 = to_physical_precise_round(scale, PADDING);

    // Add 2 px of spacing to separate the backgrounds of the "Space" and "P" keys.
    let spacing = to_physical_precise_round::<i32>(scale, 2) * 1024;

    let mut font = FontDescription::from_string(FONT);
    font.set_absolute_size(to_physical_precise_round(scale, font.size()));

    let surface = ImageSurface::create(cairo::Format::ARgb32, 0, 0)?;
    let cr = cairo::Context::new(&surface)?;
    let layout = pangocairo::functions::create_layout(&cr);
    layout.context().set_round_glyph_positions(false);
    layout.set_font_description(Some(&font));
    layout.set_alignment(Alignment::Center);
    layout.set_markup(text);
    layout.set_spacing(spacing);

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
    layout.set_markup(text);
    layout.set_spacing(spacing);

    cr.set_source_rgb(1., 1., 1.);
    pangocairo::functions::show_layout(&cr, &layout);

    cr.move_to(0., 0.);
    cr.line_to(width.into(), 0.);
    cr.line_to(width.into(), height.into());
    cr.line_to(0., height.into());
    cr.line_to(0., 0.);
    cr.set_source_rgb(0.3, 0.3, 0.3);
    // Keep the border width even to avoid blurry edges.
    cr.set_line_width((f64::from(BORDER) / 2. * scale).round() * 2.);
    cr.stroke()?;
    drop(cr);

    let data = surface.take_data().unwrap();
    let buffer = TextureBuffer::from_memory(
        renderer,
        &data,
        Fourcc::Argb8888,
        (width, height),
        false,
        scale,
        Transform::Normal,
        Vec::new(),
    )?;

    Ok(buffer)
}
