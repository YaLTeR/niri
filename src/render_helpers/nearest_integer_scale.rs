use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Frame, Renderer, TextureFilter};
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Transform};

#[derive(Debug)]
pub struct NearestIntegerScale<E: Element>(E);

impl<E: Element> From<E> for NearestIntegerScale<E> {
    fn from(value: E) -> Self {
        Self(value)
    }
}

impl<E: Element> Element for NearestIntegerScale<E> {
    fn id(&self) -> &Id {
        self.0.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.0.current_commit()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.0.geometry(scale)
    }

    fn transform(&self) -> Transform {
        self.0.transform()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.0.src()
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> Vec<Rectangle<i32, Physical>> {
        self.0.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> Vec<Rectangle<i32, Physical>> {
        self.0.opaque_regions(scale)
    }

    fn alpha(&self) -> f32 {
        self.0.alpha()
    }

    fn kind(&self) -> Kind {
        self.0.kind()
    }
}

impl<R: Renderer, E: RenderElement<R>> RenderElement<R> for NearestIntegerScale<E> {
    fn draw(
        &self,
        frame: &mut <R as Renderer>::Frame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), R::Error> {
        let mut use_nearest = false;

        // Check that we don't need to interpolate between src pixels.
        let src_i32 = src.to_i32_down::<i32>();
        if src_i32.to_f64() == src {
            // Check that the src is not zero.
            if !src_i32.size.is_empty() {
                // Check that the scale factor is an integer.
                let scale_x = dst.size.w / src_i32.size.w;
                let scale_y = dst.size.h / src_i32.size.h;
                if scale_x * src_i32.size.w == dst.size.w && scale_y * src_i32.size.h == dst.size.h
                {
                    use_nearest = true;
                }
            }
        }

        let mut prev_filter = TextureFilter::Linear;
        if use_nearest {
            prev_filter = frame.upscale_filter();
            frame.set_upscale_filter(TextureFilter::Nearest);
        }

        let rv = self.0.draw(frame, src, dst, damage);

        if use_nearest {
            frame.set_upscale_filter(prev_filter);
        }

        rv
    }

    fn underlying_storage(&self, renderer: &mut R) -> Option<UnderlyingStorage> {
        self.0.underlying_storage(renderer)
    }
}
