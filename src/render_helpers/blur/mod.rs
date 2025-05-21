// Ported from https://github.com/nferhat/fht-compositor/blob/main/src/renderer/blur/mod.rs

pub mod element;
pub(super) mod shader;

use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use element::BlurConfig;
use glam::{Mat3, Vec2};
use smithay::backend::renderer::gles::format::fourcc_to_gl_formats;
use smithay::backend::renderer::gles::{ffi, GlesError, GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Offscreen, Texture};
use smithay::output::Output;
use smithay::reexports::gbm::Format;
use smithay::utils::{Buffer, Physical, Point, Rectangle, Size, Transform};

use crate::render_helpers::renderer::NiriRenderer;
use shader::BlurShaders;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum CurrentBuffer {
    /// We are currently sampling from normal buffer, and rendering in the swapped/alternative.
    #[default]
    Normal,
    /// We are currently sampling from swapped buffer, and rendering in the normal.
    Swapped,
}

impl CurrentBuffer {
    pub fn swap(&mut self) {
        *self = match self {
            // sampled from normal, render to swapped
            Self::Normal => Self::Swapped,
            // sampled fro swapped, render to normal next
            Self::Swapped => Self::Normal,
        }
    }
}

static mut LAST_OUTPUT: Option<Output> = None;

/// Effect framebuffers associated with each output.
pub struct EffectsFramebuffers {
    // /// Contains the original pixels before blurring to draw with in case of artifacts.
    // blur_saved_pixels: GlesTexture,
    // The blur algorithms (dual-kawase) swaps between these two whenever scaling the image
    effects: GlesTexture,
    effects_swapped: GlesTexture,
    /// The buffer we are currently rendering/sampling from.
    ///
    /// In order todo the up/downscaling, we render into different buffers. On each pass, we render
    /// into a different buffer with downscaling/upscaling (depending on which pass we are at)
    ///
    /// One exception is that if we are on the first pass, we are on [`CurrentBuffer::Initial`], we
    /// are sampling from [`Self::blit_buffer`] from initial screen contents.
    current_buffer: CurrentBuffer,
}

type EffectsFramebufffersUserData = Rc<RefCell<EffectsFramebuffers>>;

impl EffectsFramebuffers {
    /// Get the assiciated [`EffectsFramebuffers`] with this output.
    pub fn get<'a>(output: &'a Output) -> RefMut<'a, Self> {
        let user_data = output
            .user_data()
            .get::<EffectsFramebufffersUserData>()
            .unwrap();
        RefCell::borrow_mut(user_data)
    }

    pub fn get_last_output() -> Option<Output> {
        //@todo this is just a hack to test the blur, we need a way to get
        // the output from the window itself
        unsafe { LAST_OUTPUT.clone() }
    }

    /// Initialize the [`EffectsFramebuffers`] for an [`Output`].
    ///
    /// The framebuffers handles live inside the Output's user data, use [`Self::get`] to access
    /// them.
    pub fn init_for_output(output: Output, renderer: &mut impl NiriRenderer) {
        let renderer = renderer.as_gles_renderer();
        let output_size = output.current_mode().unwrap().size;

        unsafe {
            LAST_OUTPUT = Some(output.clone());
        }

        fn create_buffer(
            renderer: &mut GlesRenderer,
            size: Size<i32, Physical>,
        ) -> Result<GlesTexture, GlesError> {
            renderer.create_buffer(
                Format::Abgr8888,
                size.to_logical(1).to_buffer(1, Transform::Normal),
            )
        }

        let this = EffectsFramebuffers {
            effects: create_buffer(renderer, output_size).unwrap(),
            effects_swapped: create_buffer(renderer, output_size).unwrap(),
            current_buffer: CurrentBuffer::Normal,
        };

        let user_data = output.user_data();
        assert!(
            user_data.insert_if_missing(|| Rc::new(RefCell::new(this))),
            "EffectsFrambuffers::init_for_output should only be called once!"
        );
    }

    /// Update the [`EffectsFramebuffers`] for an [`Output`].
    ///
    /// You should call this if the output's scale/size changes
    pub fn update_for_output(
        output: Output,
        renderer: &mut impl NiriRenderer,
    ) -> Result<(), GlesError> {
        let renderer = renderer.as_gles_renderer();
        let mut fx_buffers = Self::get(&output);
        let output_size = output.current_mode().unwrap().size;

        fn create_buffer(
            renderer: &mut GlesRenderer,
            size: Size<i32, Physical>,
        ) -> Result<GlesTexture, GlesError> {
            renderer.create_buffer(
                Format::Abgr8888,
                size.to_logical(1).to_buffer(1, Transform::Normal),
            )
        }

        *fx_buffers = EffectsFramebuffers {
            effects: create_buffer(renderer, output_size)?,
            effects_swapped: create_buffer(renderer, output_size)?,
            current_buffer: CurrentBuffer::Normal,
        };

        Ok(())
    }

    /// Get the sample and render buffers.
    pub fn buffers(&mut self) -> (&GlesTexture, &mut GlesTexture) {
        match self.current_buffer {
            CurrentBuffer::Normal => (&self.effects, &mut self.effects_swapped),
            CurrentBuffer::Swapped => (&self.effects_swapped, &mut self.effects),
        }
    }
}

