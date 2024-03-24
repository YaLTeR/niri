use std::cell::RefCell;
use std::cmp::{max, min};
use std::rc::Rc;

use niri::layout::{LayoutElement, LayoutElementRenderElement};
use niri::render_helpers::renderer::NiriRenderer;
use niri::render_helpers::RenderTarget;
use niri::window::ResolvedWindowRules;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::{Id, Kind};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Scale, Size, Transform};

#[derive(Debug)]
struct TestWindowInner {
    size: Size<i32, Logical>,
    requested_size: Option<Size<i32, Logical>>,
    min_size: Size<i32, Logical>,
    max_size: Size<i32, Logical>,
    buffer: SolidColorBuffer,
    pending_fullscreen: bool,
    csd_shadow_width: i32,
    csd_shadow_buffer: SolidColorBuffer,
}

#[derive(Debug, Clone)]
pub struct TestWindow {
    id: usize,
    inner: Rc<RefCell<TestWindowInner>>,
}

impl TestWindow {
    pub fn freeform(id: usize) -> Self {
        let size = Size::from((100, 200));
        let min_size = Size::from((0, 0));
        let max_size = Size::from((0, 0));
        let buffer = SolidColorBuffer::new(size, [0.15, 0.64, 0.41, 1.]);

        Self {
            id,
            inner: Rc::new(RefCell::new(TestWindowInner {
                size,
                requested_size: None,
                min_size,
                max_size,
                buffer,
                pending_fullscreen: false,
                csd_shadow_width: 0,
                csd_shadow_buffer: SolidColorBuffer::new((0, 0), [0., 0., 0., 0.3]),
            })),
        }
    }

    pub fn fixed_size(id: usize) -> Self {
        let rv = Self::freeform(id);
        rv.set_min_size((200, 400).into());
        rv.set_max_size((200, 400).into());
        rv.set_color([0.88, 0.11, 0.14, 1.]);
        rv.communicate();
        rv
    }

    pub fn set_min_size(&self, size: Size<i32, Logical>) {
        self.inner.borrow_mut().min_size = size;
    }

    pub fn set_max_size(&self, size: Size<i32, Logical>) {
        self.inner.borrow_mut().max_size = size;
    }

    pub fn set_color(&self, color: [f32; 4]) {
        self.inner.borrow_mut().buffer.set_color(color);
    }

    pub fn set_csd_shadow_width(&self, width: i32) {
        self.inner.borrow_mut().csd_shadow_width = width;
    }

    pub fn communicate(&self) -> bool {
        let mut rv = false;
        let mut inner = self.inner.borrow_mut();

        let mut new_size = inner.size;

        if let Some(size) = inner.requested_size.take() {
            assert!(size.w >= 0);
            assert!(size.h >= 0);

            if size.w != 0 {
                new_size.w = size.w;
            }
            if size.h != 0 {
                new_size.h = size.h;
            }
        }

        if inner.max_size.w > 0 {
            new_size.w = min(new_size.w, inner.max_size.w);
        }
        if inner.max_size.h > 0 {
            new_size.h = min(new_size.h, inner.max_size.h);
        }
        if inner.min_size.w > 0 {
            new_size.w = max(new_size.w, inner.min_size.w);
        }
        if inner.min_size.h > 0 {
            new_size.h = max(new_size.h, inner.min_size.h);
        }

        if inner.size != new_size {
            inner.size = new_size;
            inner.buffer.resize(new_size);
            rv = true;
        }

        let mut csd_shadow_size = new_size;
        csd_shadow_size.w += inner.csd_shadow_width * 2;
        csd_shadow_size.h += inner.csd_shadow_width * 2;
        inner.csd_shadow_buffer.resize(csd_shadow_size);

        rv
    }
}

impl LayoutElement for TestWindow {
    type Id = usize;

    fn id(&self) -> &Self::Id {
        &self.id
    }

    fn size(&self) -> Size<i32, Logical> {
        self.inner.borrow().size
    }

    fn buf_loc(&self) -> Point<i32, Logical> {
        (0, 0).into()
    }

    fn is_in_input_region(&self, _point: Point<f64, Logical>) -> bool {
        false
    }

    fn render<R: NiriRenderer>(
        &self,
        _renderer: &mut R,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        _target: RenderTarget,
    ) -> Vec<LayoutElementRenderElement<R>> {
        let inner = self.inner.borrow();

        vec![
            SolidColorRenderElement::from_buffer(
                &inner.buffer,
                location.to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            )
            .into(),
            SolidColorRenderElement::from_buffer(
                &inner.csd_shadow_buffer,
                (location - Point::from((inner.csd_shadow_width, inner.csd_shadow_width)))
                    .to_physical_precise_round(scale),
                scale,
                alpha,
                Kind::Unspecified,
            )
            .into(),
        ]
    }

    fn request_size(&self, size: Size<i32, Logical>) {
        self.inner.borrow_mut().requested_size = Some(size);
        self.inner.borrow_mut().pending_fullscreen = false;
    }

    fn request_fullscreen(&self, _size: Size<i32, Logical>) {
        self.inner.borrow_mut().pending_fullscreen = true;
    }

    fn min_size(&self) -> Size<i32, Logical> {
        self.inner.borrow().min_size
    }

    fn max_size(&self) -> Size<i32, Logical> {
        self.inner.borrow().max_size
    }

    fn is_wl_surface(&self, _wl_surface: &WlSurface) -> bool {
        false
    }

    fn set_preferred_scale_transform(&self, _scale: i32, _transform: Transform) {}

    fn has_ssd(&self) -> bool {
        false
    }

    fn output_enter(&self, _output: &Output) {}

    fn output_leave(&self, _output: &Output) {}

    fn set_offscreen_element_id(&self, _id: Option<Id>) {}

    fn set_activated(&mut self, _active: bool) {}

    fn set_bounds(&self, _bounds: Size<i32, Logical>) {}

    fn send_pending_configure(&self) {}

    fn is_fullscreen(&self) -> bool {
        false
    }

    fn is_pending_fullscreen(&self) -> bool {
        self.inner.borrow().pending_fullscreen
    }

    fn refresh(&self) {}

    fn rules(&self) -> &ResolvedWindowRules {
        static EMPTY: ResolvedWindowRules = ResolvedWindowRules::empty();
        &EMPTY
    }
}
