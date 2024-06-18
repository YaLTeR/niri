use smithay::backend::renderer::element::{Element, Id, RenderElement};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::Renderer;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size};

#[derive(Debug, Clone)]
pub struct ExtraDamage {
    id: Id,
    commit: CommitCounter,
    geometry: Rectangle<f64, Logical>,
}

impl ExtraDamage {
    pub fn new() -> Self {
        Self {
            id: Id::new(),
            commit: Default::default(),
            geometry: Default::default(),
        }
    }

    pub fn set_size(&mut self, size: Size<f64, Logical>) {
        if self.geometry.size == size {
            return;
        }

        self.geometry.size = size;
        self.commit.increment();
    }

    pub fn damage_all(&mut self) {
        self.commit.increment();
    }

    pub fn with_location(mut self, location: Point<f64, Logical>) -> Self {
        self.geometry.loc = location;
        self
    }
}

impl Default for ExtraDamage {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for ExtraDamage {
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
        self.geometry.to_physical_precise_up(scale)
    }
}

impl<R: Renderer> RenderElement<R> for ExtraDamage {
    fn draw(
        &self,
        _frame: &mut <R as Renderer>::Frame<'_>,
        _src: Rectangle<f64, Buffer>,
        _dst: Rectangle<i32, Physical>,
        _damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), R::Error> {
        Ok(())
    }
}