pub(super) unsafe fn get_main_buffer_blur(
    gl: &ffi::Gles2,
    fx_buffers: &mut EffectsFramebuffers,
    shaders: &BlurShaders,
    blur_config: BlurConfig,
    projection_matrix: Mat3,
    scale: i32,
    vbos: &[u32; 2],
    debug: bool,
    supports_instancing: bool,
    // dst is the region that we want blur on
    dst: Rectangle<i32, Physical>,
) -> Result<GlesTexture, GlesError> {
    let tex_size = fx_buffers
        .effects
        .size()
        .to_logical(1, Transform::Normal)
        .to_physical(scale);

    let dst_expanded = {
        let mut dst = dst;
        let size = (2f32.powi(blur_config.passes as i32 + 1) * blur_config.radius).ceil() as i32;
        dst.loc -= Point::from((size, size));
        dst.size += Size::from((size, size)).upscale(2);
        dst
    };

    let mut prev_fbo = 0;
    gl.GetIntegerv(ffi::FRAMEBUFFER_BINDING, &mut prev_fbo as *mut _);

    let (sample_buffer, _) = fx_buffers.buffers();

    // First get a fbo for the texture we are about to read into
    let mut sample_fbo = 0u32;
    {
        gl.GenFramebuffers(1, &mut sample_fbo as *mut _);
        gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, sample_fbo);
        gl.FramebufferTexture2D(
            ffi::FRAMEBUFFER,
            ffi::COLOR_ATTACHMENT0,
            ffi::TEXTURE_2D,
            sample_buffer.tex_id(),
            0,
        );
        gl.Clear(ffi::COLOR_BUFFER_BIT);
        let status = gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
        if status != ffi::FRAMEBUFFER_COMPLETE {
            gl.DeleteFramebuffers(1, &mut sample_fbo as *mut _);
            return Err(GlesError::FramebufferBindingError);
        }
    }

    {
        // NOTE: We are assured that the size of the effects texture is the same
        // as the bound fbo size, so blitting uses dst immediatly
        gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, sample_fbo);
        // Flip the destination Y coordinates to fix upside-down image
        let y0 = dst_expanded.loc.y;
        let y1 = dst_expanded.loc.y + dst_expanded.size.h;
        gl.BlitFramebuffer(
            dst_expanded.loc.x,
            y0,
            dst_expanded.loc.x + dst_expanded.size.w,
            y1,
            dst_expanded.loc.x,
            y1, // flip Y: destination top is bottom
            dst_expanded.loc.x + dst_expanded.size.w,
            y0, // destination bottom is top
            ffi::COLOR_BUFFER_BIT,
            ffi::LINEAR,
        );

        if gl.GetError() == ffi::INVALID_OPERATION {
            error!("TrueBlur needs GLES3.0 for blitting");
            return Err(GlesError::BlitError);
        }
    }

    {
        let passes = blur_config.passes;
        let half_pixel = [
            0.5 / (tex_size.w as f32 / 2.0),
            0.5 / (tex_size.h as f32 / 2.0),
        ];
        for i in 0..passes {
            let (sample_buffer, render_buffer) = fx_buffers.buffers();
            let damage = dst_expanded.downscale(1 << (i + 1));
            render_blur_pass_with_gl(
                gl,
                vbos,
                debug,
                supports_instancing,
                projection_matrix,
                sample_buffer,
                render_buffer,
                scale,
                &shaders.down,
                half_pixel,
                blur_config.clone(),
                damage,
            )?;
            fx_buffers.current_buffer.swap();
        }

        let half_pixel = [
            0.5 / (tex_size.w as f32 * 2.0),
            0.5 / (tex_size.h as f32 * 2.0),
        ];
        for i in 0..passes {
            let (sample_buffer, render_buffer) = fx_buffers.buffers();
            let damage = dst_expanded.downscale(1 << (passes - 1 - i));
            render_blur_pass_with_gl(
                gl,
                &vbos,
                debug,
                supports_instancing,
                projection_matrix,
                sample_buffer,
                render_buffer,
                scale,
                &shaders.up,
                half_pixel,
                blur_config.clone(),
                damage,
            )?;
            fx_buffers.current_buffer.swap();
        }
    }

    // Cleanup
    {
        gl.DeleteFramebuffers(1, &mut sample_fbo as *mut _);
        gl.BindFramebuffer(ffi::FRAMEBUFFER, prev_fbo as u32);
    }

    Ok(fx_buffers.effects.clone())
}

