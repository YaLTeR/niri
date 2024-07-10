use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::utils::{CommitCounter, OpaqueRegions};
use smithay::backend::renderer::{Frame as _, Renderer};
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size};

/// Smithay's solid color buffer, but with fractional scale.
#[derive(Debug, Clone)]
pub struct SolidColorBuffer {
    id: Id,
    size: Size<f64, Logical>,
    commit: CommitCounter,
    color: [f32; 4],
}

/// Render element for a [`SolidColorBuffer`].
#[derive(Debug, Clone)]
pub struct SolidColorRenderElement {
    id: Id,
    geometry: Rectangle<f64, Logical>,
    commit: CommitCounter,
    color: [f32; 4],
    kind: Kind,
}

impl Default for SolidColorBuffer {
    fn default() -> Self {
        Self {
            id: Id::new(),
            size: Default::default(),
            commit: Default::default(),
            color: Default::default(),
        }
    }
}

impl SolidColorBuffer {
    pub fn new(size: impl Into<Size<f64, Logical>>, color: [f32; 4]) -> Self {
        SolidColorBuffer {
            id: Id::new(),
            color,
            commit: CommitCounter::default(),
            size: size.into(),
        }
    }

    pub fn resize(&mut self, size: impl Into<Size<f64, Logical>>) {
        let size = size.into();
        if size != self.size {
            self.size = size;
            self.commit.increment();
        }
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        if color != self.color {
            self.color = color;
            self.commit.increment();
        }
    }

    pub fn update(&mut self, size: impl Into<Size<f64, Logical>>, color: [f32; 4]) {
        let size = size.into();
        if size != self.size || color != self.color {
            self.size = size;
            self.color = color;
            self.commit.increment();
        }
    }

    pub fn color(&self) -> [f32; 4] {
        self.color
    }

    pub fn size(&self) -> Size<f64, Logical> {
        self.size
    }
}

impl SolidColorRenderElement {
    pub fn from_buffer(
        buffer: &SolidColorBuffer,
        location: impl Into<Point<f64, Logical>>,
        alpha: f32,
        kind: Kind,
    ) -> Self {
        let geo = Rectangle::from_loc_and_size(location, buffer.size());
        let color = [
            buffer.color[0] * alpha,
            buffer.color[1] * alpha,
            buffer.color[2] * alpha,
            buffer.color[3] * alpha,
        ];
        Self::new(buffer.id.clone(), geo, buffer.commit, color, kind)
    }

    pub fn new(
        id: Id,
        geometry: Rectangle<f64, Logical>,
        commit: CommitCounter,
        color: [f32; 4],
        kind: Kind,
    ) -> Self {
        SolidColorRenderElement {
            id,
            geometry,
            commit,
            color,
            kind,
        }
    }

    pub fn color(&self) -> [f32; 4] {
        self.color
    }

    pub fn geo(&self) -> Rectangle<f64, Logical> {
        self.geometry
    }
}

impl Element for SolidColorRenderElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_loc_and_size((0., 0.), (1., 1.))
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.geometry.to_physical_precise_round(scale)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        if self.color[3] == 1f32 {
            let rect = Rectangle::from_loc_and_size((0., 0.), self.geometry.size)
                .to_physical_precise_down(scale);
            OpaqueRegions::from_slice(&[rect])
        } else {
            OpaqueRegions::default()
        }
    }

    fn alpha(&self) -> f32 {
        self.color[3]
    }

    fn kind(&self) -> Kind {
        self.kind
    }
}

impl<R: Renderer> RenderElement<R> for SolidColorRenderElement {
    fn draw(
        &self,
        frame: &mut <R as Renderer>::Frame<'_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), <R as Renderer>::Error> {
        frame.draw_solid(dst, damage, self.color)
    }

    #[inline]
    fn underlying_storage(&self, _renderer: &mut R) -> Option<UnderlyingStorage<'_>> {
        None
    }
}
