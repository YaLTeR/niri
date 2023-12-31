use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{DebugFlags, ImportEgl, Renderer};
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::window::WindowBuilder;
use smithay::utils::Transform;

use super::RenderResult;
use crate::config::Config;
use crate::niri::{OutputRenderElements, RedrawState, State};
use crate::utils::get_monotonic_time;
use crate::Niri;

pub struct Winit {
    config: Rc<RefCell<Config>>,
    output: Output,
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    connectors: Arc<Mutex<HashMap<String, Output>>>,
}

impl Winit {
    pub fn new(config: Rc<RefCell<Config>>, event_loop: LoopHandle<State>) -> Self {
        let builder = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            // .with_resizable(false)
            .with_title("niri");
        let (backend, winit) = winit::init_from_builder(builder).unwrap();

        let output_config = config
            .borrow()
            .outputs
            .iter()
            .find(|o| o.name == "winit")
            .cloned()
            .unwrap_or_default();

        let output = Output::new(
            "winit".to_string(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Smithay".into(),
                model: "Winit".into(),
            },
        );

        let mode = Mode {
            size: backend.window_size(),
            refresh: 60_000,
        };
        let scale = output_config.scale.clamp(1., 10.).ceil() as i32;
        output.change_current_state(
            Some(mode),
            Some(Transform::Flipped180),
            Some(Scale::Integer(scale)),
            None,
        );
        output.set_preferred(mode);

        let connectors = Arc::new(Mutex::new(HashMap::from([(
            "winit".to_owned(),
            output.clone(),
        )])));

        let damage_tracker = OutputDamageTracker::from_output(&output);

        event_loop
            .insert_source(winit, move |event, _, state| match event {
                WinitEvent::Resized { size, .. } => {
                    let winit = state.backend.winit();
                    winit.output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                    state.niri.output_resized(winit.output.clone());
                }
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Focus(_) => (),
                WinitEvent::Redraw => state
                    .niri
                    .queue_redraw(state.backend.winit().output.clone()),
                WinitEvent::CloseRequested => {
                    state.niri.stop_signal.stop();
                    state.niri.remove_output(&state.backend.winit().output);
                }
            })
            .unwrap();

        Self {
            config,
            output,
            backend,
            damage_tracker,
            connectors,
        }
    }

    pub fn init(&mut self, niri: &mut Niri) {
        if let Err(err) = self
            .backend
            .renderer()
            .bind_wl_display(&niri.display_handle)
        {
            warn!("error binding renderer wl_display: {err}");
        }

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
    ) -> RenderResult {
        let _span = tracy_client::span!("Winit::render");

        self.backend.bind().unwrap();
        let age = self.backend.buffer_age().unwrap();
        let res = self
            .damage_tracker
            .render_output(self.backend.renderer(), age, elements, [0.; 4])
            .unwrap();

        niri.update_primary_scanout_output(output, &res.states);

        let rv;
        if let Some(damage) = res.damage {
            if self
                .config
                .borrow()
                .debug
                .wait_for_frame_completion_before_queueing
            {
                let _span = tracy_client::span!("wait for completion");
                res.sync.wait();
            }

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

            rv = RenderResult::Submitted;
        } else {
            rv = RenderResult::NoDamage;
        }

        let output_state = niri.output_state.get_mut(output).unwrap();
        match mem::replace(&mut output_state.redraw_state, RedrawState::Idle) {
            RedrawState::Idle => unreachable!(),
            RedrawState::Queued(_) => (),
            RedrawState::WaitingForVBlank { .. } => unreachable!(),
            RedrawState::WaitingForEstimatedVBlank(_) => unreachable!(),
            RedrawState::WaitingForEstimatedVBlankAndQueued(_) => unreachable!(),
        }

        if output_state.unfinished_animations_remain {
            self.backend.window().request_redraw();
        }

        rv
    }

    pub fn toggle_debug_tint(&mut self) {
        let renderer = self.backend.renderer();
        renderer.set_debug_flags(renderer.debug_flags() ^ DebugFlags::TINT);
    }

    pub fn connectors(&self) -> Arc<Mutex<HashMap<String, Output>>> {
        self.connectors.clone()
    }
}