// Renders a blur pass using gl code bypassing smithay's Frame mechanisms
//
// When rendering blur in real-time (for windows, for example) there should not be a wait for
// fencing/finishing since this will be done when sending the fb to the output. Using a Frame
// forces us to do that.
unsafe fn render_blur_pass_with_gl(
    gl: &ffi::Gles2,
    vbos: &[u32; 2],
    debug: bool,
    supports_instancing: bool,
    projection_matrix: Mat3,
    // The buffers used for blurring
    sample_buffer: &GlesTexture,
    render_buffer: &mut GlesTexture,
    scale: i32,
    // The current blur program + config
    blur_program: &shader::BlurShader,
    half_pixel: [f32; 2],
    config: BlurConfig,
    // dst is the region that should have blur
    // it gets up/downscaled with passes
    damage: Rectangle<i32, Physical>,
) -> Result<(), GlesError> {
    let tex_size = sample_buffer.size();
    let src = Rectangle::from_size(tex_size.to_f64());
    let dest = src
        .to_logical(1.0, Transform::Normal, &src.size)
        .to_physical(scale as f64)
        .to_i32_round();

    // FIXME: Should we call gl.Finish() when done rendering this pass? If yes, should we check
    // if the gl context is shared or not? What about fencing, we don't have access to that

    // PERF: Instead of taking the whole src/dst as damage, adapt the code to run with only the
    // damaged window? This would cause us to make a custom WaylandSurfaceRenderElement to blur out
    // stuff. Complicated.

    // First bind to our render buffer
    let mut render_buffer_fbo = 0;
    {
        gl.GenFramebuffers(1, &mut render_buffer_fbo as *mut _);
        gl.BindFramebuffer(ffi::FRAMEBUFFER, render_buffer_fbo);
        gl.FramebufferTexture2D(
            ffi::FRAMEBUFFER,
            ffi::COLOR_ATTACHMENT0,
            ffi::TEXTURE_2D,
            render_buffer.tex_id(),
            0,
        );

        let status = gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
        if status != ffi::FRAMEBUFFER_COMPLETE {
            return Err(GlesError::FramebufferBindingError);
        }
    }

    {
        let mat = projection_matrix;
        // NOTE: We are assured that tex_size != 0, and src.size != too (by damage tracker)
        let mut tex_mat = build_texture_mat(src, dest, tex_size, Transform::Normal);

        tex_mat *= Mat3::from_cols_array(&[1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0]);

        gl.Disable(ffi::BLEND);

        // FIXME: Use actual damage for this? Would require making a custom window render element
        // that includes blur and whatnot to get the damage for the window only
        let damage = [
            damage.loc.x as f32,
            damage.loc.y as f32,
            damage.size.w as f32,
            damage.size.h as f32,
        ];

        let mut vertices = Vec::with_capacity(4);
        let damage_len = if supports_instancing {
            vertices.extend(damage);
            vertices.len() / 4
        } else {
            for _ in 0..6 {
                // Add the 4 f32s per damage rectangle for each of the 6 vertices.
                vertices.extend_from_slice(&damage);
            }

            1
        };

        // SAFETY: internal texture should always have a format
        // We also use Abgr8888 which is known and confirmed
        let (internal_format, _, _) =
            fourcc_to_gl_formats(sample_buffer.format().unwrap()).unwrap();
        let variant = blur_program.variant_for_format(Some(internal_format), false);

        let program = if debug {
            &variant.debug
        } else {
            &variant.normal
        };

        gl.ActiveTexture(ffi::TEXTURE0);
        gl.BindTexture(ffi::TEXTURE_2D, sample_buffer.tex_id());
        gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as i32);
        gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as i32);
        gl.UseProgram(program.program);

        gl.Uniform1i(program.uniform_tex, 0);
        gl.UniformMatrix3fv(
            program.uniform_matrix,
            1,
            ffi::FALSE,
            mat.as_ref() as *const f32,
        );
        gl.UniformMatrix3fv(
            program.uniform_tex_matrix,
            1,
            ffi::FALSE,
            tex_mat.as_ref() as *const f32,
        );
        gl.Uniform1f(program.uniform_alpha, 1.0);
        gl.Uniform1f(program.uniform_radius, config.radius);
        gl.Uniform2f(program.uniform_half_pixel, half_pixel[0], half_pixel[1]);

        gl.EnableVertexAttribArray(program.attrib_vert as u32);
        gl.BindBuffer(ffi::ARRAY_BUFFER, vbos[0]);
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
        gl.BindBuffer(ffi::ARRAY_BUFFER, 0);

        gl.VertexAttribPointer(
            program.attrib_vert_position as u32,
            4,
            ffi::FLOAT,
            ffi::FALSE,
            0,
            vertices.as_ptr() as *const _,
        );

        if supports_instancing {
            gl.VertexAttribDivisor(program.attrib_vert as u32, 0);
            gl.VertexAttribDivisor(program.attrib_vert_position as u32, 1);
            gl.DrawArraysInstanced(ffi::TRIANGLE_STRIP, 0, 4, damage_len as i32);
        } else {
            let count = damage_len * 6;
            gl.DrawArrays(ffi::TRIANGLES, 0, count as i32);
        }

        gl.BindTexture(ffi::TEXTURE_2D, 0);
        gl.DisableVertexAttribArray(program.attrib_vert as u32);
        gl.DisableVertexAttribArray(program.attrib_vert_position as u32);
    }

    // Clean up
    {
        gl.Enable(ffi::BLEND);
        gl.DeleteFramebuffers(1, &render_buffer_fbo as *const _);
        gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
        gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
    }

    Ok(())
}

