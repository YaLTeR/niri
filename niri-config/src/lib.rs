#[macro_use]
extern crate tracing;

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use bitflags::bitflags;
use knuffel::errors::DecodeError;
use knuffel::Decode as _;
use layer_rule::LayerRule;
use miette::{miette, Context, IntoDiagnostic, NarratableReportHandler};
use niri_ipc::{
    ConfiguredMode, LayoutSwitchTarget, PositionChange, SizeChange, Transform,
    WorkspaceReferenceArg,
};
use smithay::backend::renderer::Color32F;
use smithay::input::keyboard::keysyms::KEY_NoSymbol;
use smithay::input::keyboard::xkb::{keysym_from_name, KEYSYM_CASE_INSENSITIVE};
use smithay::input::keyboard::{Keysym, XkbConfig};
use smithay::reexports::input;

pub const DEFAULT_BACKGROUND_COLOR: Color = Color::from_array_unpremul([0.2, 0.2, 0.2, 1.]);

pub mod layer_rule;

mod utils;
pub use utils::RegexEq;

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Config {
    #[knuffel(child, default)]
    pub input: Input,
    #[knuffel(children(name = "output"))]
    pub outputs: Outputs,
    #[knuffel(children(name = "spawn-at-startup"))]
    pub spawn_at_startup: Vec<SpawnAtStartup>,
    #[knuffel(child, default)]
    pub layout: Layout,
    #[knuffel(child, default)]
    pub prefer_no_csd: bool,
    #[knuffel(child, default)]
    pub cursor: Cursor,
    #[knuffel(
        child,
        unwrap(argument),
        default = Some(String::from(
            "~/Pictures/Screenshots/Screenshot from %Y-%m-%d %H-%M-%S.png"
        )))
    ]
    pub screenshot_path: Option<String>,
    #[knuffel(child, default)]
    pub hotkey_overlay: HotkeyOverlay,
    #[knuffel(child, default)]
    pub animations: Animations,
    #[knuffel(child, default)]
    pub environment: Environment,
    #[knuffel(children(name = "window-rule"))]
    pub window_rules: Vec<WindowRule>,
    #[knuffel(children(name = "layer-rule"))]
    pub layer_rules: Vec<LayerRule>,
    #[knuffel(child, default)]
    pub binds: Binds,
    #[knuffel(child, default)]
    pub switch_events: SwitchBinds,
    #[knuffel(child, default)]
    pub debug: DebugConfig,
    #[knuffel(children(name = "workspace"))]
    pub workspaces: Vec<Workspace>,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Input {
    #[knuffel(child, default)]
    pub keyboard: Keyboard,
    #[knuffel(child, default)]
    pub touchpad: Touchpad,
    #[knuffel(child, default)]
    pub mouse: Mouse,
    #[knuffel(child, default)]
    pub trackpoint: Trackpoint,
    #[knuffel(child, default)]
    pub trackball: Trackball,
    #[knuffel(child, default)]
    pub tablet: Tablet,
    #[knuffel(child, default)]
    pub touch: Touch,
    #[knuffel(child)]
    pub disable_power_key_handling: bool,
    #[knuffel(child)]
    pub warp_mouse_to_focus: bool,
    #[knuffel(child)]
    pub focus_follows_mouse: Option<FocusFollowsMouse>,
    #[knuffel(child)]
    pub workspace_auto_back_and_forth: bool,
}

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
pub struct Keyboard {
    #[knuffel(child, default)]
    pub xkb: Xkb,
    // The defaults were chosen to match wlroots and sway.
    #[knuffel(child, unwrap(argument), default = Self::default().repeat_delay)]
    pub repeat_delay: u16,
    #[knuffel(child, unwrap(argument), default = Self::default().repeat_rate)]
    pub repeat_rate: u8,
    #[knuffel(child, unwrap(argument), default)]
    pub track_layout: TrackLayout,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            xkb: Default::default(),
            repeat_delay: 600,
            repeat_rate: 25,
            track_layout: Default::default(),
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, PartialEq, Eq, Clone)]
pub struct Xkb {
    #[knuffel(child, unwrap(argument), default)]
    pub rules: String,
    #[knuffel(child, unwrap(argument), default)]
    pub model: String,
    #[knuffel(child, unwrap(argument), default)]
    pub layout: String,
    #[knuffel(child, unwrap(argument), default)]
    pub variant: String,
    #[knuffel(child, unwrap(argument))]
    pub options: Option<String>,
}

impl Xkb {
    pub fn to_xkb_config(&self) -> XkbConfig {
        XkbConfig {
            rules: &self.rules,
            model: &self.model,
            layout: &self.layout,
            variant: &self.variant,
            options: self.options.clone(),
        }
    }
}

#[derive(knuffel::DecodeScalar, Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum CenterFocusedColumn {
    /// Focusing a column will not center the column.
    #[default]
    Never,
    /// The focused column will always be centered.
    Always,
    /// Focusing a column will center it if it doesn't fit on the screen together with the
    /// previously focused column.
    OnOverflow,
}

