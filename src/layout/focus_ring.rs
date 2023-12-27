use std::iter::zip;

use arrayvec::ArrayVec;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::utils::{Logical, Point, Scale, Size};

use crate::config::{self, Color};

#[derive(Debug)]
pub struct FocusRing {
    buffers: [SolidColorBuffer; 4],
    locations: [Point<i32, Logical>; 4],
    is_off: bool,
    is_border: bool,
    width: i32,
    active_color: Color,
    inactive_color: Color,
}

pub type FocusRingRenderElement = SolidColorRenderElement;

impl FocusRing {
    pub fn new(config: config::FocusRing) -> Self {
        Self {
            buffers: Default::default(),
            locations: Default::default(),
            is_off: config.off,
            is_border: false,
            width: config.width.into(),
            active_color: config.active_color,
            inactive_color: config.inactive_color,
        }
    }

    pub fn update_config(&mut self, config: config::FocusRing) {
        self.is_off = config.off;
        self.width = config.width.into();
        self.active_color = config.active_color;
        self.inactive_color = config.inactive_color;
    }

    pub fn update(
        &mut self,
        win_pos: Point<i32, Logical>,
        win_size: Size<i32, Logical>,
        is_border: bool,
    ) {
        if is_border {
            self.buffers[0].resize((win_size.w + self.width * 2, self.width));
            self.buffers[1].resize((win_size.w + self.width * 2, self.width));
            self.buffers[2].resize((self.width, win_size.h));
            self.buffers[3].resize((self.width, win_size.h));

            self.locations[0] = win_pos + Point::from((-self.width, -self.width));
            self.locations[1] = win_pos + Point::from((-self.width, win_size.h));
            self.locations[2] = win_pos + Point::from((-self.width, 0));
            self.locations[3] = win_pos + Point::from((win_size.w, 0));
        } else {
            let size = win_size + Size::from((self.width * 2, self.width * 2));
            self.buffers[0].resize(size);
            self.locations[0] = win_pos - Point::from((self.width, self.width));
        }

        self.is_border = is_border;
    }

    pub fn set_active(&mut self, is_active: bool) {
        let color = if is_active {
            self.active_color.into()
        } else {
            self.inactive_color.into()
        };

        for buf in &mut self.buffers {
            buf.set_color(color);
        }
    }

    pub fn render(&self, scale: Scale<f64>) -> impl Iterator<Item = FocusRingRenderElement> {
        let mut rv = ArrayVec::<_, 4>::new();

        if self.is_off {
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
            rv.push(elem);
        };

        if self.is_border {
            for (buf, loc) in zip(&self.buffers, self.locations) {
                push(buf, loc);
            }
        } else {
            push(&self.buffers[0], self.locations[0]);
        }

        rv.into_iter()
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn is_off(&self) -> bool {
        self.is_off
    }
}
