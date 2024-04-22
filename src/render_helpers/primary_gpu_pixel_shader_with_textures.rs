use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::rc::Rc;

use glam::{Mat3, Vec2};
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{
    ffi, link_program, Capability, GlesError, GlesFrame, GlesRenderer, GlesTexture, Uniform,
    UniformDesc, UniformName,
};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::utils::{Buffer, Logical, Physical, Rectangle, Scale, Size};

use super::renderer::AsGlesFrame;
use super::resources::Resources;
use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};

/// Wrapper for a pixel shader from the primary GPU for rendering with the primary GPU.
///
/// The shader accepts textures as input.
#[derive(Debug)]
pub struct PrimaryGpuPixelShaderWithTexturesRenderElement {
    shader: PixelWithTexturesProgram,
    textures: HashMap<String, GlesTexture>,
    id: Id,
    commit_counter: CommitCounter,
    area: Rectangle<i32, Logical>,
    size: Size<f64, Buffer>,
    opaque_regions: Vec<Rectangle<i32, Logical>>,
    alpha: f32,
    additional_uniforms: Vec<Uniform<'static>>,
    kind: Kind,
}

#[derive(Debug, Clone)]
pub struct PixelWithTexturesProgram(Rc<PixelWithTexturesProgramInner>);

#[derive(Debug)]
struct PixelWithTexturesProgramInner {
    program: ffi::types::GLuint,
    uniform_tex_matrix: ffi::types::GLint,
    uniform_matrix: ffi::types::GLint,
    uniform_size: ffi::types::GLint,
    uniform_alpha: ffi::types::GLint,
    attrib_vert: ffi::types::GLint,
    attrib_vert_position: ffi::types::GLint,
    additional_uniforms: HashMap<String, UniformDesc>,
    texture_uniforms: HashMap<String, ffi::types::GLint>,
}

unsafe fn compile_program(
    gl: &ffi::Gles2,
    src: &str,
    additional_uniforms: &[UniformName<'_>],
    texture_uniforms: &[&str],
    // destruction_callback_sender: Sender<CleanupResource>,
) -> Result<PixelWithTexturesProgram, GlesError> {
    let shader = src;

    let program = unsafe { link_program(gl, include_str!("shaders/texture.vert"), shader)? };

    let vert = CStr::from_bytes_with_nul(b"vert\0").expect("NULL terminated");
    let vert_position = CStr::from_bytes_with_nul(b"vert_position\0").expect("NULL terminated");
    let matrix = CStr::from_bytes_with_nul(b"matrix\0").expect("NULL terminated");
    let tex_matrix = CStr::from_bytes_with_nul(b"tex_matrix\0").expect("NULL terminated");
    let size = CStr::from_bytes_with_nul(b"niri_size\0").expect("NULL terminated");
    let alpha = CStr::from_bytes_with_nul(b"niri_alpha\0").expect("NULL terminated");

    Ok(PixelWithTexturesProgram(Rc::new(
        PixelWithTexturesProgramInner {
            program,
            uniform_matrix: gl
                .GetUniformLocation(program, matrix.as_ptr() as *const ffi::types::GLchar),
            uniform_tex_matrix: gl
                .GetUniformLocation(program, tex_matrix.as_ptr() as *const ffi::types::GLchar),
            uniform_size: gl
                .GetUniformLocation(program, size.as_ptr() as *const ffi::types::GLchar),
            uniform_alpha: gl
                .GetUniformLocation(program, alpha.as_ptr() as *const ffi::types::GLchar),
            attrib_vert: gl.GetAttribLocation(program, vert.as_ptr() as *const ffi::types::GLchar),
            attrib_vert_position: gl
                .GetAttribLocation(program, vert_position.as_ptr() as *const ffi::types::GLchar),
            additional_uniforms: additional_uniforms
                .iter()
                .map(|uniform| {
                    let name =
                        CString::new(uniform.name.as_bytes()).expect("Interior null in name");
                    let location =
                        gl.GetUniformLocation(program, name.as_ptr() as *const ffi::types::GLchar);
                    (
                        uniform.name.clone().into_owned(),
                        UniformDesc {
                            location,
                            type_: uniform.type_,
                        },
                    )
                })
                .collect(),
            texture_uniforms: texture_uniforms
                .iter()
                .map(|name_| {
                    let name = CString::new(name_.as_bytes()).expect("Interior null in name");
                    let location =
                        gl.GetUniformLocation(program, name.as_ptr() as *const ffi::types::GLchar);
                    (name_.to_string(), location)
                })
                .collect(),
        },
    )))
}

impl PixelWithTexturesProgram {
    pub fn compile(
        renderer: &mut GlesRenderer,
        src: &str,
        additional_uniforms: &[UniformName<'_>],
        texture_uniforms: &[&str],
    ) -> Result<Self, GlesError> {
        renderer.with_context(move |gl| unsafe {
            compile_program(gl, src, additional_uniforms, texture_uniforms)
        })?
    }

    pub fn destroy(self, renderer: &mut GlesRenderer) -> Result<(), GlesError> {
        renderer.with_context(move |gl| unsafe {
            gl.DeleteProgram(self.0.program);
        })
    }
}

impl PrimaryGpuPixelShaderWithTexturesRenderElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shader: PixelWithTexturesProgram,
        textures: HashMap<String, GlesTexture>,
        area: Rectangle<i32, Logical>,
        size: Size<f64, Buffer>,
        opaque_regions: Option<Vec<Rectangle<i32, Logical>>>,
        alpha: f32,
        additional_uniforms: Vec<Uniform<'_>>,
        kind: Kind,
    ) -> Self {
        Self {
            shader,
            textures,
            id: Id::new(),
            commit_counter: CommitCounter::default(),
            area,
            size,
            opaque_regions: opaque_regions.unwrap_or_default(),
            alpha,
            additional_uniforms: additional_uniforms
                .into_iter()
                .map(|u| u.into_owned())
                .collect(),
            kind,
        }
    }
}

