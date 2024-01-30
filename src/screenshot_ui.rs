use std::cmp::{max, min};
use std::collections::HashMap;
use std::iter::zip;

use anyhow::Context;
use arrayvec::ArrayVec;
use niri_config::Action;
use smithay::backend::allocator::Fourcc;
use smithay::backend::input::{ButtonState, MouseButton};
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::ExportMem;
use smithay::input::keyboard::{Keysym, ModifiersState};
use smithay::output::{Output, WeakOutput};
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Size, Transform};

use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};
use crate::render_helpers::PrimaryGpuTextureRenderElement;

const BORDER: i32 = 2;

// Ideally the screenshot UI should support cross-output selections. However, that poses some
// technical challenges when the outputs have different scales and such. So, this implementation
// allows only single-output selections for now.
//
// As a consequence of this, selection coordinates are in output-local coordinate space.
pub struct ScreenshotUi {
    state: ScreenshotUiState,
    highlight_region: Option<Rectangle<i32, Physical>>,
    output_data: HashMap<Output, OutputData>,
}

pub enum ScreenshotUiState {
    Open {
        selection: (Output, Point<i32, Physical>, Point<i32, Physical>),
        mouse_down: bool,
    },
    Closed {
        last_selection: Option<(WeakOutput, Rectangle<i32, Physical>)>,
    },
}

pub struct OutputData {
    size: Size<i32, Physical>,
    scale: i32,
    texture: GlesTexture,
    texture_buffer: TextureBuffer<GlesTexture>,
    buffers: [SolidColorBuffer; 9],
    locations: [Point<i32, Physical>; 9],
}

#[derive(Debug)]
pub enum ScreenshotUiRenderElement {
    Screenshot(PrimaryGpuTextureRenderElement),
    SolidColor(SolidColorRenderElement),
}

impl ScreenshotUi {
    pub fn new() -> Self {
        Self {
            state: ScreenshotUiState::Closed {
                last_selection: None,
            },
            highlight_region: None,
            output_data: HashMap::new(),
        }
    }

