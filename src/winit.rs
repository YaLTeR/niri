use std::time::Duration;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitError, WinitEvent, WinitEventLoop, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::LoopHandle;
use smithay::utils::{Rectangle, Transform};

use crate::backend::Backend;
use crate::niri::OutputRenderElements;
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
        _niri: &mut Niri,
        elements: &[OutputRenderElements<
            GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>,
        >],
    ) {
        let size = self.backend.window_size().physical_size;
        let damage = Rectangle::from_loc_and_size((0, 0), size);

        self.backend.bind().unwrap();
        self.damage_tracker
            .render_output(self.backend.renderer(), 0, elements, [0.1, 0.1, 0.1, 1.0])
            .unwrap();
        self.backend.submit(Some(&[damage])).unwrap();
    }
}

impl Winit {
    pub fn new(event_loop: LoopHandle<LoopData>) -> Self {
        let (backend, winit_event_loop) = winit::init().unwrap();

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
                TimeoutAction::ToDuration(Duration::from_millis(16))
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
        let _global = self.output.create_global::<Niri>(&niri.display_handle);
        niri.space.map_output(&self.output, (0, 0));
        niri.output = Some(self.output.clone());
    }

    fn dispatch(&mut self, niri: &mut Niri) {
        let res = self
            .winit_event_loop
            .dispatch_new_events(|event| match event {
                WinitEvent::Resized { size, .. } => {
                    niri.output.as_ref().unwrap().change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                }
                WinitEvent::Input(event) => niri.process_input_event(&mut |_| (), event),
                _ => (),
            });

        if let Err(WinitError::WindowClosed) = res {
            niri.stop_signal.stop();
            return;
        } else {
            res.unwrap();
        }

        niri.redraw(self);
    }
}
