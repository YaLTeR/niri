use std::cmp::max;
use std::iter::{once, zip};
use std::ptr::null;
use std::rc::Rc;

use anyhow::Context as _;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::{
    ffi, link_program, GlesError, GlesRenderer, GlesTexProgram, GlesTexture, UniformName,
    UniformType,
};
use smithay::backend::renderer::{Offscreen as _, Texture as _};
use smithay::gpu_span_location;

#[derive(Debug)]
pub struct Blur {
    program: BlurProgram,
    texture: Option<GlesTexture>,
    pub config: niri_config::Blur,
}

#[derive(Debug, Clone)]
pub struct BlurProgram(Rc<BlurProgramInner>);

#[derive(Debug)]
struct BlurProgramInner {
    down: BlurProgramInternal,
    up: BlurProgramInternal,
    render: GlesTexProgram,
}

#[derive(Debug)]
struct BlurProgramInternal {
    program: ffi::types::GLuint,
    uniform_tex_matrix: ffi::types::GLint,
    uniform_matrix: ffi::types::GLint,
    uniform_tex: ffi::types::GLint,
    uniform_half_pixel: ffi::types::GLint,
    uniform_offset: ffi::types::GLint,
    attrib_vert: ffi::types::GLint,
}

unsafe fn compile_program(gl: &ffi::Gles2, src: &str) -> Result<BlurProgramInternal, GlesError> {
    let program = unsafe { link_program(gl, include_str!("shaders/blur.vert"), src)? };

    let vert = c"vert";
    let matrix = c"matrix";
    let tex_matrix = c"tex_matrix";
    let tex = c"tex";
    let half_pixel = c"half_pixel";
    let offset = c"offset";

    Ok(BlurProgramInternal {
        program,
        uniform_matrix: gl.GetUniformLocation(program, matrix.as_ptr()),
        uniform_tex_matrix: gl.GetUniformLocation(program, tex_matrix.as_ptr()),
        uniform_tex: gl.GetUniformLocation(program, tex.as_ptr()),
        uniform_half_pixel: gl.GetUniformLocation(program, half_pixel.as_ptr()),
        uniform_offset: gl.GetUniformLocation(program, offset.as_ptr()),
        attrib_vert: gl.GetAttribLocation(program, vert.as_ptr()),
    })
}

impl BlurProgram {
    pub fn compile(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        let render = renderer.compile_custom_texture_shader(
            include_str!("shaders/blur_render.frag"),
            &[
                UniformName::new("niri_scale", UniformType::_1f),
                UniformName::new("geo_size", UniformType::_2f),
                UniformName::new("corner_radius", UniformType::_4f),
            ],
        )?;

        renderer.with_context(move |gl| unsafe {
            let down = compile_program(gl, include_str!("shaders/blur_down.frag"))?;
            let up = compile_program(gl, include_str!("shaders/blur_up.frag"))?;
            Ok(Self(Rc::new(BlurProgramInner { down, up, render })))
        })?
    }

    pub fn destroy(self, renderer: &mut GlesRenderer) -> Result<(), GlesError> {
        renderer.with_context(move |gl| unsafe {
            gl.DeleteProgram(self.0.down.program);
            gl.DeleteProgram(self.0.up.program);
        })
    }
}

impl Blur {
    pub fn new(program: BlurProgram) -> Self {
        Self {
            program,
            texture: None,
            config: niri_config::Blur::default(),
        }
    }