    pub fn open(
        &mut self,
        renderer: &GlesRenderer,
        screenshots: HashMap<Output, GlesTexture>,
        default_output: Output,
    ) -> bool {
        if screenshots.is_empty() {
            return false;
        }

        self.output_data = screenshots
            .clone()
            .into_iter()
            .map(|(output, texture)| {
                let output_transform = output.current_transform();
                let output_mode = output.current_mode().unwrap();
                let size = output_transform.transform_size(output_mode.size);
                let scale = output.current_scale().integer_scale();
                let texture_buffer = TextureBuffer::from_texture(
                    renderer,
                    texture.clone(),
                    scale,
                    Transform::Normal,
                    None,
                );
                let buffers = [
                    SolidColorBuffer::new((0, 0), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0, 0), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0, 0), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0, 0), [1., 1., 1., 1.]),
                    SolidColorBuffer::new((0, 0), [0., 0., 0., 0.5]),
                    SolidColorBuffer::new((0, 0), [0., 0., 0., 0.5]),
                    SolidColorBuffer::new((0, 0), [0., 0., 0., 0.5]),
                    SolidColorBuffer::new((0, 0), [0., 0., 0., 0.5]),
                    SolidColorBuffer::new((0, 0), [0.5, 0.5, 1., 0.5]),
                ];
                let locations = [Default::default(); 9];
                let data = OutputData {
                    size,
                    scale,
                    texture,
                    texture_buffer,
                    buffers,
                    locations,
                };
                (output, data)
            })
            .collect();

        let ScreenshotUiState::Closed { last_selection } = &mut self.state else {
            self.update_buffers();

            return false;
        };

        let last_selection = last_selection
            .as_ref()
            .and_then(|(weak, sel)| weak.upgrade().map(|output| (output, sel)));
        let selection = match last_selection {
            Some((output, rectangle)) if screenshots.contains_key(&output) => (output, *rectangle),
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

        self.state = ScreenshotUiState::Open {
            selection,
            mouse_down: false,
        };

        self.update_buffers();

        true
    }

    pub fn close(&mut self) -> bool {
        match &mut self.state {
            ScreenshotUiState::Open { selection, .. } => {
                let scale = selection.0.current_scale().integer_scale();
                let last_selection = Some((
                    selection.0.downgrade(),
                    rect_from_corner_points(selection.1, selection.2, scale),
                ));

                self.state = ScreenshotUiState::Closed { last_selection };

                true
            }
            ScreenshotUiState::Closed { .. } => false,
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.state, ScreenshotUiState::Open { .. })
    }

    fn update_buffers(&mut self) {
        if let ScreenshotUiState::Open { selection, .. } = &mut self.state {
            let (selection_output, a, b) = selection;
            let scale = selection_output.current_scale().integer_scale();
            let mut rect = rect_from_corner_points(*a, *b, scale);

            for (output, data) in &mut self.output_data {
                let buffers = &mut data.buffers;
                let locations = &mut data.locations;
                let size = data.size;

                if output == selection_output {
                    let scale = output.current_scale().integer_scale();

                    // Check if the selection is still valid. If not, reset it back to default.
                    if !Rectangle::from_loc_and_size((0, 0), size).contains_rect(rect) {
                        rect = Rectangle::from_loc_and_size(
                            (size.w / 4, size.h / 4),
                            (size.w / 2, size.h / 2),
                        );
                        *a = rect.loc;
                        *b = rect.loc + rect.size - Size::from((scale, scale));
                    }

                    let border = BORDER * scale;

                    buffers[0].resize((rect.size.w + border * 2, border));
                    buffers[1].resize((rect.size.w + border * 2, border));
                    buffers[2].resize((border, rect.size.h));
                    buffers[3].resize((border, rect.size.h));

                    buffers[4].resize((size.w, rect.loc.y));
                    buffers[5].resize((size.w, size.h - rect.loc.y - rect.size.h));
                    buffers[6].resize((rect.loc.x, rect.size.h));
                    buffers[7].resize((size.w - rect.loc.x - rect.size.w, rect.size.h));

                    buffers[8].resize((0, 0));

                    locations[0] = Point::from((rect.loc.x - border, rect.loc.y - border));
                    locations[1] = Point::from((rect.loc.x - border, rect.loc.y + rect.size.h));
                    locations[2] = Point::from((rect.loc.x - border, rect.loc.y));
                    locations[3] = Point::from((rect.loc.x + rect.size.w, rect.loc.y));

                    locations[5] = Point::from((0, rect.loc.y + rect.size.h));
                    locations[6] = Point::from((0, rect.loc.y));
                    locations[7] = Point::from((rect.loc.x + rect.size.w, rect.loc.y));
                } else {
                    buffers[0].resize((0, 0));
                    buffers[1].resize((0, 0));
                    buffers[2].resize((0, 0));
                    buffers[3].resize((0, 0));

                    buffers[4].resize(size.to_logical(1));
                    buffers[5].resize((0, 0));
                    buffers[6].resize((0, 0));
                    buffers[7].resize((0, 0));

                    buffers[8].resize((0, 0));
                }
            }
        }

        // Draw the highlight region if button is not pressed and selected
        // region doesn't equal the highlighted region.
        if let Some(hrect) = self.highlight_region {
            if !matches!(
                &self.state,
                ScreenshotUiState::Open {
                    mouse_down,
                    selection
                } if selection.1 == hrect.loc && selection.2 == hrect.loc + hrect.size || *mouse_down)
            {
                self.output_data.iter_mut().for_each(|(_, data)| {
                    let buffers = &mut data.buffers;
                    let locations = &mut data.locations;
                    let size = data.size;

                    if let Some(hrect) =
                        hrect.intersection(Rectangle::from_loc_and_size((0, 0), size))
                    {
                        buffers[8].resize((hrect.size.w, hrect.size.h));
                        locations[8] = hrect.loc;
                    } else {
                        buffers[8].resize((0, 0));
                    }
                });
            }
        }
    }

    pub fn render_output(&self, output: &Output) -> ArrayVec<ScreenshotUiRenderElement, 10> {
        let _span = tracy_client::span!("ScreenshotUi::render_output");

        let mut elements = ArrayVec::new();

        let Some(output_data) = self.output_data.get(output) else {
            return elements;
        };

        let buf_loc = zip(&output_data.buffers, &output_data.locations);
        elements.extend(buf_loc.map(|(buffer, loc)| {
            SolidColorRenderElement::from_buffer(
                buffer,
                *loc,
                1., // We treat these as physical coordinates.
                1.,
                Kind::Unspecified,
            )
            .into()
        }));

        // The screenshot itself goes last.
        elements.push(
            PrimaryGpuTextureRenderElement(TextureRenderElement::from_texture_buffer(
                (0., 0.),
                &output_data.texture_buffer,
                None,
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

        let ScreenshotUiState::Open { selection, .. } = &self.state else {
            panic!("screenshot UI must be open to capture");
        };

        let data = &self.output_data[&selection.0];
        let scale = selection.0.current_scale().integer_scale();
        let rect = rect_from_corner_points(selection.1, selection.2, scale);
        let buf_rect = rect
            .to_logical(1)
            .to_buffer(1, Transform::Normal, &data.size.to_logical(1));

        let mapping = renderer
            .copy_texture(&data.texture, buf_rect, Fourcc::Abgr8888)
            .context("error copying texture")?;
        let copy = renderer
            .map_texture(&mapping)
            .context("error mapping texture")?;

        Ok((rect.size, copy.to_vec()))
    }

    pub fn action(&self, raw: Option<Keysym>, mods: ModifiersState) -> Option<Action> {
        if !matches!(self.state, ScreenshotUiState::Open { .. }) {
            return None;
        }

        action(raw?, mods)
    }

    pub fn selection_output(&self) -> Option<&Output> {
        if let ScreenshotUiState::Open {
            selection: (output, _, _),
            ..
        } = &self.state
        {
            Some(output)
        } else {
            None
        }
    }

    pub fn output_size(&self, output: &Output) -> Option<(Size<i32, Physical>, i32)> {
        let data = self.output_data.get(output)?;
        Some((data.size, data.scale))
    }

    /// The pointer has moved to `point` relative to the current selection output.
    pub fn pointer_motion(
        &mut self,
        point: Point<i32, Physical>,
        highlight_region_target: Option<Rectangle<i32, Physical>>,
    ) {
        if let ScreenshotUiState::Open {
            selection,
            mouse_down: true,
            ..
        } = &mut self.state
        {
            selection.2 = point;
        }

        self.highlight_region = highlight_region_target;

        self.update_buffers();
    }

    pub fn pointer_button(
        &mut self,
        output: Output,
        point: Point<i32, Physical>,
        button: MouseButton,
        state: ButtonState,
    ) -> bool {
        let ScreenshotUiState::Open {
            selection,
            mouse_down,
        } = &mut self.state
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

        if down && !self.output_data.contains_key(&output) {
            return false;
        }

        *mouse_down = down;

        if down {
            *selection = (output, point, point);
        } else {
            // Check if the resulting selection is zero-sized, choose the selected region and if
            // that doesn't exist try to come up with a small default rectangle.
            let (output, a, b) = selection;
            let scale = output.current_scale().integer_scale();
            let mut rect = rect_from_corner_points(*a, *b, scale);
            if rect.size.is_empty() || rect.size == Size::from((scale, scale)) {
                if let Some(highlight_region) = self.highlight_region {
                    selection.1 = highlight_region.loc;
                    selection.2 = highlight_region.loc + highlight_region.size;
                } else {
                    let data = &self.output_data[output];
                    rect =
                        Rectangle::from_loc_and_size((rect.loc.x - 16, rect.loc.y - 16), (32, 32))
                            .intersection(Rectangle::from_loc_and_size((0, 0), data.size))
                            .unwrap_or_default();
                    let scale = output.current_scale().integer_scale();
                    *a = rect.loc;
                    *b = rect.loc + rect.size - Size::from((scale, scale));
                }
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
    scale: i32,
) -> Rectangle<i32, Physical> {
    let x1 = min(a.x, b.x);
    let y1 = min(a.y, b.y);
    let x2 = max(a.x, b.x);
    let y2 = max(a.y, b.y);
    Rectangle::from_extemities((x1, y1), (x2 + scale, y2 + scale))
}

// Manual RenderElement implementation due to AsGlesFrame requirement.
impl Element for ScreenshotUiRenderElement {
    fn id(&self) -> &Id {
        match self {
            Self::Screenshot(elem) => elem.id(),
            Self::SolidColor(elem) => elem.id(),
        }
    }

    fn current_commit(&self) -> CommitCounter {
        match self {
            Self::Screenshot(elem) => elem.current_commit(),
            Self::SolidColor(elem) => elem.current_commit(),
        }
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        match self {
            Self::Screenshot(elem) => elem.geometry(scale),
            Self::SolidColor(elem) => elem.geometry(scale),
        }
    }

    fn transform(&self) -> Transform {
        match self {
            Self::Screenshot(elem) => elem.transform(),
            Self::SolidColor(elem) => elem.transform(),
        }
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        match self {
            Self::Screenshot(elem) => elem.src(),
            Self::SolidColor(elem) => elem.src(),
        }
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> Vec<Rectangle<i32, Physical>> {
        match self {
            Self::Screenshot(elem) => elem.damage_since(scale, commit),
            Self::SolidColor(elem) => elem.damage_since(scale, commit),
        }
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> Vec<Rectangle<i32, Physical>> {
        match self {
            Self::Screenshot(elem) => elem.opaque_regions(scale),
            Self::SolidColor(elem) => elem.opaque_regions(scale),
        }
    }

    fn alpha(&self) -> f32 {
        match self {
            Self::Screenshot(elem) => elem.alpha(),
            Self::SolidColor(elem) => elem.alpha(),
        }
    }

    fn kind(&self) -> Kind {
        match self {
            Self::Screenshot(elem) => elem.kind(),
            Self::SolidColor(elem) => elem.kind(),
        }
    }
}

impl RenderElement<GlesRenderer> for ScreenshotUiRenderElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        match self {
            Self::Screenshot(elem) => {
                RenderElement::<GlesRenderer>::draw(&elem, frame, src, dst, damage)
            }
            Self::SolidColor(elem) => {
                RenderElement::<GlesRenderer>::draw(&elem, frame, src, dst, damage)
            }
        }
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}

impl<'render, 'alloc> RenderElement<TtyRenderer<'render, 'alloc>> for ScreenshotUiRenderElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'render, 'alloc, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render, 'alloc>> {
        match self {
            Self::Screenshot(elem) => {
                RenderElement::<TtyRenderer<'render, 'alloc>>::draw(&elem, frame, src, dst, damage)
            }
            Self::SolidColor(elem) => {
                RenderElement::<TtyRenderer<'render, 'alloc>>::draw(&elem, frame, src, dst, damage)
            }
        }
    }

    fn underlying_storage(
        &self,
        _renderer: &mut TtyRenderer<'render, 'alloc>,
    ) -> Option<UnderlyingStorage> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}

impl From<SolidColorRenderElement> for ScreenshotUiRenderElement {
    fn from(x: SolidColorRenderElement) -> Self {
        Self::SolidColor(x)
    }
}

impl From<PrimaryGpuTextureRenderElement> for ScreenshotUiRenderElement {
    fn from(x: PrimaryGpuTextureRenderElement) -> Self {
        Self::Screenshot(x)
    }
}
