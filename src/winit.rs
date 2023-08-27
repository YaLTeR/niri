use std::time::Duration;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitError, WinitEvent, WinitEventLoop, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::window::WindowBuilder;
use smithay::utils::Transform;

use crate::backend::Backend;
use crate::input::{BackendAction, CompositorMod};
use crate::niri::OutputRenderElements;
use crate::utils::get_monotonic_time;
use crate::{LoopData, Niri};

pub struct Winit {
    output: Output,
    backend: WinitGraphicsBackend<GlesRenderer>,
    winit_event_loop: WinitEventLoop,
    damage_tracker: OutputDamageTracker,
}

impl Backend for Winit {
    fn seat_name(&self) -> String {
        "winit".to_owned()
    }

    fn renderer(&mut self) -> &mut GlesRenderer {
        self.backend.renderer()
    }

    fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
    ) {
        let _span = tracy_client::span!("Winit::render");

        self.backend.bind().unwrap();
        let age = self.backend.buffer_age().unwrap();
        let res = self
            .damage_tracker
            .render_output(self.backend.renderer(), age, elements, [0.1, 0.1, 0.1, 1.0])
            .unwrap();
        if let Some(damage) = res.damage {
            self.backend.submit(Some(&damage)).unwrap();

            let mut presentation_feedbacks = niri.take_presentation_feedbacks(output, &res.states);
            let refresh = output.current_mode().unwrap().refresh as u32;
            presentation_feedbacks.presented::<_, smithay::utils::Monotonic>(
                get_monotonic_time(),
                refresh,
                0,
                wp_presentation_feedback::Kind::empty(),
            );

            self.backend.window().request_redraw();
        }
    }
}

impl Winit {
    pub fn new(event_loop: LoopHandle<LoopData>) -> Self {
        let builder = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            // .with_resizable(false)
            .with_title("niri");
        let (backend, winit_event_loop) = winit::init_from_builder(builder).unwrap();

        let mode = Mode {
            size: backend.window_size().physical_size,
            refresh: 60_000,
        };

        let output = Output::new(
            "winit".to_string(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Smithay".into(),
                model: "Winit".into(),
            },
        );
        output.change_current_state(
            Some(mode),
            Some(Transform::Flipped180),
            None,
            Some((0, 0).into()),
        );
        output.set_preferred(mode);

        let damage_tracker = OutputDamageTracker::from_output(&output);

        let timer = Timer::immediate();
        event_loop
            .insert_source(timer, move |_, _, data| {
                let winit = data.winit.as_mut().unwrap();
                winit.dispatch(&mut data.niri);
                TimeoutAction::ToDuration(Duration::from_micros(16667))
            })
            .unwrap();

        Self {
            output,
            backend,
            winit_event_loop,
            damage_tracker,
        }
    }

    pub fn init(&mut self, niri: &mut Niri) {
        // For some reason, binding the display here causes damage tracker artifacts.
        //
        // use smithay::backend::renderer::ImportEgl;
        //
        // if let Err(err) = self
        //     .backend
        //     .renderer()
        //     .bind_wl_display(&niri.display_handle)
        // {
        //     warn!("error binding renderer wl_display: {err}");
        // }
        niri.add_output(self.output.clone(), None);
    }

    fn dispatch(&mut self, niri: &mut Niri) {
        let renderer = self.backend.renderer();
        let res = self
            .winit_event_loop
            .dispatch_new_events(|event| match event {
                WinitEvent::Resized { size, .. } => {
                    self.output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                    niri.output_resized(self.output.clone());
                }
                WinitEvent::Input(event) => {
                    match niri.process_input_event(CompositorMod::Alt, event) {
                        BackendAction::None => (),
                        BackendAction::ChangeVt(_) => (),
                        BackendAction::Screenshot => {
                            let active = niri.monitor_set.active_output().cloned();
                            if let Some(active) = active {
                                if let Err(err) = niri.screenshot(renderer, &active) {
                                    warn!("error taking screenshot: {err:?}");
                                }
                            }
                        }
                    }
                }
                WinitEvent::Focus(_) => (),
                WinitEvent::Refresh => niri.queue_redraw(self.output.clone()),
            });

        // I want this to stop compiling if more errors are added.
        #[allow(clippy::single_match)]
        match res {
            Err(WinitError::WindowClosed) => {
                niri.stop_signal.stop();
                niri.remove_output(&self.output);
            }
            Ok(()) => (),
        }
    }
}
