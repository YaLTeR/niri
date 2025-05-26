// Ported from https://github.com/nferhat/fht-compositor/blob/main/src/renderer/blur/element.rs

use niri_config::Blur;

use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture, Uniform};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::Renderer;
use smithay::output::Output;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};
use crate::render_helpers::render_data::RendererData;
use crate::render_helpers::renderer::{AsGlesFrame, NiriRenderer};
use crate::render_helpers::shaders::Shaders;

use super::optimized_blur_texture_element::OptimizedBlurTextureElement;
use super::{CurrentBuffer, EffectsFramebuffers};

#[derive(Debug)]
pub enum BlurRenderElement {
    /// Use optimized blur, aka X-ray blur.
    ///
    /// This technique relies on [`EffectsFramebuffers::optimized_blur`] to be populated. It will
    /// render this texture no matter what is below the blur render element.
    Optimized {
        tex: OptimizedBlurTextureElement,
        corner_radius: f32,
        noise: f32,
        scale: f64,
    },
    /// Use true blur.
    ///
    /// When using this technique, the compositor will blur the current framebuffer ccontents that
    /// are below the [`BlurElement`] in order to display them. This adds an additional render step
    /// but provides true results with the blurred contents.
    TrueBlur {
        // we are just a funny texture element that generates the texture on RenderElement::draw
        id: Id,
        scale: f64,
        transform: Transform,
        src: Rectangle<f64, Logical>,
        size: Size<i32, Logical>,
        corner_radius: f32,
        loc: Point<i32, Physical>,
        output: Output,
        config: Blur,
        // FIXME: Use DamageBag and expand it as needed?
        commit_counter: CommitCounter,
    },
}

impl BlurRenderElement {
    /// Create a new [`BlurElement`]. You are supposed to put this **below** the translucent surface
    /// that you want to blur. `area` is assumed to be relative to the `output` you are rendering
    /// in.
    ///
    /// If you don't update the blur optimized buffer
    /// [`EffectsFramebuffers::update_optimized_blur_buffer`] this element will either
    /// - Display outdated/wrong contents
    /// - Not display anything since the buffer will be empty.
    pub fn new(
        renderer: &mut impl NiriRenderer,
        output: &Output,
        sample_area: Rectangle<i32, Logical>,
        loc: Point<i32, Physical>,
        corner_radius: f32,
        optimized: bool,
        scale: f64,
        config: Blur,
    ) -> Self {
        let fbs = &mut *EffectsFramebuffers::get(output);
        let texture = fbs.optimized_blur.clone();

        if optimized {
            let scaled = sample_area.to_f64().upscale(scale);

            let texture = TextureRenderElement::from_static_texture(
                Id::new(),
                renderer.as_gles_renderer().context_id(),
                loc.to_f64(),
                texture,
                1,
                Transform::Normal,
                Some(1.0),
                Some(scaled),
                Some(scaled.size.to_i32_ceil()),
                // NOTE: Since this is "optimized" blur, anything below the window will not be
                // rendered
                Some(vec![Rectangle::new(
                    scaled.loc.to_i32_ceil(),
                    scaled.size.to_i32_ceil(),
                )
                .to_buffer(1, Transform::Normal, &sample_area.size)]),
                Kind::Unspecified,
            );

            Self::Optimized {
                tex: texture.into(),
                corner_radius,
                noise: config.noise.0 as f32,
                scale,
            }
        } else {
            Self::TrueBlur {
                id: Id::new(),
                scale,
                src: sample_area.to_f64(),
                transform: Transform::Normal,
                size: sample_area.size,
                corner_radius,
                loc,
                config,
                output: output.clone(), // fixme i hate this
                commit_counter: CommitCounter::default(),
            }
        }
    }
}

impl Element for BlurRenderElement {
    fn id(&self) -> &Id {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.id(),
            BlurRenderElement::TrueBlur { id, .. } => id,
        }
    }

    fn current_commit(&self) -> CommitCounter {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.current_commit(),
            BlurRenderElement::TrueBlur { commit_counter, .. } => *commit_counter,
        }
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.location(scale),
            BlurRenderElement::TrueBlur { loc, .. } => *loc,
        }
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.src(),
            BlurRenderElement::TrueBlur {
                src,
                transform,
                size,
                scale,
                ..
            } => src.to_buffer(*scale as f64, *transform, &size.to_f64()),
        }
    }

    fn transform(&self) -> Transform {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.transform(),
            BlurRenderElement::TrueBlur { transform, .. } => *transform,
        }
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.damage_since(scale, commit),
            BlurRenderElement::TrueBlur { config, .. } => {
                let passes = config.passes;
                let radius = config.radius.0 as f32;

                // Since the blur element samples from around itself, we must expand the damage it
                // induces to include any potential changes.
                let mut geometry = Rectangle::from_size(self.geometry(scale).size);
                let size = (2f32.powi(passes as i32 + 1) * radius).ceil() as i32;
                geometry.loc -= Point::from((size, size));
                geometry.size += Size::from((size, size)).upscale(2);

                // FIXME: Damage tracking?
                DamageSet::from_slice(&[geometry])
            }
        }
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.opaque_regions(scale),
            BlurRenderElement::TrueBlur { .. } => {
                // Since we are rendering as true blur, we will draw whatever is behind the window
                OpaqueRegions::default()
            }
        }
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        match self {
            BlurRenderElement::Optimized { tex, .. } => tex.geometry(scale),
            BlurRenderElement::TrueBlur { loc, size, .. } => {
                Rectangle::new(*loc, size.to_physical_precise_round(scale))
            }
        }
    }

    fn alpha(&self) -> f32 {
        1.0
    }

    fn kind(&self) -> Kind {
        Kind::Unspecified
    }
}