// Copied from smithay, adapted to use glam structs
fn build_texture_mat(
    src: Rectangle<f64, Buffer>,
    dest: Rectangle<i32, Physical>,
    texture: Size<i32, Buffer>,
    transform: Transform,
) -> Mat3 {
    let dst_src_size = transform.transform_size(src.size);
    let scale = dst_src_size.to_f64() / dest.size.to_f64();

    let mut tex_mat = Mat3::IDENTITY;
    // first bring the damage into src scale
    tex_mat = Mat3::from_scale(Vec2::new(scale.x as f32, scale.y as f32)) * tex_mat;

    // then compensate for the texture transform
    let transform_mat = Mat3::from_cols_array(transform.matrix().as_ref());
    let translation = match transform {
        Transform::Normal => Mat3::IDENTITY,
        Transform::_90 => Mat3::from_translation(Vec2::new(0f32, dst_src_size.w as f32)),
        Transform::_180 => {
            Mat3::from_translation(Vec2::new(dst_src_size.w as f32, dst_src_size.h as f32))
        }
        Transform::_270 => Mat3::from_translation(Vec2::new(dst_src_size.h as f32, 0f32)),
        Transform::Flipped => Mat3::from_translation(Vec2::new(dst_src_size.w as f32, 0f32)),
        Transform::Flipped90 => Mat3::IDENTITY,
        Transform::Flipped180 => Mat3::from_translation(Vec2::new(0f32, dst_src_size.h as f32)),
        Transform::Flipped270 => {
            Mat3::from_translation(Vec2::new(dst_src_size.h as f32, dst_src_size.w as f32))
        }
    };
    tex_mat = transform_mat * tex_mat;
    tex_mat = translation * tex_mat;

    // now we can add the src crop loc, the size already done implicit by the src size
    tex_mat = Mat3::from_translation(Vec2::new(src.loc.x as f32, src.loc.y as f32)) * tex_mat;

    // at last we have to normalize the values for UV space
    tex_mat = Mat3::from_scale(Vec2::new(
        (1.0f64 / texture.w as f64) as f32,
        (1.0f64 / texture.h as f64) as f32,
    )) * tex_mat;

    tex_mat
}
