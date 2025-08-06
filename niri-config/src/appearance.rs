use std::ops::{Mul, MulAssign};
use std::str::FromStr;

use knuffel::errors::DecodeError;
use miette::{miette, IntoDiagnostic};
use smithay::backend::renderer::Color32F;

use crate::core::FloatOrInt;

pub const DEFAULT_BACKGROUND_COLOR: Color = Color::from_array_unpremul([0.25, 0.25, 0.25, 1.]);
pub const DEFAULT_BACKDROP_COLOR: Color = Color::from_array_unpremul([0.15, 0.15, 0.15, 1.]);

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct FocusRing {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = Self::default().width)]
    pub width: FloatOrInt<0, 65535>,
    #[knuffel(child, default = Self::default().active_color)]
    pub active_color: Color,
    #[knuffel(child, default = Self::default().inactive_color)]
    pub inactive_color: Color,
    #[knuffel(child, default = Self::default().urgent_color)]
    pub urgent_color: Color,
    #[knuffel(child)]
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub urgent_gradient: Option<Gradient>,
}

impl Default for FocusRing {
    fn default() -> Self {
        Self {
            off: false,
            width: FloatOrInt(4.),
            active_color: Color::from_rgba8_unpremul(127, 200, 255, 255),
            inactive_color: Color::from_rgba8_unpremul(80, 80, 80, 255),
            urgent_color: Color::from_rgba8_unpremul(155, 0, 0, 255),
            active_gradient: None,
            inactive_gradient: None,
            urgent_gradient: None,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Gradient {
    #[knuffel(property, str)]
    pub from: Color,
    #[knuffel(property, str)]
    pub to: Color,
    #[knuffel(property, default = 180)]
    pub angle: i16,
    #[knuffel(property, default)]
    pub relative_to: GradientRelativeTo,
    #[knuffel(property(name = "in"), str, default)]
    pub in_: GradientInterpolation,
}

impl From<Color> for Gradient {
    fn from(value: Color) -> Self {
        Self {
            from: value,
            to: value,
            angle: 0,
            relative_to: GradientRelativeTo::Window,
            in_: GradientInterpolation::default(),
        }
    }
}

#[derive(knuffel::DecodeScalar, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GradientRelativeTo {
    #[default]
    Window,
    WorkspaceView,
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct GradientInterpolation {
    pub color_space: GradientColorSpace,
    pub hue_interpolation: HueInterpolation,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GradientColorSpace {
    #[default]
    Srgb,
    SrgbLinear,
    Oklab,
    Oklch,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum HueInterpolation {
    #[default]
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Border {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = Self::default().width)]
    pub width: FloatOrInt<0, 65535>,
    #[knuffel(child, default = Self::default().active_color)]
    pub active_color: Color,
    #[knuffel(child, default = Self::default().inactive_color)]
    pub inactive_color: Color,
    #[knuffel(child, default = Self::default().urgent_color)]
    pub urgent_color: Color,
    #[knuffel(child)]
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub urgent_gradient: Option<Gradient>,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            off: true,
            width: FloatOrInt(4.),
            active_color: Color::from_rgba8_unpremul(255, 200, 127, 255),
            inactive_color: Color::from_rgba8_unpremul(80, 80, 80, 255),
            urgent_color: Color::from_rgba8_unpremul(155, 0, 0, 255),
            active_gradient: None,
            inactive_gradient: None,
            urgent_gradient: None,
        }
    }
}

impl From<Border> for FocusRing {
    fn from(value: Border) -> Self {
        Self {
            off: value.off,
            width: value.width,
            active_color: value.active_color,
            inactive_color: value.inactive_color,
            urgent_color: value.urgent_color,
            active_gradient: value.active_gradient,
            inactive_gradient: value.inactive_gradient,
            urgent_gradient: value.urgent_gradient,
        }
    }
}

impl From<FocusRing> for Border {
    fn from(value: FocusRing) -> Self {
        Self {
            off: value.off,
            width: value.width,
            active_color: value.active_color,
            inactive_color: value.inactive_color,
            urgent_color: value.urgent_color,
            active_gradient: value.active_gradient,
            inactive_gradient: value.inactive_gradient,
            urgent_gradient: value.urgent_gradient,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    #[knuffel(child)]
    pub on: bool,
    #[knuffel(child, default = Self::default().offset)]
    pub offset: ShadowOffset,
    #[knuffel(child, unwrap(argument), default = Self::default().softness)]
    pub softness: FloatOrInt<0, 1024>,
    #[knuffel(child, unwrap(argument), default = Self::default().spread)]
    pub spread: FloatOrInt<-1024, 1024>,
    #[knuffel(child, unwrap(argument), default = Self::default().draw_behind_window)]
    pub draw_behind_window: bool,
    #[knuffel(child, default = Self::default().color)]
    pub color: Color,
    #[knuffel(child)]
    pub inactive_color: Option<Color>,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            on: false,
            offset: ShadowOffset {
                x: FloatOrInt(0.),
                y: FloatOrInt(5.),
            },
            softness: FloatOrInt(30.),
            spread: FloatOrInt(5.),
            draw_behind_window: false,
            color: Color::from_rgba8_unpremul(0, 0, 0, 0x70),
            inactive_color: None,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct ShadowOffset {
    #[knuffel(property, default)]
    pub x: FloatOrInt<-65535, 65535>,
    #[knuffel(property, default)]
    pub y: FloatOrInt<-65535, 65535>,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct WorkspaceShadow {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, default = Self::default().offset)]
    pub offset: ShadowOffset,
    #[knuffel(child, unwrap(argument), default = Self::default().softness)]
    pub softness: FloatOrInt<0, 1024>,
    #[knuffel(child, unwrap(argument), default = Self::default().spread)]
    pub spread: FloatOrInt<-1024, 1024>,
    #[knuffel(child, default = Self::default().color)]
    pub color: Color,
}

impl Default for WorkspaceShadow {
    fn default() -> Self {
        Self {
            off: false,
            offset: ShadowOffset {
                x: FloatOrInt(0.),
                y: FloatOrInt(10.),
            },
            softness: FloatOrInt(40.),
            spread: FloatOrInt(10.),
            color: Color::from_rgba8_unpremul(0, 0, 0, 0x50),
        }
    }
}

impl From<WorkspaceShadow> for Shadow {
    fn from(value: WorkspaceShadow) -> Self {
        Self {
            on: !value.off,
            offset: value.offset,
            softness: value.softness,
            spread: value.spread,
            draw_behind_window: false,
            color: value.color,
            inactive_color: None,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct TabIndicator {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub hide_when_single_tab: bool,
    #[knuffel(child)]
    pub place_within_column: bool,
    #[knuffel(child, unwrap(argument), default = Self::default().gap)]
    pub gap: FloatOrInt<-65535, 65535>,
    #[knuffel(child, unwrap(argument), default = Self::default().width)]
    pub width: FloatOrInt<0, 65535>,
    #[knuffel(child, default = Self::default().length)]
    pub length: TabIndicatorLength,
    #[knuffel(child, unwrap(argument), default = Self::default().position)]
    pub position: TabIndicatorPosition,
    #[knuffel(child, unwrap(argument), default = Self::default().gaps_between_tabs)]
    pub gaps_between_tabs: FloatOrInt<0, 65535>,
    #[knuffel(child, unwrap(argument), default = Self::default().corner_radius)]
    pub corner_radius: FloatOrInt<0, 65535>,
    #[knuffel(child)]
    pub active_color: Option<Color>,
    #[knuffel(child)]
    pub inactive_color: Option<Color>,
    #[knuffel(child)]
    pub urgent_color: Option<Color>,
    #[knuffel(child)]
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub urgent_gradient: Option<Gradient>,
}

impl Default for TabIndicator {
    fn default() -> Self {
        Self {
            off: false,
            hide_when_single_tab: false,
            place_within_column: false,
            gap: FloatOrInt(5.),
            width: FloatOrInt(4.),
            length: TabIndicatorLength {
                total_proportion: Some(0.5),
            },
            position: TabIndicatorPosition::Left,
            gaps_between_tabs: FloatOrInt(0.),
            corner_radius: FloatOrInt(0.),
            active_color: None,
            inactive_color: None,
            urgent_color: None,
            active_gradient: None,
            inactive_gradient: None,
            urgent_gradient: None,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct TabIndicatorLength {
    #[knuffel(property)]
    pub total_proportion: Option<f64>,
}

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq)]
pub enum TabIndicatorPosition {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct InsertHint {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, default = Self::default().color)]
    pub color: Color,
    #[knuffel(child)]
    pub gradient: Option<Gradient>,
}

impl Default for InsertHint {
    fn default() -> Self {
        Self {
            off: false,
            color: Color::from_rgba8_unpremul(127, 200, 255, 128),
            gradient: None,
        }
    }
}

/// RGB color in [0, 1] with unpremultiplied alpha.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new_unpremul(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_rgba8_unpremul(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::from_array_unpremul([r, g, b, a].map(|x| x as f32 / 255.))
    }

    pub fn from_array_premul([r, g, b, a]: [f32; 4]) -> Self {
        let a = a.clamp(0., 1.);

        if a == 0. {
            Self::new_unpremul(0., 0., 0., 0.)
        } else {
            Self {
                r: (r / a).clamp(0., 1.),
                g: (g / a).clamp(0., 1.),
                b: (b / a).clamp(0., 1.),
                a,
            }
        }
    }

    pub const fn from_array_unpremul([r, g, b, a]: [f32; 4]) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_color32f(color: Color32F) -> Self {
        Self::from_array_premul(color.components())
    }

    pub fn to_array_unpremul(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_array_premul(self) -> [f32; 4] {
        let [r, g, b, a] = [self.r, self.g, self.b, self.a];
        [r * a, g * a, b * a, a]
    }
}

impl Mul<f32> for Color {
    type Output = Self;

    fn mul(mut self, rhs: f32) -> Self::Output {
        self.a *= rhs;
        self
    }
}

impl MulAssign<f32> for Color {
    fn mul_assign(&mut self, rhs: f32) {
        self.a *= rhs;
    }
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
pub struct BorderRule {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub on: bool,
    #[knuffel(child, unwrap(argument))]
    pub width: Option<FloatOrInt<0, 65535>>,
    #[knuffel(child)]
    pub active_color: Option<Color>,
    #[knuffel(child)]
    pub inactive_color: Option<Color>,
    #[knuffel(child)]
    pub urgent_color: Option<Color>,
    #[knuffel(child)]
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub urgent_gradient: Option<Gradient>,
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
pub struct ShadowRule {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub on: bool,
    #[knuffel(child)]
    pub offset: Option<ShadowOffset>,
    #[knuffel(child, unwrap(argument))]
    pub softness: Option<FloatOrInt<0, 1024>>,
    #[knuffel(child, unwrap(argument))]
    pub spread: Option<FloatOrInt<-1024, 1024>>,
    #[knuffel(child, unwrap(argument))]
    pub draw_behind_window: Option<bool>,
    #[knuffel(child)]
    pub color: Option<Color>,
    #[knuffel(child)]
    pub inactive_color: Option<Color>,
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
pub struct TabIndicatorRule {
    #[knuffel(child)]
    pub active_color: Option<Color>,
    #[knuffel(child)]
    pub inactive_color: Option<Color>,
    #[knuffel(child)]
    pub urgent_color: Option<Color>,
    #[knuffel(child)]
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub urgent_gradient: Option<Gradient>,
}

impl BorderRule {
    pub fn merge_with(&mut self, other: &Self) {
        if other.off {
            self.off = true;
            self.on = false;
        }

        if other.on {
            self.off = false;
            self.on = true;
        }

        if let Some(x) = other.width {
            self.width = Some(x);
        }
        if let Some(x) = other.active_color {
            self.active_color = Some(x);
        }
        if let Some(x) = other.inactive_color {
            self.inactive_color = Some(x);
        }
        if let Some(x) = other.urgent_color {
            self.urgent_color = Some(x);
        }
        if let Some(x) = other.active_gradient {
            self.active_gradient = Some(x);
        }
        if let Some(x) = other.inactive_gradient {
            self.inactive_gradient = Some(x);
        }
        if let Some(x) = other.urgent_gradient {
            self.urgent_gradient = Some(x);
        }
    }

    pub fn resolve_against(&self, mut config: Border) -> Border {
        config.off |= self.off;
        if self.on {
            config.off = false;
        }

        if let Some(x) = self.width {
            config.width = x;
        }
        if let Some(x) = self.active_color {
            config.active_color = x;
            config.active_gradient = None;
        }
        if let Some(x) = self.inactive_color {
            config.inactive_color = x;
            config.inactive_gradient = None;
        }
        if let Some(x) = self.urgent_color {
            config.urgent_color = x;
            config.urgent_gradient = None;
        }
        if let Some(x) = self.active_gradient {
            config.active_gradient = Some(x);
        }
        if let Some(x) = self.inactive_gradient {
            config.inactive_gradient = Some(x);
        }
        if let Some(x) = self.urgent_gradient {
            config.urgent_gradient = Some(x);
        }

        config
    }
}

impl ShadowRule {
    pub fn merge_with(&mut self, other: &Self) {
        if other.off {
            self.off = true;
            self.on = false;
        }

        if other.on {
            self.off = false;
            self.on = true;
        }

        if let Some(x) = other.offset {
            self.offset = Some(x);
        }
        if let Some(x) = other.softness {
            self.softness = Some(x);
        }
        if let Some(x) = other.spread {
            self.spread = Some(x);
        }
        if let Some(x) = other.draw_behind_window {
            self.draw_behind_window = Some(x);
        }
        if let Some(x) = other.color {
            self.color = Some(x);
        }
        if let Some(x) = other.inactive_color {
            self.inactive_color = Some(x);
        }
    }

    pub fn resolve_against(&self, mut config: Shadow) -> Shadow {
        config.on |= self.on;
        if self.off {
            config.on = false;
        }

        if let Some(x) = self.offset {
            config.offset = x;
        }
        if let Some(x) = self.softness {
            config.softness = x;
        }
        if let Some(x) = self.spread {
            config.spread = x;
        }
        if let Some(x) = self.draw_behind_window {
            config.draw_behind_window = x;
        }
        if let Some(x) = self.color {
            config.color = x;
        }
        if let Some(x) = self.inactive_color {
            config.inactive_color = Some(x);
        }

        config
    }
}

impl TabIndicatorRule {
    pub fn merge_with(&mut self, other: &Self) {
        if let Some(x) = other.active_color {
            self.active_color = Some(x);
        }
        if let Some(x) = other.inactive_color {
            self.inactive_color = Some(x);
        }
        if let Some(x) = other.urgent_color {
            self.urgent_color = Some(x);
        }
        if let Some(x) = other.active_gradient {
            self.active_gradient = Some(x);
        }
        if let Some(x) = other.inactive_gradient {
            self.inactive_gradient = Some(x);
        }
        if let Some(x) = other.urgent_gradient {
            self.urgent_gradient = Some(x);
        }
    }
}

impl FromStr for GradientInterpolation {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split_whitespace();
        let in_part1 = iter.next();
        let in_part2 = iter.next();
        let in_part3 = iter.next();

        let Some(in_part1) = in_part1 else {
            return Err(miette!("missing color space"));
        };

        let color = match in_part1 {
            "srgb" => GradientColorSpace::Srgb,
            "srgb-linear" => GradientColorSpace::SrgbLinear,
            "oklab" => GradientColorSpace::Oklab,
            "oklch" => GradientColorSpace::Oklch,
            x => {
                return Err(miette!(
                    "invalid color space {x}; can be srgb, srgb-linear, oklab or oklch"
                ))
            }
        };

        let interpolation = if let Some(in_part2) = in_part2 {
            if color != GradientColorSpace::Oklch {
                return Err(miette!("only oklch color space can have hue interpolation"));
            }

            if in_part3 != Some("hue") {
                return Err(miette!(
                    "interpolation must end with \"hue\", like \"oklch shorter hue\""
                ));
            } else if iter.next().is_some() {
                return Err(miette!("unexpected text after hue interpolation"));
            } else {
                match in_part2 {
                    "shorter" => HueInterpolation::Shorter,
                    "longer" => HueInterpolation::Longer,
                    "increasing" => HueInterpolation::Increasing,
                    "decreasing" => HueInterpolation::Decreasing,
                    x => {
                        return Err(miette!(
                            "invalid hue interpolation {x}; \
                             can be shorter, longer, increasing, decreasing"
                        ))
                    }
                }
            }
        } else {
            HueInterpolation::default()
        };

        Ok(Self {
            color_space: color,
            hue_interpolation: interpolation,
        })
    }
}

impl FromStr for Color {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let color = csscolorparser::parse(s)
            .into_diagnostic()?
            .clamp()
            .to_array();
        Ok(Self::from_array_unpremul(color))
    }
}

#[derive(knuffel::Decode)]
struct ColorRgba {
    #[knuffel(argument)]
    r: u8,
    #[knuffel(argument)]
    g: u8,
    #[knuffel(argument)]
    b: u8,
    #[knuffel(argument)]
    a: u8,
}

impl From<ColorRgba> for Color {
    fn from(value: ColorRgba) -> Self {
        let ColorRgba { r, g, b, a } = value;
        Self::from_array_unpremul([r, g, b, a].map(|x| x as f32 / 255.))
    }
}

// Manual impl to allow both one-argument string and 4-argument RGBA forms.
impl<S> knuffel::Decode<S> for Color
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        // Check for unexpected type name.
        if let Some(type_name) = &node.type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }

        // Get the first argument.
        let mut iter_args = node.arguments.iter();
        let val = iter_args
            .next()
            .ok_or_else(|| DecodeError::missing(node, "additional argument is required"))?;

        // Check for unexpected type name.
        if let Some(typ) = &val.type_name {
            ctx.emit_error(DecodeError::TypeName {
                span: typ.span().clone(),
                found: Some((**typ).clone()),
                expected: knuffel::errors::ExpectedType::no_type(),
                rust_type: "str",
            });
        }

        // Check the argument type.
        let rv = match *val.literal {
            // If it's a string, use FromStr.
            knuffel::ast::Literal::String(ref s) => {
                Color::from_str(s).map_err(|e| DecodeError::conversion(&val.literal, e))
            }
            // Otherwise, fall back to the 4-argument RGBA form.
            _ => return ColorRgba::decode_node(node, ctx).map(Color::from),
        }?;

        // Check for unexpected following arguments.
        if let Some(val) = iter_args.next() {
            ctx.emit_error(DecodeError::unexpected(
                &val.literal,
                "argument",
                "only one string argument is accepted",
            ));
        }

        // Check for unexpected properties.
        for name in node.properties.keys() {
            ctx.emit_error(DecodeError::unexpected(
                name,
                "property",
                "no properties expected for this node",
            ))
        }

        // Check for unexpected children.
        for child in node.children.iter() {
            ctx.emit_error(DecodeError::unexpected(
                child,
                "child node",
                "no child nodes expected for this node",
            ))
        }

        Ok(rv)
    }
}
