use std::cmp::{max, min};
use std::collections::HashMap;
use std::iter::zip;
use std::mem;

use anyhow::Context;
use arrayvec::ArrayVec;
use niri_config::Action;
use smithay::backend::allocator::Fourcc;
use smithay::backend::input::{ButtonState, MouseButton};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::ExportMem;
use smithay::input::keyboard::{Keysym, ModifiersState};
use smithay::output::{Output, WeakOutput};
use smithay::utils::{Physical, Point, Rectangle, Size, Transform};

use crate::niri_render_elements;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::render_helpers::RenderTarget;
use crate::utils::to_physical_precise_round;

const BORDER: i32 = 2;

// Ideally the screenshot UI should support cross-output selections. However, that poses some
// technical challenges when the outputs have different scales and such. So, this implementation
// allows only single-output selections for now.
//
// As a consequence of this, selection coordinates are in output-local coordinate space.
pub enum ScreenshotUi {
    Closed {
        last_selection: Option<(WeakOutput, Rectangle<i32, Physical>)>,
    },
    Open {
        selection: (Output, Point<i32, Physical>, Point<i32, Physical>),
        output_data: HashMap<Output, OutputData>,
        mouse_down: bool,
    },
}

pub struct OutputData {
    size: Size<i32, Physical>,
    scale: f64,
    transform: Transform,
    // Output, screencast, screen capture.
    texture: [GlesTexture; 3],
    texture_buffer: [TextureBuffer<GlesTexture>; 3],
    buffers: [SolidColorBuffer; 8],
    locations: [Point<i32, Physical>; 8],
}

niri_render_elements! {
    ScreenshotUiRenderElement => {
        Screenshot = PrimaryGpuTextureRenderElement,
        SolidColor = SolidColorRenderElement,
    }
}

impl ScreenshotUi {
    pub fn new() -> Self {
        Self::Closed {
            last_selection: None,
        }
    }

    pub fn open(
        &mut self,
        renderer: &GlesRenderer,
        // Output, screencast, screen capture.
        screenshots: HashMap<Output, [GlesTexture; 3]>,
        default_output: Output,
    ) -> bool {
        if screenshots.is_empty() {
            return false;
        }

        let Self::Closed { last_selection } = self else {
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
                    Rectangle::from_loc_and_size(
                        (size.w / 4, size.h / 4),
                        (size.w / 2, size.h / 2),
                    ),
                )
            }
        };

        let scale = selection.0.current_scale().integer_scale();
        let selection = (
            selection.0,
            selection.1.loc,
            selection.1.loc + selection.1.size - Size::from((scale, scale)),
        );

        let output_data = screenshots
            .into_iter()
            .map(|(output, texture)| {
                let transform = output.current_transform();
                let output_mode = output.current_mode().unwrap();
                let size = transform.transform_size(output_mode.size);
                let scale = output.current_scale().fractional_scale();
                let texture_buffer = texture.clone().map(|texture| {
                    TextureBuffer::from_texture(
                        renderer,
                        texture,
                        scale,
                        Transform::Normal,
                        Vec::new(),
                    )
                });
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
                let data = OutputData {
                    size,
                    scale,
                    transform,
                    texture,
                    texture_buffer,
                    buffers,
                    locations,
                };
                (output, data)
            })
            .collect();

        *self = Self::Open {
            selection,
            output_data,
            mouse_down: false,
        };

        self.update_buffers();

        true
    }

    pub fn close(&mut self) -> bool {
        let selection = match mem::take(self) {
            Self::Open { selection, .. } => selection,
            closed @ Self::Closed { .. } => {
                // Put it back.
                *self = closed;
                return false;
            }
        };

        let last_selection = Some((
            selection.0.downgrade(),
            rect_from_corner_points(selection.1, selection.2),
        ));

        *self = Self::Closed { last_selection };

        true
    }

    pub fn is_open(&self) -> bool {
        matches!(self, ScreenshotUi::Open { .. })
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
                if !Rectangle::from_loc_and_size((0, 0), size).contains_rect(rect) {
                    rect = Rectangle::from_loc_and_size(
                        (size.w / 4, size.h / 4),
                        (size.w / 2, size.h / 2),
                    );
                    *a = rect.loc;
                    *b = rect.loc + rect.size - Size::from((1, 1));
                }

                let border = to_physical_precise_round(scale, BORDER);

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
    ) -> ArrayVec<ScreenshotUiRenderElement, 9> {
        let _span = tracy_client::span!("ScreenshotUi::render_output");

        let Self::Open { output_data, .. } = self else {
            panic!("screenshot UI must be open to render it");
        };

        let mut elements = ArrayVec::new();

        let Some(output_data) = output_data.get(output) else {
            return elements;
        };

        let buf_loc = zip(&output_data.buffers, &output_data.locations);
        elements.extend(buf_loc.map(|(buffer, loc)| {
            SolidColorRenderElement::from_buffer(
                buffer,
                loc.to_f64().to_logical(output_data.scale),
                1.,
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
        elements.push(
            PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
                output_data.texture_buffer[index].clone(),
                (0., 0.),
                1.,
                None,
                None,
                Kind::Unspecified,
            ))
            .into(),
        );

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
            ..
        } = self
        else {
            panic!("screenshot UI must be open to capture");
        };

        let data = &output_data[&selection.0];
        let rect = rect_from_corner_points(selection.1, selection.2);
        let buf_rect = rect
            .to_logical(1)
            .to_buffer(1, Transform::Normal, &data.size.to_logical(1));

        let mapping = renderer
            .copy_texture(&data.texture[0], buf_rect, Fourcc::Abgr8888)
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
                rect = Rectangle::from_loc_and_size((rect.loc.x - 16, rect.loc.y - 16), (32, 32))
                    .intersection(Rectangle::from_loc_and_size((0, 0), data.size))
                    .unwrap_or_default();
                *a = rect.loc;
                *b = rect.loc + rect.size - Size::from((1, 1));
            }
        }

        self.update_buffers();

        true
    }
}

impl Default for ScreenshotUi {
    fn default() -> Self {
        Self::new()
    }
}

fn action(raw: Keysym, mods: ModifiersState) -> Option<Action> {
    if raw == Keysym::Escape {
        return Some(Action::CancelScreenshot);
    }

    if mods.alt || mods.shift {
        return None;
    }

    if (mods.ctrl && raw == Keysym::c)
        || (!mods.ctrl && (raw == Keysym::space || raw == Keysym::Return))
    {
        return Some(Action::ConfirmScreenshot);
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
    Rectangle::from_extemities((x1, y1), (x2 + 1, y2 + 1))
}
