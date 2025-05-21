// Ported from https://github.com/nferhat/fht-compositor/blob/main/src/renderer/blur/element.rs

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::output::Output;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use crate::backend::tty::{TtyFrame, TtyRenderer, TtyRendererError};
use crate::render_helpers::render_data::RendererData;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::shaders::Shaders;

use super::{CurrentBuffer, EffectsFramebuffers};

#[derive(Clone, Debug)]
pub struct BlurConfig {
    pub passes: u32,
    pub radius: f32,
    pub noise: f32,
}

const BLUR_CONFIG: BlurConfig = BlurConfig {
    passes: 3,
    radius: 5.,
    noise: 0.,
};

#[derive(Debug)]
pub enum BlurRenderElement {
    /// Use true blur.
    ///
    /// When using this technique, the compositor will blur the current framebuffer ccontents that
    /// are below the [`BlurElement`] in order to display them. This adds an additional render step
    /// but provides true results with the blurred contents.
    TrueBlur {
        // we are just a funny texture element that generates the texture on RenderElement::draw
        id: Id,
        scale: i32,
        transform: Transform,
        src: Rectangle<f64, Logical>,
        size: Size<i32, Logical>,
        corner_radius: f32,
        loc: Point<i32, Physical>,
        output: Output,
        alpha: f32,
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
        scale: i32,
        alpha: f32,
    ) -> Self {
        debug!("BlurRenderElement::new({:?}, {:?})", loc, sample_area);

        Self::TrueBlur {
            id: Id::new(),
            scale,
            src: sample_area.to_f64(),
            transform: Transform::Normal,
            size: sample_area.size,
            corner_radius,
            loc,
            alpha,
            output: output.clone(), // fixme i hate this
            commit_counter: CommitCounter::default(),
        }
    }
}

impl Element for BlurRenderElement {
    fn id(&self) -> &Id {
        match self {
            BlurRenderElement::TrueBlur { id, .. } => id,
        }
    }

    fn current_commit(&self) -> CommitCounter {
        match self {
            BlurRenderElement::TrueBlur { commit_counter, .. } => *commit_counter,
        }
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        match self {
            BlurRenderElement::TrueBlur { loc, .. } => *loc,
        }
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        match self {
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
            BlurRenderElement::TrueBlur { transform, .. } => *transform,
        }
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        match self {
            BlurRenderElement::TrueBlur { .. } => {
                // Since the blur element samples from around itself, we must expand the damage it
                // induces to include any potential changes.
                let mut geometry = Rectangle::from_size(self.geometry(scale).size);
                let size =
                    (2f32.powi(BLUR_CONFIG.passes as i32 + 1) * BLUR_CONFIG.radius).ceil() as i32;
                geometry.loc -= Point::from((size, size));
                geometry.size += Size::from((size, size)).upscale(2);

                // FIXME: Damage tracking?
                DamageSet::from_slice(&[geometry])
            }
        }
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        match self {
            BlurRenderElement::TrueBlur { .. } => {
                // Since we are rendering as true blur, we will draw whatever is behind the window
                OpaqueRegions::default()
            }
        }
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        match self {
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
            Self::TrueBlur {
                output,
                scale,
                corner_radius,
                alpha,
                ..
            } => {
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
                        BLUR_CONFIG,
                        projection_matrix,
                        *scale,
                        &vbos,
                        debug,
                        supports_instancing,
                        dst,
                    )
                })??;

                //@todo Handle noise and corner radiusS
                // let (program, additional_uniforms) = if *corner_radius == 0.0 {
                //     (None, vec![])
                // } else {
                //     let program = Shaders::get_from_frame(gles_frame).blur_finish.clone();
                //     (
                //         program,
                //         vec![
                //             Uniform::new(
                //                 "geo",
                //                 [
                //                     dst.loc.x as f32,
                //                     dst.loc.y as f32,
                //                     dst.size.w as f32,
                //                     dst.size.h as f32,
                //                 ],
                //             ),
                //             Uniform::new("corner_radius", *corner_radius),
                //             Uniform::new("noise", BLUR_CONFIG.noise),
                //         ],
                //     )
                // };

                gles_frame.render_texture_from_to(
                    &blurred_texture,
                    src,
                    dst,
                    damage,
                    opaque_regions,
                    Transform::Normal,
                    1.,
                    None,
                    &[], // program.as_ref(),
                         // &additional_uniforms,
                )
            }
        }
    }

    fn underlying_storage(&self, _: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

impl<'render> RenderElement<TtyRenderer<'render>> for BlurRenderElement {
    fn draw(
        &self,
        _frame: &mut TtyFrame<'_, '_, '_>,
        _src: Rectangle<f64, Buffer>,
        _dst: Rectangle<i32, Physical>,
        _damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), TtyRendererError<'render>> {
        Ok(())
    }

    fn underlying_storage(&self, renderer: &mut TtyRenderer<'render>) -> Option<UnderlyingStorage> {
        None
    }
}
