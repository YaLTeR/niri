use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{DebugFlags, Renderer};
use smithay::backend::winit::{self, WinitError, WinitEvent, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::window::WindowBuilder;
use smithay::utils::Transform;
use smithay::wayland::dmabuf::DmabufFeedback;

use crate::niri::OutputRenderElements;
use crate::utils::get_monotonic_time;
use crate::{LoopData, Niri};

pub struct Winit {
    output: Output,
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    connectors: Arc<Mutex<HashMap<String, Output>>>,
}

impl Winit {
    pub fn new(event_loop: LoopHandle<LoopData>) -> Self {
        let builder = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            // .with_resizable(false)
            .with_title("niri");
        let (backend, mut winit_event_loop) = winit::init_from_builder(builder).unwrap();

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

        let connectors = Arc::new(Mutex::new(HashMap::from([(
            "winit".to_owned(),
            output.clone(),
        )])));

        let damage_tracker = OutputDamageTracker::from_output(&output);

        let timer = Timer::immediate();
        event_loop
            .insert_source(timer, move |_, _, data| {
                let res = winit_event_loop.dispatch_new_events(|event| match event {
                    WinitEvent::Resized { size, .. } => {
                        let winit = data.state.backend.winit();
                        winit.output.change_current_state(
                            Some(Mode {
                                size,
                                refresh: 60_000,
                            }),
                            None,
                            None,
                            None,
                        );
                        data.state.niri.output_resized(winit.output.clone());
                    }
                    WinitEvent::Input(event) => data.state.process_input_event(event),
                    WinitEvent::Focus(_) => (),
                    WinitEvent::Refresh => data
                        .state
                        .niri
                        .queue_redraw(data.state.backend.winit().output.clone()),
                });

                // I want this to stop compiling if more errors are added.
                #[allow(clippy::single_match)]
                match res {
                    Err(WinitError::WindowClosed) => {
                        data.state.niri.stop_signal.stop();
                        data.state
                            .niri
                            .remove_output(&data.state.backend.winit().output);
                    }
                    Ok(()) => (),
                }
                TimeoutAction::ToDuration(Duration::from_micros(16667))
            })
            .unwrap();

        Self {
            output,
            backend,
            damage_tracker,
            connectors,
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

    pub fn seat_name(&self) -> String {
        "winit".to_owned()
    }

    pub fn renderer(&mut self) -> &mut GlesRenderer {
        self.backend.renderer()
    }

    pub fn render(
        &mut self,
        niri: &mut Niri,
        output: &Output,
        elements: &[OutputRenderElements<GlesRenderer>],
    ) -> Option<&DmabufFeedback> {
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
            let mode = output.current_mode().unwrap();
            let refresh = Duration::from_secs_f64(1_000f64 / mode.refresh as f64);
            presentation_feedbacks.presented::<_, smithay::utils::Monotonic>(
                get_monotonic_time(),
                refresh,
                0,
                wp_presentation_feedback::Kind::empty(),
            );

            self.backend.window().request_redraw();
        }

        None
    }

    pub fn toggle_debug_tint(&mut self) {
        let renderer = self.backend.renderer();
        renderer.set_debug_flags(renderer.debug_flags() ^ DebugFlags::TINT);
    }

    pub fn connectors(&self) -> Arc<Mutex<HashMap<String, Output>>> {
        self.connectors.clone()
    }
}
