use smithay::backend::renderer::gles::{GlesPixelProgram, GlesRenderer, UniformName, UniformType};

use super::renderer::NiriRenderer;

pub struct Shaders {
    pub gradient_border: Option<GlesPixelProgram>,
}

impl Shaders {
    fn compile(renderer: &mut GlesRenderer) -> Self {
        let _span = tracy_client::span!("Shaders::compile");

        let gradient_border = renderer
            .compile_custom_pixel_shader(
                include_str!("gradient_border.frag"),
                &[
                    UniformName::new("color_from", UniformType::_4f),
                    UniformName::new("color_to", UniformType::_4f),
                    UniformName::new("angle", UniformType::_1f),
                    UniformName::new("gradient_offset", UniformType::_2f),
                    UniformName::new("gradient_width", UniformType::_1f),
                    UniformName::new("gradient_total", UniformType::_1f),
                ],
            )
            .map_err(|err| {
                warn!("error compiling gradient border shader: {err:?}");
            })
            .ok();

        Self { gradient_border }
    }

    pub fn get(renderer: &mut impl NiriRenderer) -> &Self {
        let renderer = renderer.as_gles_renderer();
        let data = renderer.egl_context().user_data();
        data.get()
            .expect("shaders::init() must be called when creating the renderer")
    }
}

pub fn init(renderer: &mut GlesRenderer) {
    let shaders = Shaders::compile(renderer);
    let data = renderer.egl_context().user_data();
    if !data.insert_if_missing(|| shaders) {
        error!("shaders were already compiled");
    }
}
