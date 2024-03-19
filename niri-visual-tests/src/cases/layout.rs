use std::collections::HashMap;
use std::time::Duration;

use niri::layout::workspace::ColumnWidth;
use niri::layout::{LayoutElement as _, Options};
use niri::utils::get_monotonic_time;
use niri_config::Color;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::layer_map_for_output;
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::utils::{Logical, Physical, Size};

use super::TestCase;
use crate::test_window::TestWindow;

type DynStepFn = Box<dyn FnOnce(&mut Layout)>;

pub struct Layout {
    output: Output,
    windows: Vec<TestWindow>,
    layout: niri::layout::Layout<TestWindow>,
    start_time: Duration,
    steps: HashMap<Duration, DynStepFn>,
}

impl Layout {
    pub fn new(size: Size<i32, Logical>) -> Self {
        let output = Output::new(
            String::new(),
            PhysicalProperties {
                size: Size::from((size.w, size.h)),
                subpixel: Subpixel::Unknown,
                make: String::new(),
                model: String::new(),
            },
        );
        let mode = Some(Mode {
            size: size.to_physical(1),
            refresh: 60000,
        });
        output.change_current_state(mode, None, None, None);

        let options = Options {
            focus_ring: niri_config::FocusRing {
                off: true,
                ..Default::default()
            },
            border: niri_config::Border {
                off: false,
                width: 4,
                active_color: Color::new(255, 163, 72, 255),
                inactive_color: Color::new(50, 50, 50, 255),
                active_gradient: None,
                inactive_gradient: None,
            },
            ..Default::default()
        };
        let mut layout = niri::layout::Layout::with_options(options);
        layout.add_output(output.clone());

        Self {
            output,
            windows: Vec::new(),
            layout,
            start_time: get_monotonic_time(),
            steps: HashMap::new(),
        }
    }

    pub fn open_in_between(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::new(size);

        rv.add_window(TestWindow::freeform(0), Some(ColumnWidth::Proportion(0.3)));
        rv.add_window(TestWindow::freeform(1), Some(ColumnWidth::Proportion(0.3)));
        rv.layout.activate_window(&0);

        rv.add_step(500, |l| {
            let win = TestWindow::freeform(2);
            l.add_window(win.clone(), Some(ColumnWidth::Proportion(0.3)));
            l.layout.start_open_animation_for_window(win.id());
        });

        rv
    }

    pub fn open_multiple_quickly(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::new(size);

        for delay in [100, 200, 300] {
            rv.add_step(delay, move |l| {
                let win = TestWindow::freeform(delay as usize);
                l.add_window(win.clone(), Some(ColumnWidth::Proportion(0.3)));
                l.layout.start_open_animation_for_window(win.id());
            });
        }

        rv
    }

    pub fn open_multiple_quickly_big(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::new(size);

        for delay in [100, 200, 300] {
            rv.add_step(delay, move |l| {
                let win = TestWindow::freeform(delay as usize);
                l.add_window(win.clone(), Some(ColumnWidth::Proportion(0.5)));
                l.layout.start_open_animation_for_window(win.id());
            });
        }

        rv
    }

    pub fn open_to_the_left(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::new(size);

        rv.add_window(TestWindow::freeform(0), Some(ColumnWidth::Proportion(0.3)));
        rv.add_window(TestWindow::freeform(1), Some(ColumnWidth::Proportion(0.3)));

        rv.add_step(500, |l| {
            let win = TestWindow::freeform(2);
            let right_of = l.windows[0].clone();
            l.add_window_right_of(&right_of, win.clone(), Some(ColumnWidth::Proportion(0.3)));
            l.layout.start_open_animation_for_window(win.id());
        });

        rv
    }

    pub fn open_to_the_left_big(size: Size<i32, Logical>) -> Self {
        let mut rv = Self::new(size);

        rv.add_window(TestWindow::freeform(0), Some(ColumnWidth::Proportion(0.3)));
        rv.add_window(TestWindow::freeform(1), Some(ColumnWidth::Proportion(0.8)));

        rv.add_step(500, |l| {
            let win = TestWindow::freeform(2);
            let right_of = l.windows[0].clone();
            l.add_window_right_of(&right_of, win.clone(), Some(ColumnWidth::Proportion(0.5)));
            l.layout.start_open_animation_for_window(win.id());
        });

        rv
    }

    fn add_window(&mut self, window: TestWindow, width: Option<ColumnWidth>) {
        self.layout.add_window(window.clone(), width, false);
        if window.communicate() {
            self.layout.update_window(window.id());
        }
        self.windows.push(window);
    }

    fn add_window_right_of(
        &mut self,
        right_of: &TestWindow,
        window: TestWindow,
        width: Option<ColumnWidth>,
    ) {
        self.layout
            .add_window_right_of(right_of.id(), window.clone(), width, false);
        if window.communicate() {
            self.layout.update_window(window.id());
        }
        self.windows.push(window);
    }

    fn add_step(&mut self, delay_ms: u64, f: impl FnOnce(&mut Self) + 'static) {
        self.steps
            .insert(Duration::from_millis(delay_ms), Box::new(f) as _);
    }
}

impl TestCase for Layout {
    fn resize(&mut self, width: i32, height: i32) {
        let mode = Some(Mode {
            size: Size::from((width, height)),
            refresh: 60000,
        });
        self.output.change_current_state(mode, None, None, None);
        layer_map_for_output(&self.output).arrange();
        self.layout.update_output_size(&self.output);
        for win in &self.windows {
            if win.communicate() {
                self.layout.update_window(win.id());
            }
        }
    }

    fn are_animations_ongoing(&self) -> bool {
        self.layout
            .monitor_for_output(&self.output)
            .unwrap()
            .are_animations_ongoing()
            || !self.steps.is_empty()
    }

    fn advance_animations(&mut self, mut current_time: Duration) {
        let run = self
            .steps
            .keys()
            .copied()
            .filter(|delay| self.start_time + *delay <= current_time)
            .collect::<Vec<_>>();
        for key in &run {
            let f = self.steps.remove(key).unwrap();
            f(self);
        }
        if !run.is_empty() {
            current_time = get_monotonic_time();
        }

        self.layout.advance_animations(current_time);
    }

    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        _size: Size<i32, Physical>,
    ) -> Vec<Box<dyn RenderElement<GlesRenderer>>> {
        self.layout
            .monitor_for_output(&self.output)
            .unwrap()
            .render_elements(renderer)
            .into_iter()
            .map(|elem| Box::new(elem) as _)
            .collect()
    }
}
