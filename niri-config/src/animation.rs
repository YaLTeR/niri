// Animation types and implementations for niri configuration

use knuffel::errors::DecodeError;
use knuffel::Decode;

use crate::core::FloatOrInt;

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Animations {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = FloatOrInt(1.))]
    pub slowdown: FloatOrInt<0, { i32::MAX }>,
    #[knuffel(child, default)]
    pub workspace_switch: WorkspaceSwitchAnim,
    #[knuffel(child, default)]
    pub window_open: WindowOpenAnim,
    #[knuffel(child, default)]
    pub window_close: WindowCloseAnim,
    #[knuffel(child, default)]
    pub horizontal_view_movement: HorizontalViewMovementAnim,
    #[knuffel(child, default)]
    pub window_movement: WindowMovementAnim,
    #[knuffel(child, default)]
    pub window_resize: WindowResizeAnim,
    #[knuffel(child, default)]
    pub config_notification_open_close: ConfigNotificationOpenCloseAnim,
    #[knuffel(child, default)]
    pub screenshot_ui_open: ScreenshotUiOpenAnim,
    #[knuffel(child, default)]
    pub overview_open_close: OverviewOpenCloseAnim,
}

impl Default for Animations {
    fn default() -> Self {
        Self {
            off: false,
            slowdown: FloatOrInt(1.),
            workspace_switch: Default::default(),
            horizontal_view_movement: Default::default(),
            window_movement: Default::default(),
            window_open: Default::default(),
            window_close: Default::default(),
            window_resize: Default::default(),
            config_notification_open_close: Default::default(),
            screenshot_ui_open: Default::default(),
            overview_open_close: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkspaceSwitchAnim(pub Animation);

impl Default for WorkspaceSwitchAnim {
    fn default() -> Self {
        Self(Animation {
            off: false,
            kind: AnimationKind::Spring(SpringParams {
                damping_ratio: 1.,
                stiffness: 1000,
                epsilon: 0.0001,
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowOpenAnim {
    pub anim: Animation,
    pub custom_shader: Option<String>,
}

impl Default for WindowOpenAnim {
    fn default() -> Self {
        Self {
            anim: Animation {
                off: false,
                kind: AnimationKind::Easing(EasingParams {
                    duration_ms: 150,
                    curve: AnimationCurve::EaseOutExpo,
                }),
            },
            custom_shader: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowCloseAnim {
    pub anim: Animation,
    pub custom_shader: Option<String>,
}

impl Default for WindowCloseAnim {
    fn default() -> Self {
        Self {
            anim: Animation {
                off: false,
                kind: AnimationKind::Easing(EasingParams {
                    duration_ms: 150,
                    curve: AnimationCurve::EaseOutQuad,
                }),
            },
            custom_shader: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorizontalViewMovementAnim(pub Animation);

impl Default for HorizontalViewMovementAnim {
    fn default() -> Self {
        Self(Animation {
            off: false,
            kind: AnimationKind::Spring(SpringParams {
                damping_ratio: 1.,
                stiffness: 800,
                epsilon: 0.0001,
            }),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowMovementAnim(pub Animation);

impl Default for WindowMovementAnim {
    fn default() -> Self {
        Self(Animation {
            off: false,
            kind: AnimationKind::Spring(SpringParams {
                damping_ratio: 1.,
                stiffness: 800,
                epsilon: 0.0001,
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowResizeAnim {
    pub anim: Animation,
    pub custom_shader: Option<String>,
}

impl Default for WindowResizeAnim {
    fn default() -> Self {
        Self {
            anim: Animation {
                off: false,
                kind: AnimationKind::Spring(SpringParams {
                    damping_ratio: 1.,
                    stiffness: 800,
                    epsilon: 0.0001,
                }),
            },
            custom_shader: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfigNotificationOpenCloseAnim(pub Animation);

impl Default for ConfigNotificationOpenCloseAnim {
    fn default() -> Self {
        Self(Animation {
            off: false,
            kind: AnimationKind::Spring(SpringParams {
                damping_ratio: 0.6,
                stiffness: 1000,
                epsilon: 0.001,
            }),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenshotUiOpenAnim(pub Animation);

impl Default for ScreenshotUiOpenAnim {
    fn default() -> Self {
        Self(Animation {
            off: false,
            kind: AnimationKind::Easing(EasingParams {
                duration_ms: 200,
                curve: AnimationCurve::EaseOutQuad,
            }),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverviewOpenCloseAnim(pub Animation);

impl Default for OverviewOpenCloseAnim {
    fn default() -> Self {
        Self(Animation {
            off: false,
            kind: AnimationKind::Spring(SpringParams {
                damping_ratio: 1.,
                stiffness: 800,
                epsilon: 0.0001,
            }),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Animation {
    pub off: bool,
    pub kind: AnimationKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationKind {
    Easing(EasingParams),
    Spring(SpringParams),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EasingParams {
    pub duration_ms: u32,
    pub curve: AnimationCurve,
}

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq)]
pub enum AnimationCurve {
    Linear,
    EaseOutQuad,
    EaseOutCubic,
    EaseOutExpo,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringParams {
    pub damping_ratio: f64,
    pub stiffness: u32,
    pub epsilon: f64,
}

// Helper functions for animation parsing

fn expect_only_children<S>(
    node: &knuffel::ast::SpannedNode<S>,
    ctx: &mut knuffel::decode::Context<S>,
) where
    S: knuffel::traits::ErrorSpan,
{
    if let Some(type_name) = &node.type_name {
        ctx.emit_error(DecodeError::unexpected(
            type_name,
            "type name",
            "no type name expected for this node",
        ));
    }

    for val in node.arguments.iter() {
        ctx.emit_error(DecodeError::unexpected(
            &val.literal,
            "argument",
            "no arguments expected for this node",
        ))
    }

    for name in node.properties.keys() {
        ctx.emit_error(DecodeError::unexpected(
            name,
            "property",
            "no properties expected for this node",
        ))
    }
}

fn parse_arg_node<S: knuffel::traits::ErrorSpan, T: knuffel::traits::DecodeScalar<S>>(
    name: &str,
    node: &knuffel::ast::SpannedNode<S>,
    ctx: &mut knuffel::decode::Context<S>,
) -> Result<T, DecodeError<S>> {
    let mut iter_args = node.arguments.iter();
    let val = iter_args.next().ok_or_else(|| {
        DecodeError::missing(node, format!("additional argument `{name}` is required"))
    })?;

    let value = knuffel::traits::DecodeScalar::decode(val, ctx)?;

    if let Some(val) = iter_args.next() {
        ctx.emit_error(DecodeError::unexpected(
            &val.literal,
            "argument",
            "unexpected argument",
        ));
    }
    for name in node.properties.keys() {
        ctx.emit_error(DecodeError::unexpected(
            name,
            "property",
            format!("unexpected property `{}`", name.escape_default()),
        ));
    }
    for child in node.children() {
        ctx.emit_error(DecodeError::unexpected(
            child,
            "node",
            format!("unexpected node `{}`", child.node_name.escape_default()),
        ));
    }

    Ok(value)
}

// knuffel::Decode implementations for animation types

impl<S> knuffel::Decode<S> for WorkspaceSwitchAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().0;
        Ok(Self(Animation::decode_node(node, ctx, default, |_, _| {
            Ok(false)
        })?))
    }
}

impl<S> knuffel::Decode<S> for HorizontalViewMovementAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().0;
        Ok(Self(Animation::decode_node(node, ctx, default, |_, _| {
            Ok(false)
        })?))
    }
}

impl<S> knuffel::Decode<S> for WindowMovementAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().0;
        Ok(Self(Animation::decode_node(node, ctx, default, |_, _| {
            Ok(false)
        })?))
    }
}

impl<S> knuffel::Decode<S> for WindowOpenAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().anim;
        let mut custom_shader = None;
        let anim = Animation::decode_node(node, ctx, default, |child, ctx| {
            if &**child.node_name == "custom-shader" {
                custom_shader = parse_arg_node("custom-shader", child, ctx)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })?;

        Ok(Self {
            anim,
            custom_shader,
        })
    }
}

impl<S> knuffel::Decode<S> for WindowCloseAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().anim;
        let mut custom_shader = None;
        let anim = Animation::decode_node(node, ctx, default, |child, ctx| {
            if &**child.node_name == "custom-shader" {
                custom_shader = parse_arg_node("custom-shader", child, ctx)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })?;

        Ok(Self {
            anim,
            custom_shader,
        })
    }
}

impl<S> knuffel::Decode<S> for WindowResizeAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().anim;
        let mut custom_shader = None;
        let anim = Animation::decode_node(node, ctx, default, |child, ctx| {
            if &**child.node_name == "custom-shader" {
                custom_shader = parse_arg_node("custom-shader", child, ctx)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })?;

        Ok(Self {
            anim,
            custom_shader,
        })
    }
}

impl<S> knuffel::Decode<S> for ConfigNotificationOpenCloseAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().0;
        Ok(Self(Animation::decode_node(node, ctx, default, |_, _| {
            Ok(false)
        })?))
    }
}

impl<S> knuffel::Decode<S> for ScreenshotUiOpenAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().0;
        Ok(Self(Animation::decode_node(node, ctx, default, |_, _| {
            Ok(false)
        })?))
    }
}

impl<S> knuffel::Decode<S> for OverviewOpenCloseAnim
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let default = Self::default().0;
        Ok(Self(Animation::decode_node(node, ctx, default, |_, _| {
            Ok(false)
        })?))
    }
}

impl Animation {
    pub fn new_off() -> Self {
        Self {
            off: true,
            kind: AnimationKind::Easing(EasingParams {
                duration_ms: 0,
                curve: AnimationCurve::Linear,
            }),
        }
    }

    fn decode_node<S: knuffel::traits::ErrorSpan>(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
        default: Self,
        mut process_children: impl FnMut(
            &knuffel::ast::SpannedNode<S>,
            &mut knuffel::decode::Context<S>,
        ) -> Result<bool, DecodeError<S>>,
    ) -> Result<Self, DecodeError<S>> {
        #[derive(Default, PartialEq)]
        struct OptionalEasingParams {
            duration_ms: Option<u32>,
            curve: Option<AnimationCurve>,
        }

        expect_only_children(node, ctx);

        let mut off = false;
        let mut easing_params = OptionalEasingParams::default();
        let mut spring_params = None;

        for child in node.children() {
            match &**child.node_name {
                "off" => {
                    knuffel::decode::check_flag_node(child, ctx);
                    if off {
                        ctx.emit_error(DecodeError::unexpected(
                            &child.node_name,
                            "node",
                            "duplicate node `off`, single node expected",
                        ));
                    } else {
                        off = true;
                    }
                }
                "spring" => {
                    if easing_params != OptionalEasingParams::default() {
                        ctx.emit_error(DecodeError::unexpected(
                            child,
                            "node",
                            "cannot set both spring and easing parameters at once",
                        ));
                    }
                    if spring_params.is_some() {
                        ctx.emit_error(DecodeError::unexpected(
                            &child.node_name,
                            "node",
                            "duplicate node `spring`, single node expected",
                        ));
                    }

                    spring_params = Some(SpringParams::decode_node(child, ctx)?);
                }
                "duration-ms" => {
                    if spring_params.is_some() {
                        ctx.emit_error(DecodeError::unexpected(
                            child,
                            "node",
                            "cannot set both spring and easing parameters at once",
                        ));
                    }
                    if easing_params.duration_ms.is_some() {
                        ctx.emit_error(DecodeError::unexpected(
                            &child.node_name,
                            "node",
                            "duplicate node `duration-ms`, single node expected",
                        ));
                    }

                    easing_params.duration_ms = Some(parse_arg_node("duration-ms", child, ctx)?);
                }
                "curve" => {
                    if spring_params.is_some() {
                        ctx.emit_error(DecodeError::unexpected(
                            child,
                            "node",
                            "cannot set both spring and easing parameters at once",
                        ));
                    }
                    if easing_params.curve.is_some() {
                        ctx.emit_error(DecodeError::unexpected(
                            &child.node_name,
                            "node",
                            "duplicate node `curve`, single node expected",
                        ));
                    }

                    easing_params.curve = Some(parse_arg_node("curve", child, ctx)?);
                }
                name_str => {
                    if !process_children(child, ctx)? {
                        ctx.emit_error(DecodeError::unexpected(
                            child,
                            "node",
                            format!("unexpected node `{}`", name_str.escape_default()),
                        ));
                    }
                }
            }
        }

        let kind = if let Some(spring_params) = spring_params {
            // Configured spring.
            AnimationKind::Spring(spring_params)
        } else if easing_params == OptionalEasingParams::default() {
            // Did not configure anything.
            default.kind
        } else {
            // Configured easing.
            let default = if let AnimationKind::Easing(easing) = default.kind {
                easing
            } else {
                // Generic fallback values for when the default animation is spring, but the user
                // configured an easing animation.
                EasingParams {
                    duration_ms: 250,
                    curve: AnimationCurve::EaseOutCubic,
                }
            };

            AnimationKind::Easing(EasingParams {
                duration_ms: easing_params.duration_ms.unwrap_or(default.duration_ms),
                curve: easing_params.curve.unwrap_or(default.curve),
            })
        };

        Ok(Self { off, kind })
    }
}

impl<S> knuffel::Decode<S> for SpringParams
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        if let Some(type_name) = &node.type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }
        if let Some(val) = node.arguments.first() {
            ctx.emit_error(DecodeError::unexpected(
                &val.literal,
                "argument",
                "unexpected argument",
            ));
        }
        for child in node.children() {
            ctx.emit_error(DecodeError::unexpected(
                child,
                "node",
                format!("unexpected node `{}`", child.node_name.escape_default()),
            ));
        }

        let mut damping_ratio = None;
        let mut stiffness = None;
        let mut epsilon = None;
        for (name, val) in &node.properties {
            match &***name {
                "damping-ratio" => {
                    damping_ratio = Some(knuffel::traits::DecodeScalar::decode(val, ctx)?);
                }
                "stiffness" => {
                    stiffness = Some(knuffel::traits::DecodeScalar::decode(val, ctx)?);
                }
                "epsilon" => {
                    epsilon = Some(knuffel::traits::DecodeScalar::decode(val, ctx)?);
                }
                name_str => {
                    ctx.emit_error(DecodeError::unexpected(
                        name,
                        "property",
                        format!("unexpected property `{}`", name_str.escape_default()),
                    ));
                }
            }
        }
        let damping_ratio = damping_ratio
            .ok_or_else(|| DecodeError::missing(node, "property `damping-ratio` is required"))?;
        let stiffness = stiffness
            .ok_or_else(|| DecodeError::missing(node, "property `stiffness` is required"))?;
        let epsilon =
            epsilon.ok_or_else(|| DecodeError::missing(node, "property `epsilon` is required"))?;

        if !(0.1..=10.).contains(&damping_ratio) {
            ctx.emit_error(DecodeError::conversion(
                node,
                "damping-ratio must be between 0.1 and 10.0",
            ));
        }
        if stiffness < 1 {
            ctx.emit_error(DecodeError::conversion(node, "stiffness must be >= 1"));
        }
        if !(0.00001..=0.1).contains(&epsilon) {
            ctx.emit_error(DecodeError::conversion(
                node,
                "epsilon must be between 0.00001 and 0.1",
            ));
        }

        Ok(SpringParams {
            damping_ratio,
            stiffness,
            epsilon,
        })
    }
}
