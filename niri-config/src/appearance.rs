use std::ops::{Mul, MulAssign};
use std::str::FromStr;

use knuffel::errors::DecodeError;
use miette::{miette, IntoDiagnostic as _};
use smithay::backend::renderer::Color32F;

use crate::FloatOrInt;

pub const DEFAULT_BACKGROUND_COLOR: Color = Color::from_array_unpremul([0.25, 0.25, 0.25, 1.]);
pub const DEFAULT_BACKDROP_COLOR: Color = Color::from_array_unpremul([0.15, 0.15, 0.15, 1.]);

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

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CornerRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl From<CornerRadius> for [f32; 4] {
    fn from(value: CornerRadius) -> Self {
        [
            value.top_left,
            value.top_right,
            value.bottom_right,
            value.bottom_left,
        ]
    }
}

impl From<f32> for CornerRadius {
    fn from(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

impl CornerRadius {
    pub fn fit_to(self, width: f32, height: f32) -> Self {
        // Like in CSS: https://drafts.csswg.org/css-backgrounds/#corner-overlap
        let reduction = f32::min(
            f32::min(
                width / (self.top_left + self.top_right),
                width / (self.bottom_left + self.bottom_right),
            ),
            f32::min(
                height / (self.top_left + self.bottom_left),
                height / (self.top_right + self.bottom_right),
            ),
        );
        let reduction = f32::min(1., reduction);

        Self {
            top_left: self.top_left * reduction,
            top_right: self.top_right * reduction,
            bottom_right: self.bottom_right * reduction,
            bottom_left: self.bottom_left * reduction,
        }
    }

    pub fn expanded_by(mut self, width: f32) -> Self {
        // Radius = 0 is preserved, so that square corners remain square.
        if self.top_left > 0. {
            self.top_left += width;
        }
        if self.top_right > 0. {
            self.top_right += width;
        }
        if self.bottom_right > 0. {
            self.bottom_right += width;
        }
        if self.bottom_left > 0. {
            self.bottom_left += width;
        }

        if width < 0. {
            self.top_left = self.top_left.max(0.);
            self.top_right = self.top_right.max(0.);
            self.bottom_left = self.bottom_left.max(0.);
            self.bottom_right = self.bottom_right.max(0.);
        }

        self
    }

    pub fn scaled_by(self, scale: f32) -> Self {
        Self {
            top_left: self.top_left * scale,
            top_right: self.top_right * scale,
            bottom_right: self.bottom_right * scale,
            bottom_left: self.bottom_left * scale,
        }
    }
}

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
pub struct Blur {
    #[knuffel(child)]
    pub on: bool,
    #[knuffel(child, unwrap(argument), default = Self::default().passes)]
    pub passes: u32,
    #[knuffel(child, unwrap(argument), default = Self::default().radius)]
    pub radius: FloatOrInt<0, 1024>,
    #[knuffel(child, unwrap(argument), default = Self::default().noise)]
    pub noise: FloatOrInt<0, 1024>,
}

impl Default for Blur {
    fn default() -> Self {
        Self {
            on: false,
            passes: 2,
            radius: FloatOrInt(4.),
            noise: FloatOrInt(0.),
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
            color: Color::from_rgba8_unpremul(0, 0, 0, 0x77),
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

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockOutFrom {
    Screencast,
    ScreenCapture,
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
pub struct BlurRule {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub on: bool,
    #[knuffel(child, unwrap(argument))]
    pub passes: Option<u32>,
    #[knuffel(child, unwrap(argument))]
    pub radius: Option<FloatOrInt<0, 1024>>,
    #[knuffel(child, unwrap(argument))]
    pub noise: Option<FloatOrInt<0, 1024>>,
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
            self.active_gradient = None;
        }
        if let Some(x) = other.inactive_color {
            self.inactive_color = Some(x);
            self.inactive_gradient = None;
        }
        if let Some(x) = other.urgent_color {
            self.urgent_color = Some(x);
            self.urgent_gradient = None;
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

impl BlurRule {
    pub fn merge_with(&mut self, other: &Self) {
        if other.off {
            self.off = true;
            self.on = false;
        }

        if other.on {
            self.off = false;
            self.on = true;
        }

        if let Some(x) = other.passes {
            self.passes = Some(x);
        }

        if let Some(x) = other.radius {
            self.radius = Some(x);
        }

        if let Some(x) = other.noise {
            self.noise = Some(x);
        }
    }

    pub fn resolve_against(&self, mut config: Blur) -> Blur {
        config.on |= self.on;

        if self.off {
            config.on = false;
        }

        if let Some(x) = self.passes {
            config.passes = x;
        }

        if let Some(x) = self.radius {
            config.radius = x;
        }

        if let Some(x) = self.noise {
            config.noise = x;
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
            self.active_gradient = None;
        }
        if let Some(x) = other.inactive_color {
            self.inactive_color = Some(x);
            self.inactive_gradient = None;
        }
        if let Some(x) = other.urgent_color {
            self.urgent_color = Some(x);
            self.urgent_gradient = None;
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
                "unexpected argument",
            ));
        }

        // Check for unexpected properties and children.
        for name in node.properties.keys() {
            ctx.emit_error(DecodeError::unexpected(
                name,
                "property",
                format!("unexpected property `{}`", name.escape_default()),
            ));
        }
        for child in node.children.as_ref().map(|lst| &lst[..]).unwrap_or(&[]) {
            ctx.emit_error(DecodeError::unexpected(
                child,
                "node",
                format!("unexpected node `{}`", child.node_name.escape_default()),
            ));
        }

        Ok(rv)
    }
}

impl<S> knuffel::Decode<S> for CornerRadius
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

        let decode_radius = |ctx: &mut knuffel::decode::Context<S>,
                             val: &knuffel::ast::Value<S>| {
            // Check for unexpected type name.
            if let Some(typ) = &val.type_name {
                ctx.emit_error(DecodeError::TypeName {
                    span: typ.span().clone(),
                    found: Some((**typ).clone()),
                    expected: knuffel::errors::ExpectedType::no_type(),
                    rust_type: "str",
                });
            }

            // Decode both integers and floats.
            let radius = match *val.literal {
                knuffel::ast::Literal::Int(ref x) => f32::from(match x.try_into() {
                    Ok(x) => x,
                    Err(err) => {
                        ctx.emit_error(DecodeError::conversion(&val.literal, err));
                        0i16
                    }
                }),
                knuffel::ast::Literal::Decimal(ref x) => match x.try_into() {
                    Ok(x) => x,
                    Err(err) => {
                        ctx.emit_error(DecodeError::conversion(&val.literal, err));
                        0.
                    }
                },
                _ => {
                    ctx.emit_error(DecodeError::scalar_kind(
                        knuffel::decode::Kind::Int,
                        &val.literal,
                    ));
                    0.
                }
            };

            if radius < 0. {
                ctx.emit_error(DecodeError::conversion(&val.literal, "radius must be >= 0"));
            }

            radius
        };

        // Get the first argument.
        let mut iter_args = node.arguments.iter();
        let val = iter_args
            .next()
            .ok_or_else(|| DecodeError::missing(node, "additional argument is required"))?;

        let top_left = decode_radius(ctx, val);

        let mut rv = CornerRadius {
            top_left,
            top_right: top_left,
            bottom_right: top_left,
            bottom_left: top_left,
        };

        if let Some(val) = iter_args.next() {
            rv.top_right = decode_radius(ctx, val);

            let val = iter_args.next().ok_or_else(|| {
                DecodeError::missing(node, "either 1 or 4 arguments are required")
            })?;
            rv.bottom_right = decode_radius(ctx, val);

            let val = iter_args.next().ok_or_else(|| {
                DecodeError::missing(node, "either 1 or 4 arguments are required")
            })?;
            rv.bottom_left = decode_radius(ctx, val);

            // Check for unexpected following arguments.
            if let Some(val) = iter_args.next() {
                ctx.emit_error(DecodeError::unexpected(
                    &val.literal,
                    "argument",
                    "unexpected argument",
                ));
            }
        }

        // Check for unexpected properties and children.
        for name in node.properties.keys() {
            ctx.emit_error(DecodeError::unexpected(
                name,
                "property",
                format!("unexpected property `{}`", name.escape_default()),
            ));
        }
        for child in node.children.as_ref().map(|lst| &lst[..]).unwrap_or(&[]) {
            ctx.emit_error(DecodeError::unexpected(
                child,
                "node",
                format!("unexpected node `{}`", child.node_name.escape_default()),
            ));
        }

        Ok(rv)
    }
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};

    use super::*;
    use crate::Config;

    #[test]
    fn parse_gradient_interpolation() {
        assert_eq!(
            "srgb".parse::<GradientInterpolation>().unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::Srgb,
                ..Default::default()
            }
        );
        assert_eq!(
            "srgb-linear".parse::<GradientInterpolation>().unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::SrgbLinear,
                ..Default::default()
            }
        );
        assert_eq!(
            "oklab".parse::<GradientInterpolation>().unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::Oklab,
                ..Default::default()
            }
        );
        assert_eq!(
            "oklch".parse::<GradientInterpolation>().unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::Oklch,
                ..Default::default()
            }
        );
        assert_eq!(
            "oklch shorter hue"
                .parse::<GradientInterpolation>()
                .unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::Oklch,
                hue_interpolation: HueInterpolation::Shorter,
            }
        );
        assert_eq!(
            "oklch longer hue".parse::<GradientInterpolation>().unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::Oklch,
                hue_interpolation: HueInterpolation::Longer,
            }
        );
        assert_eq!(
            "oklch decreasing hue"
                .parse::<GradientInterpolation>()
                .unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::Oklch,
                hue_interpolation: HueInterpolation::Decreasing,
            }
        );
        assert_eq!(
            "oklch increasing hue"
                .parse::<GradientInterpolation>()
                .unwrap(),
            GradientInterpolation {
                color_space: GradientColorSpace::Oklch,
                hue_interpolation: HueInterpolation::Increasing,
            }
        );

        assert!("".parse::<GradientInterpolation>().is_err());
        assert!("srgb shorter hue".parse::<GradientInterpolation>().is_err());
        assert!("oklch shorter".parse::<GradientInterpolation>().is_err());
        assert!("oklch shorter h".parse::<GradientInterpolation>().is_err());
        assert!("oklch a hue".parse::<GradientInterpolation>().is_err());
        assert!("oklch shorter hue a"
            .parse::<GradientInterpolation>()
            .is_err());
    }

    #[test]
    fn test_border_rule_on_off_merging() {
        fn is_on(config: &str, rules: &[&str]) -> String {
            let mut resolved = BorderRule {
                off: false,
                on: false,
                width: None,
                active_color: None,
                inactive_color: None,
                urgent_color: None,
                active_gradient: None,
                inactive_gradient: None,
                urgent_gradient: None,
            };

            for rule in rules.iter().copied() {
                let rule = BorderRule {
                    off: rule == "off" || rule == "off,on",
                    on: rule == "on" || rule == "off,on",
                    ..Default::default()
                };

                resolved.merge_with(&rule);
            }

            let config = Border {
                off: config == "off",
                ..Default::default()
            };

            if resolved.resolve_against(config).off {
                "off"
            } else {
                "on"
            }
            .to_owned()
        }

        assert_snapshot!(is_on("off", &[]), @"off");
        assert_snapshot!(is_on("off", &["off"]), @"off");
        assert_snapshot!(is_on("off", &["on"]), @"on");
        assert_snapshot!(is_on("off", &["off,on"]), @"on");

        assert_snapshot!(is_on("on", &[]), @"on");
        assert_snapshot!(is_on("on", &["off"]), @"off");
        assert_snapshot!(is_on("on", &["on"]), @"on");
        assert_snapshot!(is_on("on", &["off,on"]), @"on");

        assert_snapshot!(is_on("off", &["off", "off"]), @"off");
        assert_snapshot!(is_on("off", &["off", "on"]), @"on");
        assert_snapshot!(is_on("off", &["on", "off"]), @"off");
        assert_snapshot!(is_on("off", &["on", "on"]), @"on");

        assert_snapshot!(is_on("on", &["off", "off"]), @"off");
        assert_snapshot!(is_on("on", &["off", "on"]), @"on");
        assert_snapshot!(is_on("on", &["on", "off"]), @"off");
        assert_snapshot!(is_on("on", &["on", "on"]), @"on");
    }

    #[test]
    fn rule_color_can_override_base_gradient() {
        let config = Config::parse(
            "test.kdl",
            r##"
            // Start with gradient set.
            layout {
                border {
                    active-gradient from="#101010" to="#202020"
                    inactive-gradient from="#111111" to="#212121"
                    urgent-gradient from="#121212" to="#222222"
                }
            }

            // Override with color.
            window-rule {
                border {
                    active-color "#abcdef"
                    inactive-color "#123456"
                    urgent-color "#fedcba"
                }
            }
            "##,
        )
        .unwrap();

        let mut border_rule = BorderRule::default();
        for rule in &config.window_rules {
            border_rule.merge_with(&rule.border);
        }

        let border = border_rule.resolve_against(config.layout.border);

        // Gradient should be None because it's overwritten.
        assert_debug_snapshot!(
            (
                border.active_gradient.is_some(),
                border.inactive_gradient.is_some(),
                border.urgent_gradient.is_some(),
            ),
            @r"
        (
            false,
            false,
            false,
        )
        "
        );
    }

    #[test]
    fn rule_color_can_override_rule_gradient() {
        let config = Config::parse(
            "test.kdl",
            r##"
            // Start with gradient set.
            layout {
                border {
                    active-gradient from="#101010" to="#202020"
                    inactive-gradient from="#111111" to="#212121"
                    urgent-gradient from="#121212" to="#222222"
                }
            }

            // Window rule with gradients set.
            window-rule {
                border {
                    active-gradient from="#303030" to="#404040"
                    inactive-gradient from="#313131" to="#414141"
                    urgent-gradient from="#323232" to="#424242"
                }

                tab-indicator {
                    active-gradient from="#505050" to="#606060"
                    inactive-gradient from="#515151" to="#616161"
                    urgent-gradient from="#525252" to="#626262"
                }
            }

            // Override with color.
            window-rule {
                border {
                    active-color "#abcdef"
                    inactive-color "#123456"
                    urgent-color "#fedcba"
                }

                tab-indicator {
                    active-color "#abcdef"
                    inactive-color "#123456"
                    urgent-color "#fedcba"
                }
            }
            "##,
        )
        .unwrap();

        let mut border_rule = BorderRule::default();
        let mut tab_indicator_rule = TabIndicatorRule::default();
        for rule in &config.window_rules {
            border_rule.merge_with(&rule.border);
            tab_indicator_rule.merge_with(&rule.tab_indicator);
        }

        let border = border_rule.resolve_against(config.layout.border);

        // Gradient should be None because it's overwritten.
        assert_debug_snapshot!(
            (
                border.active_gradient.is_some(),
                border.inactive_gradient.is_some(),
                border.urgent_gradient.is_some(),
                tab_indicator_rule.active_gradient.is_some(),
                tab_indicator_rule.inactive_gradient.is_some(),
                tab_indicator_rule.urgent_gradient.is_some(),
            ),
            @r"
        (
            false,
            false,
            false,
            false,
            false,
            false,
        )
        "
        );
    }
}
