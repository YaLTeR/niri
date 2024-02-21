use std::iter::zip;

use arrayvec::ArrayVec;
use niri_config;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::utils::{Logical, Point, Scale, Size};

#[derive(Debug)]
pub struct FocusRing {
    buffers: [SolidColorBuffer; 4],
    locations: [Point<i32, Logical>; 4],
    is_border: bool,
    config: niri_config::FocusRing,
}

pub type FocusRingRenderElement = SolidColorRenderElement;

impl FocusRing {
    pub fn new(config: niri_config::FocusRing) -> Self {
        Self {
            buffers: Default::default(),
            locations: Default::default(),
            is_border: false,
            config,
        }
    }

    pub fn update_config(&mut self, config: niri_config::FocusRing) {
        self.config = config;
    }

    pub fn update(&mut self, win_size: Size<i32, Logical>, is_border: bool) {
        let width = i32::from(self.config.width);

        if is_border {
            self.buffers[0].resize((win_size.w + width * 2, width));
            self.buffers[1].resize((win_size.w + width * 2, width));
            self.buffers[2].resize((width, win_size.h));
            self.buffers[3].resize((width, win_size.h));

            self.locations[0] = Point::from((-width, -width));
            self.locations[1] = Point::from((-width, win_size.h));
            self.locations[2] = Point::from((-width, 0));
            self.locations[3] = Point::from((win_size.w, 0));
        } else {
            let size = win_size + Size::from((width * 2, width * 2));
            self.buffers[0].resize(size);
            self.locations[0] = Point::from((-width, -width));
        }

        self.is_border = is_border;
    }

    pub fn set_active(&mut self, is_active: bool) {
        let color = if is_active {
            self.config.active_color.into()
        } else {
            self.config.inactive_color.into()
        };

        for buf in &mut self.buffers {
            buf.set_color(color);
        }
    }

    pub fn render(
        &self,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
    ) -> impl Iterator<Item = FocusRingRenderElement> {
        let mut rv = ArrayVec::<_, 4>::new();

        if self.config.off {
            return rv.into_iter();
        }

        let mut push = |buffer, location: Point<i32, Logical>| {
            let elem = SolidColorRenderElement::from_buffer(
                buffer,
                location.to_physical_precise_round(scale),
                scale,
                1.,
                Kind::Unspecified,
            );
            rv.push(elem.into());
        };

        if self.is_border {
            for (buf, loc) in zip(&self.buffers, self.locations) {
                push(buf, location + loc);
            }
        } else {
            push(&self.buffers[0], location + self.locations[0]);
        }

        rv.into_iter()
    }

    pub fn width(&self) -> i32 {
        self.config.width.into()
    }

    pub fn is_off(&self) -> bool {
        self.config.off
    }
}