    pub fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        source: &GlesTexture,
        config: niri_config::Blur,
    ) -> anyhow::Result<GlesTexture> {
        let _span = tracy_client::span!("Blur::render");

        self.config = config;

        let passes = config.passes.clamp(1, 64) as usize;
        let offset = config.offset as f32;
        let size = source.size();
        if let Some(texture) = &self.texture {
            if texture.size() != size {
                self.texture = None;
            }
        }
        let target: GlesTexture = renderer
            .create_buffer(Fourcc::Abgr8888, size)
            .context("error creating output texture")?;

        renderer.with_profiled_context(gpu_span_location!("Blur::render"), |gl| unsafe {
            while gl.GetError() != ffi::NO_ERROR {}

            gl.Disable(ffi::BLEND);
            gl.Disable(ffi::SCISSOR_TEST);

            let mut fbos = [0; 2];
            let mut textures = vec![0; passes];
            gl.GenFramebuffers(fbos.len() as _, fbos.as_mut_ptr());
            gl.GenTextures(textures.len() as _, textures.as_mut_ptr());

            gl.ActiveTexture(ffi::TEXTURE0);
            let mut w = size.w;
            let mut h = size.h;
            for dst in textures.iter().copied() {
                w = max(1, w / 2);
                h = max(1, h / 2);

                gl.BindTexture(ffi::TEXTURE_2D, dst);
                gl.TexImage2D(
                    ffi::TEXTURE_2D,
                    0,
                    ffi::RGBA8 as _,
                    w,
                    h,
                    0,
                    ffi::RGBA,
                    ffi::UNSIGNED_BYTE,
                    null(),
                );
            }

            gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, fbos[0]);

            let program = &self.program.0.down;
            gl.UseProgram(program.program);
            gl.Uniform1i(program.uniform_tex, 0);
            gl.Uniform1f(program.uniform_offset, offset);

            let vertices: [f32; 12] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0];
            gl.EnableVertexAttribArray(program.attrib_vert as u32);
            gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
            gl.VertexAttribPointer(
                program.attrib_vert as u32,
                2,
                ffi::FLOAT,
                ffi::FALSE,
                0,
                vertices.as_ptr().cast(),
            );

            loop {
                let err = gl.GetError();
                if err == ffi::NO_ERROR {
                    break;
                }
                warn!("GL error preparing: {err}");
            }

            let src = once(source.tex_id()).chain(textures.iter().copied());
            let dst = textures.iter().copied();
            let mut w = size.w;
            let mut h = size.h;
            for (src, dst) in zip(src, dst) {
                w = max(1, w >> 1);
                h = max(1, h >> 1);
                gl.Viewport(0, 0, w, h);

                // TODO verify
                gl.Uniform2f(program.uniform_half_pixel, 1.0 / w as f32, 1.0 / h as f32);

                debug!("drawing down {src} to {dst}");
                gl.FramebufferTexture2D(
                    ffi::DRAW_FRAMEBUFFER,
                    ffi::COLOR_ATTACHMENT0,
                    ffi::TEXTURE_2D,
                    dst,
                    0,
                );

                gl.BindTexture(ffi::TEXTURE_2D, src);
                gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as i32);
                gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as i32);
                gl.TexParameteri(
                    ffi::TEXTURE_2D,
                    ffi::TEXTURE_WRAP_S,
                    ffi::CLAMP_TO_EDGE as i32,
                );
                gl.TexParameteri(
                    ffi::TEXTURE_2D,
                    ffi::TEXTURE_WRAP_T,
                    ffi::CLAMP_TO_EDGE as i32,
                );

                gl.DrawArrays(ffi::TRIANGLES, 0, 6);

                loop {
                    let err = gl.GetError();
                    if err == ffi::NO_ERROR {
                        break;
                    }
                    warn!("GL error drawing: {err}");
                }
            }

            gl.DisableVertexAttribArray(program.attrib_vert as u32);

            // Up
            let program = &self.program.0.up;
            gl.UseProgram(program.program);
            gl.Uniform1i(program.uniform_tex, 0);
            gl.Uniform1f(program.uniform_offset, offset);

            let vertices: [f32; 12] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0];
            gl.EnableVertexAttribArray(program.attrib_vert as u32);
            gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
            gl.VertexAttribPointer(
                program.attrib_vert as u32,
                2,
                ffi::FLOAT,
                ffi::FALSE,
                0,
                vertices.as_ptr().cast(),
            );

            loop {
                let err = gl.GetError();
                if err == ffi::NO_ERROR {
                    break;
                }
                warn!("GL error preparing: {err}");
            }

            let src = textures.iter().rev().copied();
            let dst = textures
                .iter()
                .rev()
                .skip(1)
                .copied()
                .chain(once(target.tex_id()));
            for (i, (src, dst)) in zip(src, dst).enumerate() {
                let w = max(1, size.w >> (textures.len() - i - 1));
                let h = max(1, size.h >> (textures.len() - i - 1));
                gl.Viewport(0, 0, w, h);

                // TODO verify
                gl.Uniform2f(program.uniform_half_pixel, 1.0 / w as f32, 1.0 / h as f32);

                debug!("drawing up {src} to {dst}");
                gl.FramebufferTexture2D(
                    ffi::DRAW_FRAMEBUFFER,
                    ffi::COLOR_ATTACHMENT0,
                    ffi::TEXTURE_2D,
                    dst,
                    0,
                );

                gl.BindTexture(ffi::TEXTURE_2D, src);
                gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as i32);
                gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as i32);
                gl.TexParameteri(
                    ffi::TEXTURE_2D,
                    ffi::TEXTURE_WRAP_S,
                    ffi::CLAMP_TO_EDGE as i32,
                );
                gl.TexParameteri(
                    ffi::TEXTURE_2D,
                    ffi::TEXTURE_WRAP_T,
                    ffi::CLAMP_TO_EDGE as i32,
                );

                gl.DrawArrays(ffi::TRIANGLES, 0, 6);

                loop {
                    let err = gl.GetError();
                    if err == ffi::NO_ERROR {
                        break;
                    }
                    warn!("GL error drawing: {err}");
                }
            }

            gl.DisableVertexAttribArray(program.attrib_vert as u32);

            gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
            gl.DeleteFramebuffers(fbos.len() as _, fbos.as_ptr());
            gl.DeleteTextures(textures.len() as _, textures.as_ptr());
        })?;

        Ok(target)
    }
}
