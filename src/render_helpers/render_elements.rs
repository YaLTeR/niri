// We need to implement RenderElement manually due to AsGlesFrame requirement.
// This macro does it for us.
#[macro_export]
macro_rules! niri_render_elements {
    ($name:ident => { $($variant:ident = $type:ty),+ $(,)? }) => {
        #[derive(Debug)]
        pub enum $name<R: $crate::render_helpers::renderer::NiriRenderer> {
            $($variant($type)),+
        }

        impl<R: $crate::render_helpers::renderer::NiriRenderer> smithay::backend::renderer::element::Element for $name<R> {
            fn id(&self) -> &smithay::backend::renderer::element::Id {
                match self {
                    $($name::$variant(elem) => elem.id()),+
                }
            }

            fn current_commit(&self) -> smithay::backend::renderer::utils::CommitCounter {
                match self {
                    $($name::$variant(elem) => elem.current_commit()),+
                }
            }

            fn geometry(&self, scale: smithay::utils::Scale<f64>) -> Rectangle<i32, smithay::utils::Physical> {
                match self {
                    $($name::$variant(elem) => elem.geometry(scale)),+
                }
            }

            fn transform(&self) -> smithay::utils::Transform {
                match self {
                    $($name::$variant(elem) => elem.transform()),+
                }
            }

            fn src(&self) -> smithay::utils::Rectangle<f64, smithay::utils::Buffer> {
                match self {
                    $($name::$variant(elem) => elem.src()),+
                }
            }

            fn damage_since(
                &self,
                scale: smithay::utils::Scale<f64>,
                commit: Option<smithay::backend::renderer::utils::CommitCounter>,
            ) -> Vec<smithay::utils::Rectangle<i32, smithay::utils::Physical>> {
                match self {
                    $($name::$variant(elem) => elem.damage_since(scale, commit)),+
                }
            }

            fn opaque_regions(&self, scale: smithay::utils::Scale<f64>) -> Vec<smithay::utils::Rectangle<i32, smithay::utils::Physical>> {
                match self {
                    $($name::$variant(elem) => elem.opaque_regions(scale)),+
                }
            }

            fn alpha(&self) -> f32 {
                match self {
                    $($name::$variant(elem) => elem.alpha()),+
                }
            }

            fn kind(&self) -> smithay::backend::renderer::element::Kind {
                match self {
                    $($name::$variant(elem) => elem.kind()),+
                }
            }
        }

        impl smithay::backend::renderer::element::RenderElement<smithay::backend::renderer::gles::GlesRenderer> for $name<smithay::backend::renderer::gles::GlesRenderer> {
            fn draw(
                &self,
                frame: &mut smithay::backend::renderer::gles::GlesFrame<'_>,
                src: smithay::utils::Rectangle<f64, smithay::utils::Buffer>,
                dst: smithay::utils::Rectangle<i32, smithay::utils::Physical>,
                damage: &[smithay::utils::Rectangle<i32, smithay::utils::Physical>],
            ) -> Result<(), smithay::backend::renderer::gles::GlesError> {
                match self {
                    $($name::$variant(elem) => {
                        smithay::backend::renderer::element::RenderElement::<smithay::backend::renderer::gles::GlesRenderer>::draw(elem, frame, src, dst, damage)
                    })+
                }
            }

            fn underlying_storage(&self, renderer: &mut smithay::backend::renderer::gles::GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage> {
                match self {
                    $($name::$variant(elem) => elem.underlying_storage(renderer)),+
                }
            }
        }

        impl<'render, 'alloc> smithay::backend::renderer::element::RenderElement<$crate::backend::tty::TtyRenderer<'render, 'alloc>>
            for $name<$crate::backend::tty::TtyRenderer<'render, 'alloc>>
        {
            fn draw(
                &self,
                frame: &mut $crate::backend::tty::TtyFrame<'render, 'alloc, '_>,
                src: smithay::utils::Rectangle<f64, smithay::utils::Buffer>,
                dst: smithay::utils::Rectangle<i32, smithay::utils::Physical>,
                damage: &[smithay::utils::Rectangle<i32, smithay::utils::Physical>],
            ) -> Result<(), $crate::backend::tty::TtyRendererError<'render, 'alloc>> {
                match self {
                    $($name::$variant(elem) => {
                        smithay::backend::renderer::element::RenderElement::<$crate::backend::tty::TtyRenderer<'render, 'alloc>>::draw(elem, frame, src, dst, damage)
                    })+
                }
            }

            fn underlying_storage(
                &self,
                renderer: &mut $crate::backend::tty::TtyRenderer<'render, 'alloc>,
            ) -> Option<smithay::backend::renderer::element::UnderlyingStorage> {
                match self {
                    $($name::$variant(elem) => elem.underlying_storage(renderer)),+
                }
            }
        }

        $(impl<R: $crate::render_helpers::renderer::NiriRenderer> From<$type> for $name<R> {
            fn from(x: $type) -> Self {
                Self::$variant(x)
            }
        })+
    };
}
