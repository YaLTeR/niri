use std::cell::RefCell;

use glam::Mat3;
use smithay::backend::renderer::gles::{
    GlesError, GlesPixelProgram, GlesRenderer, Uniform, UniformName, UniformType, UniformValue,
};

use super::primary_gpu_pixel_shader_with_textures::PixelWithTexturesProgram;
use super::renderer::NiriRenderer;

pub struct Shaders {
    pub gradient_border: Option<GlesPixelProgram>,
    pub resize: Option<PixelWithTexturesProgram>,
    pub custom_resize: RefCell<Option<PixelWithTexturesProgram>>,
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

        let resize = compile_resize_program(renderer, include_str!("resize.frag"))
            .map_err(|err| {
                warn!("error compiling resize shader: {err:?}");
            })
            .ok();

        Self {
            gradient_border,
            resize,
            custom_resize: RefCell::new(None),
        }
    }

    pub fn get(renderer: &mut impl NiriRenderer) -> &Self {
        let renderer = renderer.as_gles_renderer();
        let data = renderer.egl_context().user_data();
        data.get()
            .expect("shaders::init() must be called when creating the renderer")
    }

    pub fn replace_custom_resize_program(
        &self,
        program: Option<PixelWithTexturesProgram>,
    ) -> Option<PixelWithTexturesProgram> {
        self.custom_resize.replace(program)
    }

    pub fn resize(&self) -> Option<PixelWithTexturesProgram> {
        self.custom_resize
            .borrow()
            .clone()
            .or_else(|| self.resize.clone())
    }
}

pub fn init(renderer: &mut GlesRenderer) {
    let shaders = Shaders::compile(renderer);
    let data = renderer.egl_context().user_data();
    if !data.insert_if_missing(|| shaders) {
        error!("shaders were already compiled");
    }
}

fn compile_resize_program(
    renderer: &mut GlesRenderer,
    src: &str,
) -> Result<PixelWithTexturesProgram, GlesError> {
    let mut program = include_str!("resize-prelude.frag").to_string();
    program.push_str(src);
    program.push_str(include_str!("resize-epilogue.frag"));

    PixelWithTexturesProgram::compile(
        renderer,
        &program,
        &[
            UniformName::new("niri_input_to_curr_geo", UniformType::Matrix3x3),
            UniformName::new("niri_curr_geo_to_prev_geo", UniformType::Matrix3x3),
            UniformName::new("niri_curr_geo_to_next_geo", UniformType::Matrix3x3),
            UniformName::new("niri_curr_geo_size", UniformType::_2f),
            UniformName::new("niri_geo_to_tex_prev", UniformType::Matrix3x3),
            UniformName::new("niri_geo_to_tex_next", UniformType::Matrix3x3),
            UniformName::new("niri_progress", UniformType::_1f),
            UniformName::new("niri_clamped_progress", UniformType::_1f),
        ],
        &["niri_tex_prev", "niri_tex_next"],
    )
}

pub fn set_custom_resize_program(renderer: &mut GlesRenderer, src: Option<&str>) {
    let program = if let Some(src) = src {
        match compile_resize_program(renderer, src) {
            Ok(program) => Some(program),
            Err(err) => {
                warn!("error compiling custom resize shader: {err:?}");
                return;
            }
        }
    } else {
        None
    };

    if let Some(prev) = Shaders::get(renderer).replace_custom_resize_program(program) {
        if let Err(err) = prev.destroy(renderer) {
            warn!("error destroying previous custom resize shader: {err:?}");
        }
    }
}

pub fn mat3_uniform(name: &str, mat: Mat3) -> Uniform {
    Uniform::new(
        name,
        UniformValue::Matrix3x3 {
            matrices: vec![mat.to_cols_array()],
            transpose: false,
        },
    )
}
