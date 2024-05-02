use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::Scale;

use super::renderer::NiriRenderer;
use crate::niri::OutputRenderElements;

pub fn draw_opaque_regions<R: NiriRenderer>(
    elements: &mut Vec<OutputRenderElements<R>>,
    scale: Scale<f64>,
) {
    let _span = tracy_client::span!("draw_opaque_regions");

    let mut i = 0;
    while i < elements.len() {
        let elem = &elements[i];
        i += 1;

        // HACK
        if format!("{elem:?}").contains("ExtraDamage") {
            continue;
        }

        let geo = elem.geometry(scale);
        let mut opaque = elem.opaque_regions(scale);

        for rect in &mut opaque {
            rect.loc += geo.loc;
        }

        let semitransparent = geo.subtract_rects(opaque.iter().copied());

        for rect in opaque {
            let color = SolidColorRenderElement::new(
                Id::new(),
                rect,
                CommitCounter::default(),
                [0., 0., 0.2, 0.2],
                Kind::Unspecified,
            );
            elements.insert(i - 1, OutputRenderElements::SolidColor(color));
            i += 1;
        }

        for rect in semitransparent {
            let color = SolidColorRenderElement::new(
                Id::new(),
                rect,
                CommitCounter::default(),
                [0.3, 0., 0., 0.3],
                Kind::Unspecified,
            );
            elements.insert(i - 1, OutputRenderElements::SolidColor(color));
            i += 1;
        }
    }
}