#[derive(knuffel::DecodeScalar, Debug, Default, PartialEq, Eq)]
pub enum TrackLayout {
    /// The layout change is global.
    #[default]
    Global,
    /// The layout change is window local.
    Window,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Touchpad {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub tap: bool,
    #[knuffel(child)]
    pub dwt: bool,
    #[knuffel(child)]
    pub dwtp: bool,
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument, str))]
    pub click_method: Option<ClickMethod>,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
    #[knuffel(child, unwrap(argument, str))]
    pub accel_profile: Option<AccelProfile>,
    #[knuffel(child, unwrap(argument, str))]
    pub scroll_method: Option<ScrollMethod>,
    #[knuffel(child, unwrap(argument))]
    pub scroll_button: Option<u32>,
    #[knuffel(child, unwrap(argument, str))]
    pub tap_button_map: Option<TapButtonMap>,
    #[knuffel(child)]
    pub left_handed: bool,
    #[knuffel(child)]
    pub disabled_on_external_mouse: bool,
    #[knuffel(child)]
    pub middle_emulation: bool,
    #[knuffel(child, unwrap(argument))]
    pub scroll_factor: Option<FloatOrInt<0, 100>>,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Mouse {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
    #[knuffel(child, unwrap(argument, str))]
    pub accel_profile: Option<AccelProfile>,
    #[knuffel(child, unwrap(argument, str))]
    pub scroll_method: Option<ScrollMethod>,
    #[knuffel(child, unwrap(argument))]
    pub scroll_button: Option<u32>,
    #[knuffel(child)]
    pub left_handed: bool,
    #[knuffel(child)]
    pub middle_emulation: bool,
    #[knuffel(child, unwrap(argument))]
    pub scroll_factor: Option<FloatOrInt<0, 100>>,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Trackpoint {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
    #[knuffel(child, unwrap(argument, str))]
    pub accel_profile: Option<AccelProfile>,
    #[knuffel(child, unwrap(argument, str))]
    pub scroll_method: Option<ScrollMethod>,
    #[knuffel(child, unwrap(argument))]
    pub scroll_button: Option<u32>,
    #[knuffel(child)]
    pub middle_emulation: bool,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Trackball {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
    #[knuffel(child, unwrap(argument, str))]
    pub accel_profile: Option<AccelProfile>,
    #[knuffel(child, unwrap(argument, str))]
    pub scroll_method: Option<ScrollMethod>,
    #[knuffel(child, unwrap(argument))]
    pub scroll_button: Option<u32>,
    #[knuffel(child)]
    pub left_handed: bool,
    #[knuffel(child)]
    pub middle_emulation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMethod {
    Clickfinger,
    ButtonAreas,
}

impl From<ClickMethod> for input::ClickMethod {
    fn from(value: ClickMethod) -> Self {
        match value {
            ClickMethod::Clickfinger => Self::Clickfinger,
            ClickMethod::ButtonAreas => Self::ButtonAreas,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelProfile {
    Adaptive,
    Flat,
}

impl From<AccelProfile> for input::AccelProfile {
    fn from(value: AccelProfile) -> Self {
        match value {
            AccelProfile::Adaptive => Self::Adaptive,
            AccelProfile::Flat => Self::Flat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollMethod {
    NoScroll,
    TwoFinger,
    Edge,
    OnButtonDown,
}

impl From<ScrollMethod> for input::ScrollMethod {
    fn from(value: ScrollMethod) -> Self {
        match value {
            ScrollMethod::NoScroll => Self::NoScroll,
            ScrollMethod::TwoFinger => Self::TwoFinger,
            ScrollMethod::Edge => Self::Edge,
            ScrollMethod::OnButtonDown => Self::OnButtonDown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapButtonMap {
    LeftRightMiddle,
    LeftMiddleRight,
}

impl From<TapButtonMap> for input::TapButtonMap {
    fn from(value: TapButtonMap) -> Self {
        match value {
            TapButtonMap::LeftRightMiddle => Self::LeftRightMiddle,
            TapButtonMap::LeftMiddleRight => Self::LeftMiddleRight,
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Tablet {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument))]
    pub map_to_output: Option<String>,
    #[knuffel(child)]
    pub left_handed: bool,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Touch {
    #[knuffel(child, unwrap(argument))]
    pub map_to_output: Option<String>,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct FocusFollowsMouse {
    #[knuffel(property, str)]
    pub max_scroll_amount: Option<Percent>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Percent(pub f64);

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outputs(pub Vec<Output>);

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Output {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(child, unwrap(argument))]
    pub scale: Option<FloatOrInt<0, 10>>,
    #[knuffel(child, unwrap(argument, str), default = Transform::Normal)]
    pub transform: Transform,
    #[knuffel(child)]
    pub position: Option<Position>,
    #[knuffel(child, unwrap(argument, str))]
    pub mode: Option<ConfiguredMode>,
    #[knuffel(child)]
    pub variable_refresh_rate: Option<Vrr>,
    #[knuffel(child, default = DEFAULT_BACKGROUND_COLOR)]
    pub background_color: Color,
}

impl Output {
    pub fn is_vrr_always_on(&self) -> bool {
        self.variable_refresh_rate == Some(Vrr { on_demand: false })
    }

    pub fn is_vrr_on_demand(&self) -> bool {
        self.variable_refresh_rate == Some(Vrr { on_demand: true })
    }

    pub fn is_vrr_always_off(&self) -> bool {
        self.variable_refresh_rate.is_none()
    }
}

impl Default for Output {
    fn default() -> Self {
        Self {
            off: false,
            name: String::new(),
            scale: None,
            transform: Transform::Normal,
            position: None,
            mode: None,
            variable_refresh_rate: None,
            background_color: DEFAULT_BACKGROUND_COLOR,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputName {
    pub connector: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    #[knuffel(property)]
    pub x: i32,
    #[knuffel(property)]
    pub y: i32,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Default)]
pub struct Vrr {
    #[knuffel(property, default = false)]
    pub on_demand: bool,
}

// MIN and MAX generics are only used during parsing to check the value.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FloatOrInt<const MIN: i32, const MAX: i32>(pub f64);

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Layout {
    #[knuffel(child, default)]
    pub focus_ring: FocusRing,
    #[knuffel(child, default)]
    pub border: Border,
    #[knuffel(child, default)]
    pub insert_hint: InsertHint,
    #[knuffel(child, unwrap(children), default)]
    pub preset_column_widths: Vec<PresetSize>,
    #[knuffel(child)]
    pub default_column_width: Option<DefaultPresetSize>,
    #[knuffel(child, unwrap(children), default)]
    pub preset_window_heights: Vec<PresetSize>,
    #[knuffel(child, unwrap(argument), default)]
    pub center_focused_column: CenterFocusedColumn,
    #[knuffel(child)]
    pub always_center_single_column: bool,
    #[knuffel(child)]
    pub empty_workspace_above_first: bool,
    #[knuffel(child, unwrap(argument), default = Self::default().gaps)]
    pub gaps: FloatOrInt<0, 65535>,
    #[knuffel(child, default)]
    pub struts: Struts,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            focus_ring: Default::default(),
            border: Default::default(),
            insert_hint: Default::default(),
            preset_column_widths: Default::default(),
            default_column_width: Default::default(),
            center_focused_column: Default::default(),
            always_center_single_column: false,
            empty_workspace_above_first: false,
            gaps: FloatOrInt(16.),
            struts: Default::default(),
            preset_window_heights: Default::default(),
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct SpawnAtStartup {
    #[knuffel(arguments)]
    pub command: Vec<String>,
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
    #[knuffel(child)]
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
}

impl Default for FocusRing {
    fn default() -> Self {
        Self {
            off: false,
            width: FloatOrInt(4.),
            active_color: Color::from_rgba8_unpremul(127, 200, 255, 255),
            inactive_color: Color::from_rgba8_unpremul(80, 80, 80, 255),
            active_gradient: None,
            inactive_gradient: None,
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
    #[knuffel(child)]
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            off: true,
            width: FloatOrInt(4.),
            active_color: Color::from_rgba8_unpremul(255, 200, 127, 255),
            inactive_color: Color::from_rgba8_unpremul(80, 80, 80, 255),
            active_gradient: None,
            inactive_gradient: None,
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
            active_gradient: value.active_gradient,
            inactive_gradient: value.inactive_gradient,
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
            active_gradient: value.active_gradient,
            inactive_gradient: value.inactive_gradient,
        }
    }
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

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Cursor {
    #[knuffel(child, unwrap(argument), default = String::from("default"))]
    pub xcursor_theme: String,
    #[knuffel(child, unwrap(argument), default = 24)]
    pub xcursor_size: u8,
    #[knuffel(child)]
    pub hide_when_typing: bool,
    #[knuffel(child, unwrap(argument))]
    pub hide_after_inactive_ms: Option<u32>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            xcursor_theme: String::from("default"),
            xcursor_size: 24,
            hide_when_typing: false,
            hide_after_inactive_ms: None,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub enum PresetSize {
    Proportion(#[knuffel(argument)] f64),
    Fixed(#[knuffel(argument)] i32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultPresetSize(pub Option<PresetSize>);

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
pub struct Struts {
    #[knuffel(child, unwrap(argument), default)]
    pub left: FloatOrInt<-65535, 65535>,
    #[knuffel(child, unwrap(argument), default)]
    pub right: FloatOrInt<-65535, 65535>,
    #[knuffel(child, unwrap(argument), default)]
    pub top: FloatOrInt<-65535, 65535>,
    #[knuffel(child, unwrap(argument), default)]
    pub bottom: FloatOrInt<-65535, 65535>,
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyOverlay {
    #[knuffel(child)]
    pub skip_at_startup: bool,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Animations {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = 1.)]
    pub slowdown: f64,
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
}

impl Default for Animations {
    fn default() -> Self {
        Self {
            off: false,
            slowdown: 1.,
            workspace_switch: Default::default(),
            horizontal_view_movement: Default::default(),
            window_movement: Default::default(),
            window_open: Default::default(),
            window_close: Default::default(),
            window_resize: Default::default(),
            config_notification_open_close: Default::default(),
            screenshot_ui_open: Default::default(),
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

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq, Eq)]
pub struct Environment(#[knuffel(children)] pub Vec<EnvironmentVariable>);

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentVariable {
    #[knuffel(node_name)]
    pub name: String,
    #[knuffel(argument)]
    pub value: Option<String>,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    #[knuffel(argument)]
    pub name: WorkspaceName,
    #[knuffel(child, unwrap(argument))]
    pub open_on_output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceName(pub String);

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct WindowRule {
    #[knuffel(children(name = "match"))]
    pub matches: Vec<Match>,
    #[knuffel(children(name = "exclude"))]
    pub excludes: Vec<Match>,

    // Rules applied at initial configure.
    #[knuffel(child)]
    pub default_column_width: Option<DefaultPresetSize>,
    #[knuffel(child)]
    pub default_window_height: Option<DefaultPresetSize>,
    #[knuffel(child, unwrap(argument))]
    pub open_on_output: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub open_on_workspace: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub open_maximized: Option<bool>,
    #[knuffel(child, unwrap(argument))]
    pub open_fullscreen: Option<bool>,
    #[knuffel(child, unwrap(argument))]
    pub open_floating: Option<bool>,
    #[knuffel(child, unwrap(argument))]
    pub open_focused: Option<bool>,

    // Rules applied dynamically.
    #[knuffel(child, unwrap(argument))]
    pub min_width: Option<u16>,
    #[knuffel(child, unwrap(argument))]
    pub min_height: Option<u16>,
    #[knuffel(child, unwrap(argument))]
    pub max_width: Option<u16>,
    #[knuffel(child, unwrap(argument))]
    pub max_height: Option<u16>,

    #[knuffel(child, default)]
    pub focus_ring: BorderRule,
    #[knuffel(child, default)]
    pub border: BorderRule,
    #[knuffel(child, unwrap(argument))]
    pub draw_border_with_background: Option<bool>,
    #[knuffel(child, unwrap(argument))]
    pub opacity: Option<f32>,
    #[knuffel(child)]
    pub geometry_corner_radius: Option<CornerRadius>,
    #[knuffel(child, unwrap(argument))]
    pub clip_to_geometry: Option<bool>,
    #[knuffel(child, unwrap(argument))]
    pub block_out_from: Option<BlockOutFrom>,
    #[knuffel(child, unwrap(argument))]
    pub variable_refresh_rate: Option<bool>,
    #[knuffel(child)]
    pub default_floating_position: Option<FoIPosition>,
}

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct Match {
    #[knuffel(property, str)]
    pub app_id: Option<RegexEq>,
    #[knuffel(property, str)]
    pub title: Option<RegexEq>,
    #[knuffel(property)]
    pub is_active: Option<bool>,
    #[knuffel(property)]
    pub is_focused: Option<bool>,
    #[knuffel(property)]
    pub is_active_in_column: Option<bool>,
    #[knuffel(property)]
    pub is_floating: Option<bool>,
    #[knuffel(property)]
    pub at_startup: Option<bool>,
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
    pub active_gradient: Option<Gradient>,
    #[knuffel(child)]
    pub inactive_gradient: Option<Gradient>,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct FoIPosition {
    #[knuffel(property)]
    pub x: FloatOrInt<-65535, 65535>,
    #[knuffel(property)]
    pub y: FloatOrInt<-65535, 65535>,
    #[knuffel(property, default)]
    pub relative_to: RelativeTo,
}

#[derive(knuffel::DecodeScalar, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RelativeTo {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Default, PartialEq)]
pub struct Binds(pub Vec<Bind>);

#[derive(Debug, Clone, PartialEq)]
pub struct Bind {
    pub key: Key,
    pub action: Action,
    pub repeat: bool,
    pub cooldown: Option<Duration>,
    pub allow_when_locked: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Key {
    pub trigger: Trigger,
    pub modifiers: Modifiers,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Trigger {
    Keysym(Keysym),
    WheelScrollDown,
    WheelScrollUp,
    WheelScrollLeft,
    WheelScrollRight,
    TouchpadScrollDown,
    TouchpadScrollUp,
    TouchpadScrollLeft,
    TouchpadScrollRight,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers : u8 {
        const CTRL = 1;
        const SHIFT = 1 << 1;
        const ALT = 1 << 2;
        const SUPER = 1 << 3;
        const ISO_LEVEL3_SHIFT = 1 << 4;
        const ISO_LEVEL5_SHIFT = 1 << 5;
        const COMPOSITOR = 1 << 6;
    }
}

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct SwitchBinds {
    #[knuffel(child)]
    pub lid_open: Option<SwitchAction>,
    #[knuffel(child)]
    pub lid_close: Option<SwitchAction>,
    #[knuffel(child)]
    pub tablet_mode_on: Option<SwitchAction>,
    #[knuffel(child)]
    pub tablet_mode_off: Option<SwitchAction>,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct SwitchAction {
    #[knuffel(child, unwrap(arguments))]
    pub spawn: Vec<String>,
}

// Remember to add new actions to the CLI enum too.
#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub enum Action {
    Quit(#[knuffel(property(name = "skip-confirmation"), default)] bool),
    #[knuffel(skip)]
    ChangeVt(i32),
    Suspend,
    PowerOffMonitors,
    PowerOnMonitors,
    ToggleDebugTint,
    DebugToggleOpaqueRegions,
    DebugToggleDamage,
    Spawn(#[knuffel(arguments)] Vec<String>),
    DoScreenTransition(#[knuffel(property(name = "delay-ms"))] Option<u16>),
    #[knuffel(skip)]
    ConfirmScreenshot,
    #[knuffel(skip)]
    CancelScreenshot,
    #[knuffel(skip)]
    ScreenshotTogglePointer,
    Screenshot,
    ScreenshotScreen,
    ScreenshotWindow,
    #[knuffel(skip)]
    ScreenshotWindowById(u64),
    CloseWindow,
    #[knuffel(skip)]
    CloseWindowById(u64),
    FullscreenWindow,
    #[knuffel(skip)]
    FullscreenWindowById(u64),
    #[knuffel(skip)]
    FocusWindow(u64),
    FocusWindowPrevious,
    FocusColumnLeft,
    FocusColumnRight,
    FocusColumnFirst,
    FocusColumnLast,
    FocusColumnRightOrFirst,
    FocusColumnLeftOrLast,
    FocusWindowOrMonitorUp,
    FocusWindowOrMonitorDown,
    FocusColumnOrMonitorLeft,
    FocusColumnOrMonitorRight,
    FocusWindowDown,
    FocusWindowUp,
    FocusWindowDownOrColumnLeft,
    FocusWindowDownOrColumnRight,
    FocusWindowUpOrColumnLeft,
    FocusWindowUpOrColumnRight,
    FocusWindowOrWorkspaceDown,
    FocusWindowOrWorkspaceUp,
    MoveColumnLeft,
    MoveColumnRight,
    MoveColumnToFirst,
    MoveColumnToLast,
    MoveColumnLeftOrToMonitorLeft,
    MoveColumnRightOrToMonitorRight,
    MoveWindowDown,
    MoveWindowUp,
    MoveWindowDownOrToWorkspaceDown,
    MoveWindowUpOrToWorkspaceUp,
    ConsumeOrExpelWindowLeft,
    #[knuffel(skip)]
    ConsumeOrExpelWindowLeftById(u64),
    ConsumeOrExpelWindowRight,
    #[knuffel(skip)]
    ConsumeOrExpelWindowRightById(u64),
    ConsumeWindowIntoColumn,
    ExpelWindowFromColumn,
    CenterColumn,
    CenterWindow,
    #[knuffel(skip)]
    CenterWindowById(u64),
    FocusWorkspaceDown,
    FocusWorkspaceUp,
    FocusWorkspace(#[knuffel(argument)] WorkspaceReference),
    FocusWorkspacePrevious,
    MoveWindowToWorkspaceDown,
    MoveWindowToWorkspaceUp,
    MoveWindowToWorkspace(#[knuffel(argument)] WorkspaceReference),
    #[knuffel(skip)]
    MoveWindowToWorkspaceById {
        window_id: u64,
        reference: WorkspaceReference,
    },
    MoveColumnToWorkspaceDown,
    MoveColumnToWorkspaceUp,
    MoveColumnToWorkspace(#[knuffel(argument)] WorkspaceReference),
    MoveWorkspaceDown,
    MoveWorkspaceUp,
    FocusMonitorLeft,
    FocusMonitorRight,
    FocusMonitorDown,
    FocusMonitorUp,
    MoveWindowToMonitorLeft,
    MoveWindowToMonitorRight,
    MoveWindowToMonitorDown,
    MoveWindowToMonitorUp,
    MoveColumnToMonitorLeft,
    MoveColumnToMonitorRight,
    MoveColumnToMonitorDown,
    MoveColumnToMonitorUp,
    SetWindowWidth(#[knuffel(argument, str)] SizeChange),
    #[knuffel(skip)]
    SetWindowWidthById {
        id: u64,
        change: SizeChange,
    },
    SetWindowHeight(#[knuffel(argument, str)] SizeChange),
    #[knuffel(skip)]
    SetWindowHeightById {
        id: u64,
        change: SizeChange,
    },
    ResetWindowHeight,
    #[knuffel(skip)]
    ResetWindowHeightById(u64),
    SwitchPresetColumnWidth,
    SwitchPresetWindowWidth,
    #[knuffel(skip)]
    SwitchPresetWindowWidthById(u64),
    SwitchPresetWindowHeight,
    #[knuffel(skip)]
    SwitchPresetWindowHeightById(u64),
    MaximizeColumn,
    SetColumnWidth(#[knuffel(argument, str)] SizeChange),
    SwitchLayout(#[knuffel(argument, str)] LayoutSwitchTarget),
    ShowHotkeyOverlay,
    MoveWorkspaceToMonitorLeft,
    MoveWorkspaceToMonitorRight,
    MoveWorkspaceToMonitorDown,
    MoveWorkspaceToMonitorUp,
    ToggleWindowFloating,
    #[knuffel(skip)]
    ToggleWindowFloatingById(u64),
    MoveWindowToFloating,
    #[knuffel(skip)]
    MoveWindowToFloatingById(u64),
    MoveWindowToTiling,
    #[knuffel(skip)]
    MoveWindowToTilingById(u64),
    FocusFloating,
    FocusTiling,
    SwitchFocusBetweenFloatingAndTiling,
    #[knuffel(skip)]
    MoveFloatingWindowById {
        id: Option<u64>,
        x: PositionChange,
        y: PositionChange,
    },
}

impl From<niri_ipc::Action> for Action {
    fn from(value: niri_ipc::Action) -> Self {
        match value {
            niri_ipc::Action::Quit { skip_confirmation } => Self::Quit(skip_confirmation),
            niri_ipc::Action::PowerOffMonitors {} => Self::PowerOffMonitors,
            niri_ipc::Action::PowerOnMonitors {} => Self::PowerOnMonitors,
            niri_ipc::Action::Spawn { command } => Self::Spawn(command),
            niri_ipc::Action::DoScreenTransition { delay_ms } => Self::DoScreenTransition(delay_ms),
            niri_ipc::Action::Screenshot {} => Self::Screenshot,
            niri_ipc::Action::ScreenshotScreen {} => Self::ScreenshotScreen,
            niri_ipc::Action::ScreenshotWindow { id: None } => Self::ScreenshotWindow,
            niri_ipc::Action::ScreenshotWindow { id: Some(id) } => Self::ScreenshotWindowById(id),
            niri_ipc::Action::CloseWindow { id: None } => Self::CloseWindow,
            niri_ipc::Action::CloseWindow { id: Some(id) } => Self::CloseWindowById(id),
            niri_ipc::Action::FullscreenWindow { id: None } => Self::FullscreenWindow,
            niri_ipc::Action::FullscreenWindow { id: Some(id) } => Self::FullscreenWindowById(id),
            niri_ipc::Action::FocusWindow { id } => Self::FocusWindow(id),
            niri_ipc::Action::FocusWindowPrevious {} => Self::FocusWindowPrevious,
            niri_ipc::Action::FocusColumnLeft {} => Self::FocusColumnLeft,
            niri_ipc::Action::FocusColumnRight {} => Self::FocusColumnRight,
            niri_ipc::Action::FocusColumnFirst {} => Self::FocusColumnFirst,
            niri_ipc::Action::FocusColumnLast {} => Self::FocusColumnLast,
            niri_ipc::Action::FocusColumnRightOrFirst {} => Self::FocusColumnRightOrFirst,
            niri_ipc::Action::FocusColumnLeftOrLast {} => Self::FocusColumnLeftOrLast,
            niri_ipc::Action::FocusWindowOrMonitorUp {} => Self::FocusWindowOrMonitorUp,
            niri_ipc::Action::FocusWindowOrMonitorDown {} => Self::FocusWindowOrMonitorDown,
            niri_ipc::Action::FocusColumnOrMonitorLeft {} => Self::FocusColumnOrMonitorLeft,
            niri_ipc::Action::FocusColumnOrMonitorRight {} => Self::FocusColumnOrMonitorRight,
            niri_ipc::Action::FocusWindowDown {} => Self::FocusWindowDown,
            niri_ipc::Action::FocusWindowUp {} => Self::FocusWindowUp,
            niri_ipc::Action::FocusWindowDownOrColumnLeft {} => Self::FocusWindowDownOrColumnLeft,
            niri_ipc::Action::FocusWindowDownOrColumnRight {} => Self::FocusWindowDownOrColumnRight,
            niri_ipc::Action::FocusWindowUpOrColumnLeft {} => Self::FocusWindowUpOrColumnLeft,
            niri_ipc::Action::FocusWindowUpOrColumnRight {} => Self::FocusWindowUpOrColumnRight,
            niri_ipc::Action::FocusWindowOrWorkspaceDown {} => Self::FocusWindowOrWorkspaceDown,
            niri_ipc::Action::FocusWindowOrWorkspaceUp {} => Self::FocusWindowOrWorkspaceUp,
            niri_ipc::Action::MoveColumnLeft {} => Self::MoveColumnLeft,
            niri_ipc::Action::MoveColumnRight {} => Self::MoveColumnRight,
            niri_ipc::Action::MoveColumnToFirst {} => Self::MoveColumnToFirst,
            niri_ipc::Action::MoveColumnToLast {} => Self::MoveColumnToLast,
            niri_ipc::Action::MoveColumnLeftOrToMonitorLeft {} => {
                Self::MoveColumnLeftOrToMonitorLeft
            }
            niri_ipc::Action::MoveColumnRightOrToMonitorRight {} => {
                Self::MoveColumnRightOrToMonitorRight
            }
            niri_ipc::Action::MoveWindowDown {} => Self::MoveWindowDown,
            niri_ipc::Action::MoveWindowUp {} => Self::MoveWindowUp,
            niri_ipc::Action::MoveWindowDownOrToWorkspaceDown {} => {
                Self::MoveWindowDownOrToWorkspaceDown
            }
            niri_ipc::Action::MoveWindowUpOrToWorkspaceUp {} => Self::MoveWindowUpOrToWorkspaceUp,
            niri_ipc::Action::ConsumeOrExpelWindowLeft { id: None } => {
                Self::ConsumeOrExpelWindowLeft
            }
            niri_ipc::Action::ConsumeOrExpelWindowLeft { id: Some(id) } => {
                Self::ConsumeOrExpelWindowLeftById(id)
            }
            niri_ipc::Action::ConsumeOrExpelWindowRight { id: None } => {
                Self::ConsumeOrExpelWindowRight
            }
            niri_ipc::Action::ConsumeOrExpelWindowRight { id: Some(id) } => {
                Self::ConsumeOrExpelWindowRightById(id)
            }
            niri_ipc::Action::ConsumeWindowIntoColumn {} => Self::ConsumeWindowIntoColumn,
            niri_ipc::Action::ExpelWindowFromColumn {} => Self::ExpelWindowFromColumn,
            niri_ipc::Action::CenterColumn {} => Self::CenterColumn,
            niri_ipc::Action::CenterWindow { id: None } => Self::CenterWindow,
            niri_ipc::Action::CenterWindow { id: Some(id) } => Self::CenterWindowById(id),
            niri_ipc::Action::FocusWorkspaceDown {} => Self::FocusWorkspaceDown,
            niri_ipc::Action::FocusWorkspaceUp {} => Self::FocusWorkspaceUp,
            niri_ipc::Action::FocusWorkspace { reference } => {
                Self::FocusWorkspace(WorkspaceReference::from(reference))
            }
            niri_ipc::Action::FocusWorkspacePrevious {} => Self::FocusWorkspacePrevious,
            niri_ipc::Action::MoveWindowToWorkspaceDown {} => Self::MoveWindowToWorkspaceDown,
            niri_ipc::Action::MoveWindowToWorkspaceUp {} => Self::MoveWindowToWorkspaceUp,
            niri_ipc::Action::MoveWindowToWorkspace {
                window_id: None,
                reference,
            } => Self::MoveWindowToWorkspace(WorkspaceReference::from(reference)),
            niri_ipc::Action::MoveWindowToWorkspace {
                window_id: Some(window_id),
                reference,
            } => Self::MoveWindowToWorkspaceById {
                window_id,
                reference: WorkspaceReference::from(reference),
            },
            niri_ipc::Action::MoveColumnToWorkspaceDown {} => Self::MoveColumnToWorkspaceDown,
            niri_ipc::Action::MoveColumnToWorkspaceUp {} => Self::MoveColumnToWorkspaceUp,
            niri_ipc::Action::MoveColumnToWorkspace { reference } => {
                Self::MoveColumnToWorkspace(WorkspaceReference::from(reference))
            }
            niri_ipc::Action::MoveWorkspaceDown {} => Self::MoveWorkspaceDown,
            niri_ipc::Action::MoveWorkspaceUp {} => Self::MoveWorkspaceUp,
            niri_ipc::Action::FocusMonitorLeft {} => Self::FocusMonitorLeft,
            niri_ipc::Action::FocusMonitorRight {} => Self::FocusMonitorRight,
            niri_ipc::Action::FocusMonitorDown {} => Self::FocusMonitorDown,
            niri_ipc::Action::FocusMonitorUp {} => Self::FocusMonitorUp,
            niri_ipc::Action::MoveWindowToMonitorLeft {} => Self::MoveWindowToMonitorLeft,
            niri_ipc::Action::MoveWindowToMonitorRight {} => Self::MoveWindowToMonitorRight,
            niri_ipc::Action::MoveWindowToMonitorDown {} => Self::MoveWindowToMonitorDown,
            niri_ipc::Action::MoveWindowToMonitorUp {} => Self::MoveWindowToMonitorUp,
            niri_ipc::Action::MoveColumnToMonitorLeft {} => Self::MoveColumnToMonitorLeft,
            niri_ipc::Action::MoveColumnToMonitorRight {} => Self::MoveColumnToMonitorRight,
            niri_ipc::Action::MoveColumnToMonitorDown {} => Self::MoveColumnToMonitorDown,
            niri_ipc::Action::MoveColumnToMonitorUp {} => Self::MoveColumnToMonitorUp,
            niri_ipc::Action::SetWindowWidth { id: None, change } => Self::SetWindowWidth(change),
            niri_ipc::Action::SetWindowWidth {
                id: Some(id),
                change,
            } => Self::SetWindowWidthById { id, change },
            niri_ipc::Action::SetWindowHeight { id: None, change } => Self::SetWindowHeight(change),
            niri_ipc::Action::SetWindowHeight {
                id: Some(id),
                change,
            } => Self::SetWindowHeightById { id, change },
            niri_ipc::Action::ResetWindowHeight { id: None } => Self::ResetWindowHeight,
            niri_ipc::Action::ResetWindowHeight { id: Some(id) } => Self::ResetWindowHeightById(id),
            niri_ipc::Action::SwitchPresetColumnWidth {} => Self::SwitchPresetColumnWidth,
            niri_ipc::Action::SwitchPresetWindowWidth { id: None } => Self::SwitchPresetWindowWidth,
            niri_ipc::Action::SwitchPresetWindowWidth { id: Some(id) } => {
                Self::SwitchPresetWindowWidthById(id)
            }
            niri_ipc::Action::SwitchPresetWindowHeight { id: None } => {
                Self::SwitchPresetWindowHeight
            }
            niri_ipc::Action::SwitchPresetWindowHeight { id: Some(id) } => {
                Self::SwitchPresetWindowHeightById(id)
            }
            niri_ipc::Action::MaximizeColumn {} => Self::MaximizeColumn,
            niri_ipc::Action::SetColumnWidth { change } => Self::SetColumnWidth(change),
            niri_ipc::Action::SwitchLayout { layout } => Self::SwitchLayout(layout),
            niri_ipc::Action::ShowHotkeyOverlay {} => Self::ShowHotkeyOverlay,
            niri_ipc::Action::MoveWorkspaceToMonitorLeft {} => Self::MoveWorkspaceToMonitorLeft,
            niri_ipc::Action::MoveWorkspaceToMonitorRight {} => Self::MoveWorkspaceToMonitorRight,
            niri_ipc::Action::MoveWorkspaceToMonitorDown {} => Self::MoveWorkspaceToMonitorDown,
            niri_ipc::Action::MoveWorkspaceToMonitorUp {} => Self::MoveWorkspaceToMonitorUp,
            niri_ipc::Action::ToggleDebugTint {} => Self::ToggleDebugTint,
            niri_ipc::Action::DebugToggleOpaqueRegions {} => Self::DebugToggleOpaqueRegions,
            niri_ipc::Action::DebugToggleDamage {} => Self::DebugToggleDamage,
            niri_ipc::Action::ToggleWindowFloating { id: None } => Self::ToggleWindowFloating,
            niri_ipc::Action::ToggleWindowFloating { id: Some(id) } => {
                Self::ToggleWindowFloatingById(id)
            }
            niri_ipc::Action::MoveWindowToFloating { id: None } => Self::MoveWindowToFloating,
            niri_ipc::Action::MoveWindowToFloating { id: Some(id) } => {
                Self::MoveWindowToFloatingById(id)
            }
            niri_ipc::Action::MoveWindowToTiling { id: None } => Self::MoveWindowToTiling,
            niri_ipc::Action::MoveWindowToTiling { id: Some(id) } => {
                Self::MoveWindowToTilingById(id)
            }
            niri_ipc::Action::FocusFloating {} => Self::FocusFloating,
            niri_ipc::Action::FocusTiling {} => Self::FocusTiling,
            niri_ipc::Action::SwitchFocusBetweenFloatingAndTiling {} => {
                Self::SwitchFocusBetweenFloatingAndTiling
            }
            niri_ipc::Action::MoveFloatingWindow { id, x, y } => {
                Self::MoveFloatingWindowById { id, x, y }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum WorkspaceReference {
    Id(u64),
    Index(u8),
    Name(String),
}

impl From<WorkspaceReferenceArg> for WorkspaceReference {
    fn from(reference: WorkspaceReferenceArg) -> WorkspaceReference {
        match reference {
            WorkspaceReferenceArg::Id(id) => Self::Id(id),
            WorkspaceReferenceArg::Index(i) => Self::Index(i),
            WorkspaceReferenceArg::Name(n) => Self::Name(n),
        }
    }
}

impl<S: knuffel::traits::ErrorSpan> knuffel::DecodeScalar<S> for WorkspaceReference {
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        if let Some(type_name) = &type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<WorkspaceReference, DecodeError<S>> {
        match &**val {
            knuffel::ast::Literal::String(ref s) => Ok(WorkspaceReference::Name(s.clone().into())),
            knuffel::ast::Literal::Int(ref value) => match value.try_into() {
                Ok(v) => Ok(WorkspaceReference::Index(v)),
                Err(e) => {
                    ctx.emit_error(DecodeError::conversion(val, e));
                    Ok(WorkspaceReference::Index(0))
                }
            },
            _ => {
                ctx.emit_error(DecodeError::unsupported(
                    val,
                    "Unsupported value, only numbers and strings are recognized",
                ));
                Ok(WorkspaceReference::Index(0))
            }
        }
    }
}

impl<S: knuffel::traits::ErrorSpan, const MIN: i32, const MAX: i32> knuffel::DecodeScalar<S>
    for FloatOrInt<MIN, MAX>
{
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        if let Some(type_name) = &type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        match &**val {
            knuffel::ast::Literal::Int(ref value) => match value.try_into() {
                Ok(v) => {
                    if (MIN..=MAX).contains(&v) {
                        Ok(FloatOrInt(f64::from(v)))
                    } else {
                        ctx.emit_error(DecodeError::conversion(
                            val,
                            format!("value must be between {MIN} and {MAX}"),
                        ));
                        Ok(FloatOrInt::default())
                    }
                }
                Err(e) => {
                    ctx.emit_error(DecodeError::conversion(val, e));
                    Ok(FloatOrInt::default())
                }
            },
            knuffel::ast::Literal::Decimal(ref value) => match value.try_into() {
                Ok(v) => {
                    if (f64::from(MIN)..=f64::from(MAX)).contains(&v) {
                        Ok(FloatOrInt(v))
                    } else {
                        ctx.emit_error(DecodeError::conversion(
                            val,
                            format!("value must be between {MIN} and {MAX}"),
                        ));
                        Ok(FloatOrInt::default())
                    }
                }
                Err(e) => {
                    ctx.emit_error(DecodeError::conversion(val, e));
                    Ok(FloatOrInt::default())
                }
            },
            _ => {
                ctx.emit_error(DecodeError::unsupported(
                    val,
                    "Unsupported value, only numbers are recognized",
                ));
                Ok(FloatOrInt::default())
            }
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct DebugConfig {
    #[knuffel(child, unwrap(argument))]
    pub preview_render: Option<PreviewRender>,
    #[knuffel(child)]
    pub dbus_interfaces_in_non_session_instances: bool,
    #[knuffel(child)]
    pub wait_for_frame_completion_before_queueing: bool,
    #[knuffel(child)]
    pub enable_overlay_planes: bool,
    #[knuffel(child)]
    pub disable_cursor_plane: bool,
    #[knuffel(child)]
    pub disable_direct_scanout: bool,
    #[knuffel(child, unwrap(argument))]
    pub render_drm_device: Option<PathBuf>,
    #[knuffel(child)]
    pub force_pipewire_invalid_modifier: bool,
    #[knuffel(child)]
    pub emulate_zero_presentation_time: bool,
    #[knuffel(child)]
    pub disable_resize_throttling: bool,
    #[knuffel(child)]
    pub disable_transactions: bool,
    #[knuffel(child)]
    pub keep_laptop_panel_on_when_lid_is_closed: bool,
    #[knuffel(child)]
    pub disable_monitor_names: bool,
    #[knuffel(child)]
    pub strict_new_window_focus_policy: bool,
}

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewRender {
    Screencast,
    ScreenCapture,
}

impl Config {
    pub fn load(path: &Path) -> miette::Result<Self> {
        let _span = tracy_client::span!("Config::load");
        Self::load_internal(path).context("error loading config")
    }

    fn load_internal(path: &Path) -> miette::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .into_diagnostic()
            .with_context(|| format!("error reading {path:?}"))?;

        let config = Self::parse(
            path.file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("config.kdl"),
            &contents,
        )
        .context("error parsing")?;
        debug!("loaded config from {path:?}");
        Ok(config)
    }

    pub fn parse(filename: &str, text: &str) -> Result<Self, knuffel::Error> {
        let _span = tracy_client::span!("Config::parse");
        knuffel::parse(filename, text)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::parse(
            "default-config.kdl",
            include_str!("../../resources/default-config.kdl"),
        )
        .unwrap()
    }
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
        if let Some(x) = other.active_gradient {
            self.active_gradient = Some(x);
        }
        if let Some(x) = other.inactive_gradient {
            self.inactive_gradient = Some(x);
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
        if let Some(x) = self.active_gradient {
            config.active_gradient = Some(x);
        }
        if let Some(x) = self.inactive_gradient {
            config.inactive_gradient = Some(x);
        }

        config
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
        let color = csscolorparser::parse(s).into_diagnostic()?.to_array();
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

impl FromIterator<Output> for Outputs {
    fn from_iter<T: IntoIterator<Item = Output>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}

impl Outputs {
    pub fn find(&self, name: &OutputName) -> Option<&Output> {
        self.0.iter().find(|o| name.matches(&o.name))
    }

    pub fn find_mut(&mut self, name: &OutputName) -> Option<&mut Output> {
        self.0.iter_mut().find(|o| name.matches(&o.name))
    }
}

impl OutputName {
    pub fn from_ipc_output(output: &niri_ipc::Output) -> Self {
        Self {
            connector: output.name.clone(),
            make: (output.make != "Unknown").then(|| output.make.clone()),
            model: (output.model != "Unknown").then(|| output.model.clone()),
            serial: output.serial.clone(),
        }
    }

    /// Returns an output description matching what Smithay's `Output::new()` does.
    pub fn format_description(&self) -> String {
        format!(
            "{} - {} - {}",
            self.make.as_deref().unwrap_or("Unknown"),
            self.model.as_deref().unwrap_or("Unknown"),
            self.connector,
        )
    }

    /// Returns an output name that will match by make/model/serial or, if they are missing, by
    /// connector.
    pub fn format_make_model_serial_or_connector(&self) -> String {
        if self.make.is_none() && self.model.is_none() && self.serial.is_none() {
            self.connector.to_string()
        } else {
            self.format_make_model_serial()
        }
    }

    pub fn format_make_model_serial(&self) -> String {
        let make = self.make.as_deref().unwrap_or("Unknown");
        let model = self.model.as_deref().unwrap_or("Unknown");
        let serial = self.serial.as_deref().unwrap_or("Unknown");
        format!("{make} {model} {serial}")
    }

    pub fn matches(&self, target: &str) -> bool {
        // Match by connector.
        if target.eq_ignore_ascii_case(&self.connector) {
            return true;
        }

        // If no other fields are available, don't try to match by them.
        //
        // This is used by niri msg output.
        if self.make.is_none() && self.model.is_none() && self.serial.is_none() {
            return false;
        }

        // Match by "make model serial" with Unknown if something is missing.
        let make = self.make.as_deref().unwrap_or("Unknown");
        let model = self.model.as_deref().unwrap_or("Unknown");
        let serial = self.serial.as_deref().unwrap_or("Unknown");

        let Some(target_make) = target.get(..make.len()) else {
            return false;
        };
        let rest = &target[make.len()..];
        if !target_make.eq_ignore_ascii_case(make) {
            return false;
        }
        if !rest.starts_with(' ') {
            return false;
        }
        let rest = &rest[1..];

        let Some(target_model) = rest.get(..model.len()) else {
            return false;
        };
        let rest = &rest[model.len()..];
        if !target_model.eq_ignore_ascii_case(model) {
            return false;
        }
        if !rest.starts_with(' ') {
            return false;
        }

        let rest = &rest[1..];
        if !rest.eq_ignore_ascii_case(serial) {
            return false;
        }

        true
    }

    // Similar in spirit to Ord, but I don't want to derive Eq to avoid mistakes (you should use
    // `Self::match`, not Eq).
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        let self_missing_mms = self.make.is_none() && self.model.is_none() && self.serial.is_none();
        let other_missing_mms =
            other.make.is_none() && other.model.is_none() && other.serial.is_none();

        match (self_missing_mms, other_missing_mms) {
            (true, true) => self.connector.cmp(&other.connector),
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => self
                .make
                .cmp(&other.make)
                .then_with(|| self.model.cmp(&other.model))
                .then_with(|| self.serial.cmp(&other.serial))
                .then_with(|| self.connector.cmp(&other.connector)),
        }
    }
}

impl<S> knuffel::Decode<S> for DefaultPresetSize
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        expect_only_children(node, ctx);

        let mut children = node.children();

        if let Some(child) = children.next() {
            if let Some(unwanted_child) = children.next() {
                ctx.emit_error(DecodeError::unexpected(
                    unwanted_child,
                    "node",
                    "expected no more than one child",
                ));
            }
            PresetSize::decode_node(child, ctx).map(Some).map(Self)
        } else {
            Ok(Self(None))
        }
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

impl<S: knuffel::traits::ErrorSpan> knuffel::DecodeScalar<S> for WorkspaceName {
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        if let Some(type_name) = &type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<WorkspaceName, DecodeError<S>> {
        #[derive(Debug)]
        struct WorkspaceNameSet(Vec<String>);
        match &**val {
            knuffel::ast::Literal::String(ref s) => {
                let mut name_set: Vec<String> = match ctx.get::<WorkspaceNameSet>() {
                    Some(h) => h.0.clone(),
                    None => Vec::new(),
                };

                if name_set.iter().any(|name| name.eq_ignore_ascii_case(s)) {
                    ctx.emit_error(DecodeError::unexpected(
                        val,
                        "named workspace",
                        format!("duplicate named workspace: {}", s),
                    ));
                    return Ok(Self(String::new()));
                }

                name_set.push(s.to_string());
                ctx.set(WorkspaceNameSet(name_set));
                Ok(Self(s.clone().into()))
            }
            _ => {
                ctx.emit_error(DecodeError::unsupported(
                    val,
                    "workspace names must be strings",
                ));
                Ok(Self(String::new()))
            }
        }
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

impl<S> knuffel::Decode<S> for Binds
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        expect_only_children(node, ctx);

        let mut seen_keys = HashSet::new();

        let mut binds = Vec::new();

        for child in node.children() {
            match Bind::decode_node(child, ctx) {
                Err(e) => {
                    ctx.emit_error(e);
                }
                Ok(bind) => {
                    if seen_keys.insert(bind.key) {
                        binds.push(bind);
                    } else {
                        // ideally, this error should point to the previous instance of this keybind
                        //
                        // i (sodiboo) have tried to implement this in various ways:
                        // miette!(), #[derive(Diagnostic)]
                        // DecodeError::Custom, DecodeError::Conversion
                        // nothing seems to work, and i suspect it's not possible.
                        //
                        // DecodeError is fairly restrictive.
                        // even DecodeError::Custom just wraps a std::error::Error
                        // and this erases all rich information from miette. (why???)
                        //
                        // why does knuffel do this?
                        // from what i can tell, it doesn't even use DecodeError for much.
                        // it only ever converts them to a Report anyways!
                        // https://github.com/tailhook/knuffel/blob/c44c6b0c0f31ea6d1174d5d2ed41064922ea44ca/src/wrappers.rs#L55-L58
                        //
                        // besides like, allowing downstream users (such as us!)
                        // to match on parse failure, i don't understand why
                        // it doesn't just use a generic error type
                        //
                        // even the matching isn't consistent,
                        // because errors can also be omitted as ctx.emit_error.
                        // why does *that one* especially, require a DecodeError?
                        //
                        // anyways if you can make it format nicely, definitely do fix this
                        ctx.emit_error(DecodeError::unexpected(
                            &child.node_name,
                            "keybind",
                            "duplicate keybind",
                        ));
                    }
                }
            }
        }

        Ok(Self(binds))
    }
}

impl<S> knuffel::Decode<S> for Bind
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

        for val in node.arguments.iter() {
            ctx.emit_error(DecodeError::unexpected(
                &val.literal,
                "argument",
                "no arguments expected for this node",
            ));
        }

        let key = node
            .node_name
            .parse::<Key>()
            .map_err(|e| DecodeError::conversion(&node.node_name, e.wrap_err("invalid keybind")))?;

        let mut repeat = true;
        let mut cooldown = None;
        let mut allow_when_locked = false;
        let mut allow_when_locked_node = None;
        for (name, val) in &node.properties {
            match &***name {
                "repeat" => {
                    repeat = knuffel::traits::DecodeScalar::decode(val, ctx)?;
                }
                "cooldown-ms" => {
                    cooldown = Some(Duration::from_millis(
                        knuffel::traits::DecodeScalar::decode(val, ctx)?,
                    ));
                }
                "allow-when-locked" => {
                    allow_when_locked = knuffel::traits::DecodeScalar::decode(val, ctx)?;
                    allow_when_locked_node = Some(name);
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

        let mut children = node.children();

        // If the action is invalid but the key is fine, we still want to return something.
        // That way, the parent can handle the existence of duplicate keybinds,
        // even if their contents are not valid.
        let dummy = Self {
            key,
            action: Action::Spawn(vec![]),
            repeat: true,
            cooldown: None,
            allow_when_locked: false,
        };

        if let Some(child) = children.next() {
            for unwanted_child in children {
                ctx.emit_error(DecodeError::unexpected(
                    unwanted_child,
                    "node",
                    "only one action is allowed per keybind",
                ));
            }
            match Action::decode_node(child, ctx) {
                Ok(action) => {
                    if !matches!(action, Action::Spawn(_)) {
                        if let Some(node) = allow_when_locked_node {
                            ctx.emit_error(DecodeError::unexpected(
                                node,
                                "property",
                                "allow-when-locked can only be set on spawn binds",
                            ));
                        }
                    }

                    Ok(Self {
                        key,
                        action,
                        repeat,
                        cooldown,
                        allow_when_locked,
                    })
                }
                Err(e) => {
                    ctx.emit_error(e);
                    Ok(dummy)
                }
            }
        } else {
            ctx.emit_error(DecodeError::missing(
                node,
                "expected an action for this keybind",
            ));
            Ok(dummy)
        }
    }
}

impl FromStr for Key {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut modifiers = Modifiers::empty();

        let mut split = s.split('+');
        let key = split.next_back().unwrap();

        for part in split {
            let part = part.trim();
            if part.eq_ignore_ascii_case("mod") {
                modifiers |= Modifiers::COMPOSITOR
            } else if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
                modifiers |= Modifiers::CTRL;
            } else if part.eq_ignore_ascii_case("shift") {
                modifiers |= Modifiers::SHIFT;
            } else if part.eq_ignore_ascii_case("alt") {
                modifiers |= Modifiers::ALT;
            } else if part.eq_ignore_ascii_case("super") || part.eq_ignore_ascii_case("win") {
                modifiers |= Modifiers::SUPER;
            } else if part.eq_ignore_ascii_case("iso_level3_shift")
                || part.eq_ignore_ascii_case("mod5")
            {
                modifiers |= Modifiers::ISO_LEVEL3_SHIFT;
            } else if part.eq_ignore_ascii_case("iso_level5_shift")
                || part.eq_ignore_ascii_case("mod3")
            {
                modifiers |= Modifiers::ISO_LEVEL5_SHIFT;
            } else {
                return Err(miette!("invalid modifier: {part}"));
            }
        }

        let trigger = if key.eq_ignore_ascii_case("WheelScrollDown") {
            Trigger::WheelScrollDown
        } else if key.eq_ignore_ascii_case("WheelScrollUp") {
            Trigger::WheelScrollUp
        } else if key.eq_ignore_ascii_case("WheelScrollLeft") {
            Trigger::WheelScrollLeft
        } else if key.eq_ignore_ascii_case("WheelScrollRight") {
            Trigger::WheelScrollRight
        } else if key.eq_ignore_ascii_case("TouchpadScrollDown") {
            Trigger::TouchpadScrollDown
        } else if key.eq_ignore_ascii_case("TouchpadScrollUp") {
            Trigger::TouchpadScrollUp
        } else if key.eq_ignore_ascii_case("TouchpadScrollLeft") {
            Trigger::TouchpadScrollLeft
        } else if key.eq_ignore_ascii_case("TouchpadScrollRight") {
            Trigger::TouchpadScrollRight
        } else {
            let keysym = keysym_from_name(key, KEYSYM_CASE_INSENSITIVE);
            if keysym.raw() == KEY_NoSymbol {
                return Err(miette!("invalid key: {key}"));
            }
            Trigger::Keysym(keysym)
        };

        Ok(Key { trigger, modifiers })
    }
}

impl FromStr for ClickMethod {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "clickfinger" => Ok(Self::Clickfinger),
            "button-areas" => Ok(Self::ButtonAreas),
            _ => Err(miette!(
                r#"invalid click method, can be "button-areas" or "clickfinger""#
            )),
        }
    }
}

impl FromStr for AccelProfile {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "adaptive" => Ok(Self::Adaptive),
            "flat" => Ok(Self::Flat),
            _ => Err(miette!(
                r#"invalid accel profile, can be "adaptive" or "flat""#
            )),
        }
    }
}

impl FromStr for ScrollMethod {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "no-scroll" => Ok(Self::NoScroll),
            "two-finger" => Ok(Self::TwoFinger),
            "edge" => Ok(Self::Edge),
            "on-button-down" => Ok(Self::OnButtonDown),
            _ => Err(miette!(
                r#"invalid scroll method, can be "no-scroll", "two-finger", "edge", or "on-button-down""#
            )),
        }
    }
}

impl FromStr for TapButtonMap {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "left-right-middle" => Ok(Self::LeftRightMiddle),
            "left-middle-right" => Ok(Self::LeftMiddleRight),
            _ => Err(miette!(
                r#"invalid tap button map, can be "left-right-middle" or "left-middle-right""#
            )),
        }
    }
}

impl FromStr for Percent {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((value, empty)) = s.split_once('%') else {
            return Err(miette!("value must end with '%'"));
        };

        if !empty.is_empty() {
            return Err(miette!("trailing characters after '%' are not allowed"));
        }

        let value: f64 = value.parse().map_err(|_| miette!("error parsing value"))?;
        Ok(Percent(value / 100.))
    }
}

pub fn set_miette_hook() -> Result<(), miette::InstallError> {
    miette::set_hook(Box::new(|_| Box::new(NarratableReportHandler::new())))
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};
    use niri_ipc::PositionChange;
    use pretty_assertions::assert_eq;

    use super::*;

    #[track_caller]
    fn check(text: &str, expected: Config) {
        let _ = set_miette_hook();

        let parsed = Config::parse("test.kdl", text)
            .map_err(miette::Report::new)
            .unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse() {
        check(
            r##"
            input {
                keyboard {
                    repeat-delay 600
                    repeat-rate 25
                    track-layout "window"
                    xkb {
                        layout "us,ru"
                        options "grp:win_space_toggle"
                    }
                }

                touchpad {
                    tap
                    dwt
                    dwtp
                    click-method "clickfinger"
                    accel-speed 0.2
                    accel-profile "flat"
                    scroll-method "two-finger"
                    scroll-button 272
                    tap-button-map "left-middle-right"
                    disabled-on-external-mouse
                    scroll-factor 0.9
                }

                mouse {
                    natural-scroll
                    accel-speed 0.4
                    accel-profile "flat"
                    scroll-method "no-scroll"
                    scroll-button 273
                    middle-emulation
                    scroll-factor 0.2
                }

                trackpoint {
                    off
                    natural-scroll
                    accel-speed 0.0
                    accel-profile "flat"
                    scroll-method "on-button-down"
                    scroll-button 274
                }

                trackball {
                    off
                    natural-scroll
                    accel-speed 0.0
                    accel-profile "flat"
                    scroll-method "edge"
                    scroll-button 275
                    left-handed
                    middle-emulation
                }

                tablet {
                    map-to-output "eDP-1"
                }

                touch {
                    map-to-output "eDP-1"
                }

                disable-power-key-handling

                warp-mouse-to-focus
                focus-follows-mouse
                workspace-auto-back-and-forth
            }

            output "eDP-1" {
                scale 2
                transform "flipped-90"
                position x=10 y=20
                mode "1920x1080@144"
                variable-refresh-rate on-demand=true
                background-color "rgba(25, 25, 102, 1.0)"
            }

            layout {
                focus-ring {
                    width 5
                    active-color 0 100 200 255
                    inactive-color 255 200 100 0
                    active-gradient from="rgba(10, 20, 30, 1.0)" to="#0080ffff" relative-to="workspace-view"
                }

                border {
                    width 3
                    inactive-color "rgba(255, 200, 100, 0.0)"
                }

                preset-column-widths {
                    proportion 0.25
                    proportion 0.5
                    fixed 960
                    fixed 1280
                }

                preset-window-heights {
                    proportion 0.25
                    proportion 0.5
                    fixed 960
                    fixed 1280
                }

                default-column-width { proportion 0.25; }

                gaps 8

                struts {
                    left 1
                    right 2
                    top 3
                }

                center-focused-column "on-overflow"

                insert-hint {
                    color "rgb(255, 200, 127)"
                    gradient from="rgba(10, 20, 30, 1.0)" to="#0080ffff" relative-to="workspace-view"
                }
            }

            spawn-at-startup "alacritty" "-e" "fish"

            prefer-no-csd

            cursor {
                xcursor-theme "breeze_cursors"
                xcursor-size 16
                hide-when-typing
                hide-after-inactive-ms 3000
            }

            screenshot-path "~/Screenshots/screenshot.png"

            hotkey-overlay {
                skip-at-startup
            }

            animations {
                slowdown 2.0

                workspace-switch {
                    spring damping-ratio=1.0 stiffness=1000 epsilon=0.0001
                }

                horizontal-view-movement {
                    duration-ms 100
                    curve "ease-out-expo"
                }

                window-open { off; }
            }

            environment {
                QT_QPA_PLATFORM "wayland"
                DISPLAY null
            }

            window-rule {
                match app-id=".*alacritty"
                exclude title="~"
                exclude is-active=true is-focused=false

                open-on-output "eDP-1"
                open-maximized true
                open-fullscreen false
                open-floating false
                open-focused true
                default-window-height { fixed 500; }
                default-floating-position x=100 y=-200 relative-to="bottom-left"

                focus-ring {
                    off
                    width 3
                }

                border {
                    on
                    width 8.5
                }
            }

            layer-rule {
                match namespace="^notifications$"
                block-out-from "screencast"
            }

            binds {
                Mod+T allow-when-locked=true { spawn "alacritty"; }
                Mod+Q { close-window; }
                Mod+Shift+H { focus-monitor-left; }
                Mod+Ctrl+Shift+L { move-window-to-monitor-right; }
                Mod+Comma { consume-window-into-column; }
                Mod+1 { focus-workspace 1; }
                Mod+Shift+1 { focus-workspace "workspace-1"; }
                Mod+Shift+E { quit skip-confirmation=true; }
                Mod+WheelScrollDown cooldown-ms=150 { focus-workspace-down; }
            }

            switch-events {
                tablet-mode-on { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled true"; }
                tablet-mode-off { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled false"; }
            }

            debug {
                render-drm-device "/dev/dri/renderD129"
            }

            workspace "workspace-1" {
                open-on-output "eDP-1"
            }
            workspace "workspace-2"
            workspace "workspace-3"
            "##,
            Config {
                input: Input {
                    keyboard: Keyboard {
                        xkb: Xkb {
                            layout: "us,ru".to_owned(),
                            options: Some("grp:win_space_toggle".to_owned()),
                            ..Default::default()
                        },
                        repeat_delay: 600,
                        repeat_rate: 25,
                        track_layout: TrackLayout::Window,
                    },
                    touchpad: Touchpad {
                        off: false,
                        tap: true,
                        dwt: true,
                        dwtp: true,
                        click_method: Some(ClickMethod::Clickfinger),
                        natural_scroll: false,
                        accel_speed: 0.2,
                        accel_profile: Some(AccelProfile::Flat),
                        scroll_method: Some(ScrollMethod::TwoFinger),
                        scroll_button: Some(272),
                        tap_button_map: Some(TapButtonMap::LeftMiddleRight),
                        left_handed: false,
                        disabled_on_external_mouse: true,
                        middle_emulation: false,
                        scroll_factor: Some(FloatOrInt(0.9)),
                    },
                    mouse: Mouse {
                        off: false,
                        natural_scroll: true,
                        accel_speed: 0.4,
                        accel_profile: Some(AccelProfile::Flat),
                        scroll_method: Some(ScrollMethod::NoScroll),
                        scroll_button: Some(273),
                        left_handed: false,
                        middle_emulation: true,
                        scroll_factor: Some(FloatOrInt(0.2)),
                    },
                    trackpoint: Trackpoint {
                        off: true,
                        natural_scroll: true,
                        accel_speed: 0.0,
                        accel_profile: Some(AccelProfile::Flat),
                        scroll_method: Some(ScrollMethod::OnButtonDown),
                        scroll_button: Some(274),
                        middle_emulation: false,
                    },
                    trackball: Trackball {
                        off: true,
                        natural_scroll: true,
                        accel_speed: 0.0,
                        accel_profile: Some(AccelProfile::Flat),
                        scroll_method: Some(ScrollMethod::Edge),
                        scroll_button: Some(275),
                        left_handed: true,
                        middle_emulation: true,
                    },
                    tablet: Tablet {
                        off: false,
                        map_to_output: Some("eDP-1".to_owned()),
                        left_handed: false,
                    },
                    touch: Touch {
                        map_to_output: Some("eDP-1".to_owned()),
                    },
                    disable_power_key_handling: true,
                    warp_mouse_to_focus: true,
                    focus_follows_mouse: Some(FocusFollowsMouse {
                        max_scroll_amount: None,
                    }),
                    workspace_auto_back_and_forth: true,
                },
                outputs: Outputs(vec![Output {
                    off: false,
                    name: "eDP-1".to_owned(),
                    scale: Some(FloatOrInt(2.)),
                    transform: Transform::Flipped90,
                    position: Some(Position { x: 10, y: 20 }),
                    mode: Some(ConfiguredMode {
                        width: 1920,
                        height: 1080,
                        refresh: Some(144.),
                    }),
                    variable_refresh_rate: Some(Vrr { on_demand: true }),
                    background_color: Color::from_rgba8_unpremul(25, 25, 102, 255),
                }]),
                layout: Layout {
                    focus_ring: FocusRing {
                        off: false,
                        width: FloatOrInt(5.),
                        active_color: Color::from_rgba8_unpremul(0, 100, 200, 255),
                        inactive_color: Color::from_rgba8_unpremul(255, 200, 100, 0),
                        active_gradient: Some(Gradient {
                            from: Color::from_rgba8_unpremul(10, 20, 30, 255),
                            to: Color::from_rgba8_unpremul(0, 128, 255, 255),
                            angle: 180,
                            relative_to: GradientRelativeTo::WorkspaceView,
                            in_: GradientInterpolation {
                                color_space: GradientColorSpace::Srgb,
                                hue_interpolation: HueInterpolation::Shorter,
                            },
                        }),
                        inactive_gradient: None,
                    },
                    border: Border {
                        off: false,
                        width: FloatOrInt(3.),
                        active_color: Color::from_rgba8_unpremul(255, 200, 127, 255),
                        inactive_color: Color::from_rgba8_unpremul(255, 200, 100, 0),
                        active_gradient: None,
                        inactive_gradient: None,
                    },
                    insert_hint: InsertHint {
                        off: false,
                        color: Color::from_rgba8_unpremul(255, 200, 127, 255),
                        gradient: Some(Gradient {
                            from: Color::from_rgba8_unpremul(10, 20, 30, 255),
                            to: Color::from_rgba8_unpremul(0, 128, 255, 255),
                            angle: 180,
                            relative_to: GradientRelativeTo::WorkspaceView,
                            in_: GradientInterpolation {
                                color_space: GradientColorSpace::Srgb,
                                hue_interpolation: HueInterpolation::Shorter,
                            },
                        }),
                    },
                    preset_column_widths: vec![
                        PresetSize::Proportion(0.25),
                        PresetSize::Proportion(0.5),
                        PresetSize::Fixed(960),
                        PresetSize::Fixed(1280),
                    ],
                    default_column_width: Some(DefaultPresetSize(Some(PresetSize::Proportion(
                        0.25,
                    )))),
                    preset_window_heights: vec![
                        PresetSize::Proportion(0.25),
                        PresetSize::Proportion(0.5),
                        PresetSize::Fixed(960),
                        PresetSize::Fixed(1280),
                    ],
                    gaps: FloatOrInt(8.),
                    struts: Struts {
                        left: FloatOrInt(1.),
                        right: FloatOrInt(2.),
                        top: FloatOrInt(3.),
                        bottom: FloatOrInt(0.),
                    },
                    center_focused_column: CenterFocusedColumn::OnOverflow,
                    always_center_single_column: false,
                    empty_workspace_above_first: false,
                },
                spawn_at_startup: vec![SpawnAtStartup {
                    command: vec!["alacritty".to_owned(), "-e".to_owned(), "fish".to_owned()],
                }],
                prefer_no_csd: true,
                cursor: Cursor {
                    xcursor_theme: String::from("breeze_cursors"),
                    xcursor_size: 16,
                    hide_when_typing: true,
                    hide_after_inactive_ms: Some(3000),
                },
                screenshot_path: Some(String::from("~/Screenshots/screenshot.png")),
                hotkey_overlay: HotkeyOverlay {
                    skip_at_startup: true,
                },
                animations: Animations {
                    slowdown: 2.,
                    workspace_switch: WorkspaceSwitchAnim(Animation {
                        off: false,
                        kind: AnimationKind::Spring(SpringParams {
                            damping_ratio: 1.,
                            stiffness: 1000,
                            epsilon: 0.0001,
                        }),
                    }),
                    horizontal_view_movement: HorizontalViewMovementAnim(Animation {
                        off: false,
                        kind: AnimationKind::Easing(EasingParams {
                            duration_ms: 100,
                            curve: AnimationCurve::EaseOutExpo,
                        }),
                    }),
                    window_open: WindowOpenAnim {
                        anim: Animation {
                            off: true,
                            ..WindowOpenAnim::default().anim
                        },
                        custom_shader: None,
                    },
                    ..Default::default()
                },
                environment: Environment(vec![
                    EnvironmentVariable {
                        name: String::from("QT_QPA_PLATFORM"),
                        value: Some(String::from("wayland")),
                    },
                    EnvironmentVariable {
                        name: String::from("DISPLAY"),
                        value: None,
                    },
                ]),
                window_rules: vec![WindowRule {
                    matches: vec![Match {
                        app_id: Some(RegexEq::from_str(".*alacritty").unwrap()),
                        title: None,
                        is_active: None,
                        is_focused: None,
                        is_active_in_column: None,
                        is_floating: None,
                        at_startup: None,
                    }],
                    excludes: vec![
                        Match {
                            app_id: None,
                            title: Some(RegexEq::from_str("~").unwrap()),
                            is_active: None,
                            is_focused: None,
                            is_active_in_column: None,
                            is_floating: None,
                            at_startup: None,
                        },
                        Match {
                            app_id: None,
                            title: None,
                            is_active: Some(true),
                            is_focused: Some(false),
                            is_active_in_column: None,
                            is_floating: None,
                            at_startup: None,
                        },
                    ],
                    open_on_output: Some("eDP-1".to_owned()),
                    open_maximized: Some(true),
                    open_fullscreen: Some(false),
                    open_floating: Some(false),
                    open_focused: Some(true),
                    default_window_height: Some(DefaultPresetSize(Some(PresetSize::Fixed(500)))),
                    default_floating_position: Some(FoIPosition {
                        x: FloatOrInt(100.),
                        y: FloatOrInt(-200.),
                        relative_to: RelativeTo::BottomLeft,
                    }),
                    focus_ring: BorderRule {
                        off: true,
                        width: Some(FloatOrInt(3.)),
                        ..Default::default()
                    },
                    border: BorderRule {
                        on: true,
                        width: Some(FloatOrInt(8.5)),
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                layer_rules: vec![
                    LayerRule {
                        matches: vec![layer_rule::Match {
                            namespace: Some(RegexEq::from_str("^notifications$").unwrap()),
                            at_startup: None,
                        }],
                        excludes: vec![],
                        opacity: None,
                        block_out_from: Some(BlockOutFrom::Screencast),
                    }
                ],
                workspaces: vec![
                    Workspace {
                        name: WorkspaceName("workspace-1".to_string()),
                        open_on_output: Some("eDP-1".to_string()),
                    },
                    Workspace {
                        name: WorkspaceName("workspace-2".to_string()),
                        open_on_output: None,
                    },
                    Workspace {
                        name: WorkspaceName("workspace-3".to_string()),
                        open_on_output: None,
                    },
                ],
                binds: Binds(vec![
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::t),
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        action: Action::Spawn(vec!["alacritty".to_owned()]),
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: true,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::q),
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        action: Action::CloseWindow,
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: false,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::h),
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT,
                        },
                        action: Action::FocusMonitorLeft,
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: false,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::l),
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT | Modifiers::CTRL,
                        },
                        action: Action::MoveWindowToMonitorRight,
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: false,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::comma),
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        action: Action::ConsumeWindowIntoColumn,
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: false,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::_1),
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        action: Action::FocusWorkspace(WorkspaceReference::Index(1)),
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: false,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::_1),
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT,
                        },
                        action: Action::FocusWorkspace(WorkspaceReference::Name(
                            "workspace-1".to_string(),
                        )),
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: false,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::Keysym(Keysym::e),
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT,
                        },
                        action: Action::Quit(true),
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: false,
                    },
                    Bind {
                        key: Key {
                            trigger: Trigger::WheelScrollDown,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        action: Action::FocusWorkspaceDown,
                        repeat: true,
                        cooldown: Some(Duration::from_millis(150)),
                        allow_when_locked: false,
                    },
                ]),
                switch_events: SwitchBinds {
                    lid_open: None,
                    lid_close: None,
                    tablet_mode_on: Some(SwitchAction {
                        spawn: vec![
                            "bash".to_owned(),
                            "-c".to_owned(),
                            "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled true".to_owned(),
                        ],
                    }),
                    tablet_mode_off: Some(SwitchAction {
                        spawn: vec![
                            "bash".to_owned(),
                            "-c".to_owned(),
                            "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled false".to_owned(),
                        ],
                    }),
                },
                debug: DebugConfig {
                    render_drm_device: Some(PathBuf::from("/dev/dri/renderD129")),
                    ..Default::default()
                },
            },
        );
    }

    #[test]
    fn can_create_default_config() {
        let _ = Config::default();
    }

    #[test]
    fn parse_mode() {
        assert_eq!(
            "2560x1600@165.004".parse::<ConfiguredMode>().unwrap(),
            ConfiguredMode {
                width: 2560,
                height: 1600,
                refresh: Some(165.004),
            },
        );

        assert_eq!(
            "1920x1080".parse::<ConfiguredMode>().unwrap(),
            ConfiguredMode {
                width: 1920,
                height: 1080,
                refresh: None,
            },
        );

        assert!("1920".parse::<ConfiguredMode>().is_err());
        assert!("1920x".parse::<ConfiguredMode>().is_err());
        assert!("1920x1080@".parse::<ConfiguredMode>().is_err());
        assert!("1920x1080@60Hz".parse::<ConfiguredMode>().is_err());
    }

    #[test]
    fn parse_size_change() {
        assert_eq!(
            "10".parse::<SizeChange>().unwrap(),
            SizeChange::SetFixed(10),
        );
        assert_eq!(
            "+10".parse::<SizeChange>().unwrap(),
            SizeChange::AdjustFixed(10),
        );
        assert_eq!(
            "-10".parse::<SizeChange>().unwrap(),
            SizeChange::AdjustFixed(-10),
        );
        assert_eq!(
            "10%".parse::<SizeChange>().unwrap(),
            SizeChange::SetProportion(10.),
        );
        assert_eq!(
            "+10%".parse::<SizeChange>().unwrap(),
            SizeChange::AdjustProportion(10.),
        );
        assert_eq!(
            "-10%".parse::<SizeChange>().unwrap(),
            SizeChange::AdjustProportion(-10.),
        );

        assert!("-".parse::<SizeChange>().is_err());
        assert!("10% ".parse::<SizeChange>().is_err());
    }

    #[test]
    fn parse_position_change() {
        assert_eq!(
            "10".parse::<PositionChange>().unwrap(),
            PositionChange::SetFixed(10.),
        );
        assert_eq!(
            "+10".parse::<PositionChange>().unwrap(),
            PositionChange::AdjustFixed(10.),
        );
        assert_eq!(
            "-10".parse::<PositionChange>().unwrap(),
            PositionChange::AdjustFixed(-10.),
        );

        assert!("10%".parse::<PositionChange>().is_err());
        assert!("+10%".parse::<PositionChange>().is_err());
        assert!("-10%".parse::<PositionChange>().is_err());
        assert!("-".parse::<PositionChange>().is_err());
        assert!("10% ".parse::<PositionChange>().is_err());
    }

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
    fn parse_iso_level_shifts() {
        assert_eq!(
            "ISO_Level3_Shift+A".parse::<Key>().unwrap(),
            Key {
                trigger: Trigger::Keysym(Keysym::a),
                modifiers: Modifiers::ISO_LEVEL3_SHIFT
            },
        );
        assert_eq!(
            "Mod5+A".parse::<Key>().unwrap(),
            Key {
                trigger: Trigger::Keysym(Keysym::a),
                modifiers: Modifiers::ISO_LEVEL3_SHIFT
            },
        );

        assert_eq!(
            "ISO_Level5_Shift+A".parse::<Key>().unwrap(),
            Key {
                trigger: Trigger::Keysym(Keysym::a),
                modifiers: Modifiers::ISO_LEVEL5_SHIFT
            },
        );
        assert_eq!(
            "Mod3+A".parse::<Key>().unwrap(),
            Key {
                trigger: Trigger::Keysym(Keysym::a),
                modifiers: Modifiers::ISO_LEVEL5_SHIFT
            },
        );
    }

    #[test]
    fn default_repeat_params() {
        let config = Config::parse("config.kdl", "").unwrap();
        assert_eq!(config.input.keyboard.repeat_delay, 600);
        assert_eq!(config.input.keyboard.repeat_rate, 25);
    }

    fn make_output_name(
        connector: &str,
        make: Option<&str>,
        model: Option<&str>,
        serial: Option<&str>,
    ) -> OutputName {
        OutputName {
            connector: connector.to_string(),
            make: make.map(|x| x.to_string()),
            model: model.map(|x| x.to_string()),
            serial: serial.map(|x| x.to_string()),
        }
    }

    #[test]
    fn test_output_name_match() {
        fn check(
            target: &str,
            connector: &str,
            make: Option<&str>,
            model: Option<&str>,
            serial: Option<&str>,
        ) -> bool {
            let name = make_output_name(connector, make, model, serial);
            name.matches(target)
        }

        assert!(check("dp-2", "DP-2", None, None, None));
        assert!(!check("dp-1", "DP-2", None, None, None));
        assert!(check("dp-2", "DP-2", Some("a"), Some("b"), Some("c")));
        assert!(check(
            "some company some monitor 1234",
            "DP-2",
            Some("Some Company"),
            Some("Some Monitor"),
            Some("1234")
        ));
        assert!(!check(
            "some other company some monitor 1234",
            "DP-2",
            Some("Some Company"),
            Some("Some Monitor"),
            Some("1234")
        ));
        assert!(!check(
            "make model serial ",
            "DP-2",
            Some("make"),
            Some("model"),
            Some("serial")
        ));
        assert!(check(
            "make  serial",
            "DP-2",
            Some("make"),
            Some(""),
            Some("serial")
        ));
        assert!(check(
            "make model unknown",
            "DP-2",
            Some("Make"),
            Some("Model"),
            None
        ));
        assert!(check(
            "unknown unknown serial",
            "DP-2",
            None,
            None,
            Some("Serial")
        ));
        assert!(!check("unknown unknown unknown", "DP-2", None, None, None));
    }

    #[test]
    fn test_output_name_sorting() {
        let mut names = vec![
            make_output_name("DP-2", None, None, None),
            make_output_name("DP-1", None, None, None),
            make_output_name("DP-3", Some("B"), Some("A"), Some("A")),
            make_output_name("DP-3", Some("A"), Some("B"), Some("A")),
            make_output_name("DP-3", Some("A"), Some("A"), Some("B")),
            make_output_name("DP-3", None, Some("A"), Some("A")),
            make_output_name("DP-3", Some("A"), None, Some("A")),
            make_output_name("DP-3", Some("A"), Some("A"), None),
            make_output_name("DP-5", Some("A"), Some("A"), Some("A")),
            make_output_name("DP-4", Some("A"), Some("A"), Some("A")),
        ];
        names.sort_by(|a, b| a.compare(b));
        let names = names
            .into_iter()
            .map(|name| {
                format!(
                    "{} | {}",
                    name.format_make_model_serial_or_connector(),
                    name.connector,
                )
            })
            .collect::<Vec<_>>();
        assert_debug_snapshot!(
            names,
            @r#"
[
    "Unknown A A | DP-3",
    "A Unknown A | DP-3",
    "A A Unknown | DP-3",
    "A A A | DP-4",
    "A A A | DP-5",
    "A A B | DP-3",
    "A B A | DP-3",
    "B A A | DP-3",
    "DP-1 | DP-1",
    "DP-2 | DP-2",
]
"#
        );
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
                active_gradient: None,
                inactive_gradient: None,
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
}
