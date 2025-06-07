use smithay::utils::{Logical, Rectangle, Size};

#[derive(Debug, Clone)]
pub struct WorkspaceDimensions {
    /// Latest known output scale for this workspace.
    ///
    /// This should be set from the current workspace output, or, if all outputs have been
    /// disconnected, preserved until a new output is connected.
    scale: smithay::output::Scale,

    /// Latest known view size for this workspace.
    ///
    /// This should be computed from the current workspace output size, or, if all outputs have
    /// been disconnected, preserved until a new output is connected.
    view_size: Size<f64, Logical>,

    /// Latest known working area for this workspace.
    ///
    /// Not rounded to physical pixels.
    ///
    /// This is similar to view size, but takes into account things like layer shell exclusive
    /// zones.
    working_area: Rectangle<f64, Logical>,
}

impl WorkspaceDimensions {
    pub fn new(
        scale: smithay::output::Scale,
        view_size: Size<f64, Logical>,
        working_area: Rectangle<f64, Logical>,
    ) -> Self {
        Self {
            scale,
            view_size,
            working_area,
        }
    }

    pub fn scale(&self) -> smithay::output::Scale {
        self.scale
    }

    pub fn fractional_scale(&self) -> f64 {
        self.scale.fractional_scale()
    }

    pub fn view_size(&self) -> Size<f64, Logical> {
        self.view_size
    }

    pub fn working_area(&self) -> Rectangle<f64, Logical> {
        self.working_area
    }

    pub fn set_working_area(&mut self, working_area: Rectangle<f64, Logical>) {
        self.working_area = working_area;
    }
}