fn draw_true_blur(
    output: &Output,
    gles_frame: &mut GlesFrame,
    config: &Blur,
    scale: f64,
    dst: Rectangle<i32, Physical>,
    corner_radius: f32,
    src: Rectangle<f64, Buffer>,
    damage: &[Rectangle<i32, Physical>],
    opaque_regions: &[Rectangle<i32, Physical>],
    alpha: f32,
    is_tty: bool,
) -> Result<(), GlesError> {
    let mut fx_buffers = EffectsFramebuffers::get(output);
    fx_buffers.current_buffer = CurrentBuffer::Normal;

    let shaders = Shaders::get_from_frame(gles_frame).blur.clone();
    let vbos = RendererData::get_from_frame(gles_frame).vbos;
    let supports_instancing = gles_frame
        .capabilities()
        .contains(&smithay::backend::renderer::gles::Capability::Instancing);
    let debug = !gles_frame.debug_flags().is_empty();
    let projection_matrix = glam::Mat3::from_cols_array(gles_frame.projection());

    // Update the blur buffers.
    // We use gl ffi directly to circumvent some stuff done by smithay
    let blurred_texture = gles_frame.with_context(|gl| unsafe {
        super::get_main_buffer_blur(
            gl,
            &mut *fx_buffers,
            &shaders,
            config.clone(),
            projection_matrix,
            scale as i32,
            &vbos,
            debug,
            supports_instancing,
            dst,
            is_tty,
        )
    })??;

    let (program, additional_uniforms) = if corner_radius == 0.0 {
        (None, vec![])
    } else {
        let program = Shaders::get_from_frame(gles_frame).blur_finish.clone();
        (
            program,
            vec![
                Uniform::new(
                    "geo",
                    [
                        dst.loc.x as f32,
                        dst.loc.y as f32,
                        dst.size.w as f32,
                        dst.size.h as f32,
                    ],
                ),
                Uniform::new("alpha", alpha),
                Uniform::new("noise", config.noise.0 as f32),
                Uniform::new("corner_radius", corner_radius),
            ],
        )
    };

    gles_frame.render_texture_from_to(
        &blurred_texture,
        src,
        dst,
        damage,
        opaque_regions,
        Transform::Normal,
        alpha,
        program.as_ref(),
        &additional_uniforms,
    )
}

impl RenderElement<GlesRenderer> for BlurRenderElement {
    fn draw(
        &self,
        gles_frame: &mut GlesFrame,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        match self {
            Self::Optimized {
                tex,
                corner_radius,
                noise,
                scale,
            } => {
                let downscaled_dst = Rectangle::new(
                    dst.loc,
                    Size::from((
                        (dst.size.w as f64 / *scale) as i32,
                        (dst.size.h as f64 / *scale) as i32,
                    )),
                );

                if *corner_radius == 0.0 {
                    <OptimizedBlurTextureElement as RenderElement<GlesRenderer>>::draw(
                        tex,
                        gles_frame,
                        src,
                        downscaled_dst,
                        damage,
                        opaque_regions,
                    )
                } else {
                    let program = Shaders::get_from_frame(gles_frame).blur_finish.clone();
                    let gles_frame: &mut GlesFrame = gles_frame;
                    gles_frame.override_default_tex_program(
                        program.unwrap(),
                        vec![
                            Uniform::new(
                                "geo",
                                [
                                    dst.loc.x as f32,
                                    dst.loc.y as f32,
                                    dst.size.w as f32,
                                    dst.size.h as f32,
                                ],
                            ),
                            Uniform::new("corner_radius", *corner_radius),
                            Uniform::new("noise", *noise),
                            Uniform::new("alpha", self.alpha()),
                        ],
                    );

                    let res =
                        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(
                            &tex.0,
                            gles_frame,
                            src,
                            downscaled_dst,
                            damage,
                            opaque_regions,
                        );

                    gles_frame.clear_tex_program_override();

                    res
                }
            }
            Self::TrueBlur {
                output,
                scale,
                corner_radius,
                config,
                ..
            } => draw_true_blur(
                output,
                gles_frame,
                config,
                *scale,
                dst,
                *corner_radius,
                src,
                damage,
                opaque_regions,
                self.alpha(),
                false,
            ),
        }
    }

    fn underlying_storage(&self, _: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for BlurRenderElement {
    fn draw(
        &self,
        frame: &mut TtyFrame<'_, '_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        match self {
            Self::Optimized { .. } => {
                <BlurRenderElement as RenderElement<GlesRenderer>>::draw(
                    self,
                    frame.as_gles_frame(),
                    src,
                    dst,
                    damage,
                    opaque_regions,
                )?;
            }

            Self::TrueBlur {
                output,
                scale,
                corner_radius,
                config,
                ..
            } => {
                draw_true_blur(
                    output,
                    frame.as_gles_frame(),
                    config,
                    *scale,
                    dst,
                    *corner_radius,
                    src,
                    damage,
                    opaque_regions,
                    self.alpha(),
                    true,
                )?;
            }
        }

        Ok(())
    }

    fn underlying_storage(
        &self,
        _renderer: &mut TtyRenderer<'render>,
    ) -> Option<UnderlyingStorage> {
        None
    }
}
