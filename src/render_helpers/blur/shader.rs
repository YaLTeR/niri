// Ported from https://github.com/nferhat/fht-compositor/blob/main/src/renderer/blur/shaders.rs
// The notes below are from the original code.

//! Since I need more control over how stuff is done when blurring, I use my own shader and
//! ffi::Gles2 directly. Gives me the most control but needs some re-writing of existing smithay
//! code.
//!
//! NOTE: Since we are assured that this shader lives as long as the compositor does, there's no
//! need to add a destruction callback sender like smithay does.

use std::fmt::Write;
use std::sync::Arc;

use smithay::backend::renderer::gles::{ffi, link_program, GlesError, GlesRenderer};

const BLUR_DOWN_SRC: &str = include_str!("../shaders/blur_down.frag");
const BLUR_UP_SRC: &str = include_str!("../shaders/blur_up.frag");
const VERTEX_SRC: &str = include_str!("../shaders/texture.vert");

/// The set of blur shaders used to render the blur.
#[derive(Clone, Debug)]
pub struct BlurShaders {
    pub down: BlurShader,
    pub up: BlurShader,
}

impl BlurShaders {
    pub fn compile(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        renderer.with_context(|gl| unsafe {
            let down = BlurShader::compile(gl, BLUR_DOWN_SRC)?;
            let up = BlurShader::compile(gl, BLUR_UP_SRC)?;
            Result::<Self, GlesError>::Ok(Self { down, up })
        })?
    }
}

#[derive(Clone, Debug)]
pub struct BlurShader(Arc<[BlurShaderVariant; 2]>);
impl BlurShader {
    pub(super) fn variant_for_format(
        &self,
        format: Option<ffi::types::GLenum>,
        has_alpha: bool,
    ) -> &BlurShaderVariant {
        match format {
            Some(ffi::BGRA_EXT) | Some(ffi::RGBA) | Some(ffi::RGBA8) | Some(ffi::RGB10_A2)
            | Some(ffi::RGBA16F) => {
                if has_alpha {
                    &self.0[0]
                } else {
                    &self.0[1]
                }
            }
            // SAFETY: Since we create the blur textures ourselves (through
            // Offscreen::create_buffer) they should always be local to the renderer
            None => panic!("Blur textures should not be external!"),
            _ => panic!("Unknown texture type"),
        }
    }

    pub(super) unsafe fn compile(gl: &ffi::Gles2, src: &str) -> Result<Self, GlesError> {
        let create_variant = |defines: &[&str]| -> Result<BlurShaderVariant, GlesError> {
            let shader = src.replace(
                "//_DEFINES_",
                &defines.iter().fold(String::new(), |mut shader, define| {
                    let _ = writeln!(&mut shader, "#define {}", define);
                    shader
                }),
            );
            let debug_shader = src.replace(
                "//_DEFINES_",
                &defines.iter().chain(&["DEBUG_FLAGS"]).fold(
                    String::new(),
                    |mut shader, define| {
                        let _ = writeln!(shader, "#define {}", define);
                        shader
                    },
                ),
            );

            let program = unsafe { link_program(gl, VERTEX_SRC, &shader)? };
            let debug_program = unsafe { link_program(gl, VERTEX_SRC, debug_shader.as_ref())? };

            let vert = c"vert";
            let vert_position = c"vert_position";
            let tex = c"tex";
            let matrix = c"matrix";
            let tex_matrix = c"tex_matrix";
            let alpha = c"alpha";
            let radius = c"radius";
            let half_pixel = c"half_pixel";

            Ok(BlurShaderVariant {
                normal: BlurShaderProgram {
                    program,
                    uniform_tex: gl
                        .GetUniformLocation(program, tex.as_ptr() as *const ffi::types::GLchar),
                    uniform_matrix: gl
                        .GetUniformLocation(program, matrix.as_ptr() as *const ffi::types::GLchar),
                    uniform_tex_matrix: gl.GetUniformLocation(
                        program,
                        tex_matrix.as_ptr() as *const ffi::types::GLchar,
                    ),
                    uniform_alpha: gl
                        .GetUniformLocation(program, alpha.as_ptr() as *const ffi::types::GLchar),
                    uniform_radius: gl
                        .GetUniformLocation(program, radius.as_ptr() as *const ffi::types::GLchar),
                    uniform_half_pixel: gl.GetUniformLocation(
                        program,
                        half_pixel.as_ptr() as *const ffi::types::GLchar,
                    ),
                    attrib_vert: gl
                        .GetAttribLocation(program, vert.as_ptr() as *const ffi::types::GLchar),
                    attrib_vert_position: gl.GetAttribLocation(
                        program,
                        vert_position.as_ptr() as *const ffi::types::GLchar,
                    ),
                },
                debug: BlurShaderProgram {
                    program: debug_program,
                    uniform_tex: gl.GetUniformLocation(
                        debug_program,
                        tex.as_ptr() as *const ffi::types::GLchar,
                    ),
                    uniform_matrix: gl.GetUniformLocation(
                        debug_program,
                        matrix.as_ptr() as *const ffi::types::GLchar,
                    ),
                    uniform_tex_matrix: gl.GetUniformLocation(
                        debug_program,
                        tex_matrix.as_ptr() as *const ffi::types::GLchar,
                    ),
                    uniform_alpha: gl.GetUniformLocation(
                        debug_program,
                        alpha.as_ptr() as *const ffi::types::GLchar,
                    ),
                    uniform_radius: gl.GetUniformLocation(
                        debug_program,
                        radius.as_ptr() as *const ffi::types::GLchar,
                    ),
                    uniform_half_pixel: gl.GetUniformLocation(
                        debug_program,
                        half_pixel.as_ptr() as *const ffi::types::GLchar,
                    ),
                    attrib_vert: gl.GetAttribLocation(
                        debug_program,
                        vert.as_ptr() as *const ffi::types::GLchar,
                    ),
                    attrib_vert_position: gl.GetAttribLocation(
                        debug_program,
                        vert_position.as_ptr() as *const ffi::types::GLchar,
                    ),
                },
            })
        };

        Ok(BlurShader(Arc::new([
            create_variant(&[])?,
            create_variant(&["NO_ALPHA"])?,
        ])))
    }
}

#[derive(Clone, Debug)]
pub struct BlurShaderVariant {
    pub(super) normal: BlurShaderProgram,
    pub(super) debug: BlurShaderProgram,
}

#[derive(Copy, Clone, Debug)]
pub struct BlurShaderProgram {
    pub(super) program: ffi::types::GLuint,
    pub(super) uniform_tex: ffi::types::GLint,
    pub(super) uniform_tex_matrix: ffi::types::GLint,
    pub(super) uniform_matrix: ffi::types::GLint,
    pub(super) uniform_alpha: ffi::types::GLint,
    pub(super) uniform_radius: ffi::types::GLint,
    pub(super) uniform_half_pixel: ffi::types::GLint,
    pub(super) attrib_vert: ffi::types::GLint,
    pub(super) attrib_vert_position: ffi::types::GLint,
}
