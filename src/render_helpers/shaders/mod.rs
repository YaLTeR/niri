use smithay::backend::renderer::gles::{GlesPixelProgram, GlesRenderer, UniformName, UniformType};

use super::primary_gpu_pixel_shader_with_textures::PixelWithTexturesProgram;
use super::renderer::NiriRenderer;

pub struct Shaders {
    pub gradient_border: Option<GlesPixelProgram>,
    pub crossfade: Option<PixelWithTexturesProgram>,
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
                    UniformName::new("grad_offset", UniformType::_2f),
                    UniformName::new("grad_width", UniformType::_1f),
                    UniformName::new("grad_vec", UniformType::_2f),
                ],
            )
            .map_err(|err| {
                warn!("error compiling gradient border shader: {err:?}");
            })
            .ok();

        let crossfade = PixelWithTexturesProgram::compile(
            renderer,
            include_str!("crossfade.frag"),
            &[
                UniformName::new("tex_from_loc", UniformType::_2f),
                UniformName::new("tex_from_size", UniformType::_2f),
                UniformName::new("tex_to_loc", UniformType::_2f),
                UniformName::new("tex_to_size", UniformType::_2f),
                UniformName::new("amount", UniformType::_1f),
            ],
            &["tex_from", "tex_to"],
        )
        .map_err(|err| {
            warn!("error compiling crossfade shader: {err:?}");
        })
        .ok();

        Self {
            gradient_border,
            crossfade,
        }
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