impl Element for PrimaryGpuPixelShaderWithTexturesRenderElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit_counter
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_loc_and_size((0., 0.), self.size.to_f64())
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.area.to_physical_precise_round(scale)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> Vec<Rectangle<i32, Physical>> {
        self.opaque_regions
            .iter()
            .map(|region| region.to_physical_precise_round(scale))
            .collect()
    }

    fn alpha(&self) -> f32 {
        1.0
    }

    fn kind(&self) -> Kind {
        self.kind
    }
}

impl RenderElement<GlesRenderer> for PrimaryGpuPixelShaderWithTexturesRenderElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_>,
        src: Rectangle<f64, Buffer>,
        dest: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        let frame = frame.as_gles_frame();

        let Some(resources) = Resources::get(frame) else {
            return Ok(());
        };
        let mut resources = resources.borrow_mut();

        let supports_instancing = frame.capabilities().contains(&Capability::Instancing);

        // prepare the vertices
        resources.vertices.clear();
        if supports_instancing {
            resources.vertices.extend(damage.iter().flat_map(|rect| {
                let dest_size = dest.size;

                let rect_constrained_loc = rect
                    .loc
                    .constrain(Rectangle::from_extemities((0, 0), dest_size.to_point()));
                let rect_clamped_size = rect.size.clamp(
                    (0, 0),
                    (dest_size.to_point() - rect_constrained_loc).to_size(),
                );

                let rect = Rectangle::from_loc_and_size(rect_constrained_loc, rect_clamped_size);
                [
                    rect.loc.x as f32,
                    rect.loc.y as f32,
                    rect.size.w as f32,
                    rect.size.h as f32,
                ]
            }));
        } else {
            resources.vertices.extend(damage.iter().flat_map(|rect| {
                let dest_size = dest.size;

                let rect_constrained_loc = rect
                    .loc
                    .constrain(Rectangle::from_extemities((0, 0), dest_size.to_point()));
                let rect_clamped_size = rect.size.clamp(
                    (0, 0),
                    (dest_size.to_point() - rect_constrained_loc).to_size(),
                );

                let rect = Rectangle::from_loc_and_size(rect_constrained_loc, rect_clamped_size);
                // Add the 4 f32s per damage rectangle for each of the 6 vertices.
                (0..6).flat_map(move |_| {
                    [
                        rect.loc.x as f32,
                        rect.loc.y as f32,
                        rect.size.w as f32,
                        rect.size.h as f32,
                    ]
                })
            }));
        }

        if resources.vertices.is_empty() {
            return Ok(());
        }

        // dest position and scale
        let mut matrix = Mat3::from_translation(Vec2::new(dest.loc.x as f32, dest.loc.y as f32));

        let scale = src.size.to_f64() / dest.size.to_f64();
        let tex_matrix = Mat3::from_scale(Vec2::new(scale.x as f32, scale.y as f32));
        let tex_matrix =
            Mat3::from_translation(Vec2::new(src.loc.x as f32, src.loc.y as f32)) * tex_matrix;
        let tex_matrix = Mat3::from_scale(Vec2::new(
            (1.0f64 / self.size.w) as f32,
            (1.0f64 / self.size.h) as f32,
        )) * tex_matrix;

        //apply output transformation
        matrix = Mat3::from_cols_array(frame.projection()) * matrix;

        let program = &self.shader.0;

        // render
        frame.with_context(move |gl| -> Result<(), GlesError> {
            unsafe {
                for (i, texture) in self.textures.values().enumerate() {
                    gl.ActiveTexture(ffi::TEXTURE0 + i as u32);
                    gl.BindTexture(ffi::TEXTURE_2D, texture.tex_id());
                    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as i32);
                    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as i32);
                    gl.TexParameteri(
                        ffi::TEXTURE_2D,
                        ffi::TEXTURE_WRAP_S,
                        ffi::CLAMP_TO_BORDER as i32,
                    );
                    gl.TexParameteri(
                        ffi::TEXTURE_2D,
                        ffi::TEXTURE_WRAP_T,
                        ffi::CLAMP_TO_BORDER as i32,
                    );
                }

                gl.UseProgram(program.program);

                for (i, name) in self.textures.keys().enumerate() {
                    gl.Uniform1i(program.texture_uniforms[name], i as i32);
                }

                gl.UniformMatrix3fv(
                    program.uniform_matrix,
                    1,
                    ffi::FALSE,
                    matrix.as_ref().as_ptr(),
                );
                gl.UniformMatrix3fv(
                    program.uniform_tex_matrix,
                    1,
                    ffi::FALSE,
                    tex_matrix.as_ref().as_ptr(),
                );
                gl.Uniform2f(program.uniform_size, dest.size.w as f32, dest.size.h as f32);
                gl.Uniform1f(program.uniform_alpha, self.alpha);

                for uniform in &self.additional_uniforms {
                    let desc =
                        program
                            .additional_uniforms
                            .get(&*uniform.name)
                            .ok_or_else(|| {
                                GlesError::UnknownUniform(uniform.name.clone().into_owned())
                            })?;
                    uniform.value.set(gl, desc)?;
                }

                gl.EnableVertexAttribArray(program.attrib_vert as u32);
                gl.BindBuffer(ffi::ARRAY_BUFFER, resources.vbos[0]);
                gl.VertexAttribPointer(
                    program.attrib_vert as u32,
                    2,
                    ffi::FLOAT,
                    ffi::FALSE,
                    0,
                    std::ptr::null(),
                );

                // vert_position
                gl.EnableVertexAttribArray(program.attrib_vert_position as u32);
                gl.BindBuffer(ffi::ARRAY_BUFFER, resources.vbos[1]);
                gl.BufferData(
                    ffi::ARRAY_BUFFER,
                    (std::mem::size_of::<ffi::types::GLfloat>() * resources.vertices.len())
                        as isize,
                    resources.vertices.as_ptr() as *const _,
                    ffi::STREAM_DRAW,
                );

                gl.VertexAttribPointer(
                    program.attrib_vert_position as u32,
                    4,
                    ffi::FLOAT,
                    ffi::FALSE,
                    0,
                    std::ptr::null(),
                );

                let damage_len = damage.len() as i32;
                if supports_instancing {
                    gl.VertexAttribDivisor(program.attrib_vert as u32, 0);
                    gl.VertexAttribDivisor(program.attrib_vert_position as u32, 1);
                    gl.DrawArraysInstanced(ffi::TRIANGLE_STRIP, 0, 4, damage_len);
                } else {
                    // When we have more than 10 rectangles, draw them in batches of 10.
                    for i in 0..(damage_len - 1) / 10 {
                        gl.DrawArrays(ffi::TRIANGLES, 0, 6);

                        // Set damage pointer to the next 10 rectangles.
                        let offset =
                            (i + 1) as usize * 6 * 4 * std::mem::size_of::<ffi::types::GLfloat>();
                        gl.VertexAttribPointer(
                            program.attrib_vert_position as u32,
                            4,
                            ffi::FLOAT,
                            ffi::FALSE,
                            0,
                            offset as *const _,
                        );
                    }

                    // Draw the up to 10 remaining rectangles.
                    let count = ((damage_len - 1) % 10 + 1) * 6;
                    gl.DrawArrays(ffi::TRIANGLES, 0, count);
                }

                gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
                gl.BindTexture(ffi::TEXTURE_2D, 0);
                gl.ActiveTexture(ffi::TEXTURE0);
                gl.BindTexture(ffi::TEXTURE_2D, 0);
                gl.DisableVertexAttribArray(program.attrib_vert as u32);
                gl.DisableVertexAttribArray(program.attrib_vert_position as u32);
            }

            Ok(())
        })??;

        Ok(())
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}

impl<'render> RenderElement<TtyRenderer<'render>>
    for PrimaryGpuPixelShaderWithTexturesRenderElement
{
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        let frame = frame.as_gles_frame();

        RenderElement::<GlesRenderer>::draw(self, frame, src, dst, damage)?;

        Ok(())
    }

    fn underlying_storage(
        &self,
        _renderer: &mut TtyRenderer<'render>,
    ) -> Option<UnderlyingStorage> {
        // If scanout for things other than Wayland buffers is implemented, this will need to take
        // the target GPU into account.
        None
    }
}
