#[macro_use]
extern crate tracing;

use std::path::{Path, PathBuf};
use std::str::FromStr;

use bitflags::bitflags;
use miette::{miette, Context, IntoDiagnostic, NarratableReportHandler};
use niri_ipc::{LayoutSwitchTarget, SizeChange};
use regex::Regex;
use smithay::input::keyboard::keysyms::KEY_NoSymbol;
use smithay::input::keyboard::xkb::{keysym_from_name, KEYSYM_CASE_INSENSITIVE};
use smithay::input::keyboard::{Keysym, XkbConfig};
use smithay::reexports::input;

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Config {
    #[knuffel(child, default)]
    pub input: Input,
    #[knuffel(children(name = "output"))]
    pub outputs: Vec<Output>,
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
    #[knuffel(children(name = "window-rule"))]
    pub window_rules: Vec<WindowRule>,
    #[knuffel(child, default)]
    pub binds: Binds,
    #[knuffel(child, default)]
    pub debug: DebugConfig,
}

// FIXME: Add other devices.
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
    pub tablet: Tablet,
    #[knuffel(child)]
    pub disable_power_key_handling: bool,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq, Eq)]
pub struct Keyboard {
    #[knuffel(child, default)]
    pub xkb: Xkb,
    // The defaults were chosen to match wlroots and sway.
    #[knuffel(child, unwrap(argument), default = 600)]
    pub repeat_delay: u16,
    #[knuffel(child, unwrap(argument), default = 25)]
    pub repeat_rate: u8,
    #[knuffel(child, unwrap(argument), default)]
    pub track_layout: TrackLayout,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq, Eq, Clone)]
pub struct Xkb {
    #[knuffel(child, unwrap(argument), default)]
    pub rules: String,
    #[knuffel(child, unwrap(argument), default)]
    pub model: String,
    #[knuffel(child, unwrap(argument))]
    pub layout: Option<String>,
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
            layout: self.layout.as_deref().unwrap_or("us"),
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

// FIXME: Add the rest of the settings.
#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Touchpad {
    #[knuffel(child)]
    pub tap: bool,
    #[knuffel(child)]
    pub dwt: bool,
    #[knuffel(child)]
    pub dwtp: bool,
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
    #[knuffel(child, unwrap(argument, str))]
    pub accel_profile: Option<AccelProfile>,
    #[knuffel(child, unwrap(argument, str))]
    pub tap_button_map: Option<TapButtonMap>,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Mouse {
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
    #[knuffel(child, unwrap(argument, str))]
    pub accel_profile: Option<AccelProfile>,
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Trackpoint {
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
    #[knuffel(child, unwrap(argument, str))]
    pub accel_profile: Option<AccelProfile>,
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
    #[knuffel(child, unwrap(argument))]
    pub map_to_output: Option<String>,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Output {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(child, unwrap(argument), default = 1.)]
    pub scale: f64,
    #[knuffel(child, unwrap(argument, str), default = Transform::Normal)]
    pub transform: Transform,
    #[knuffel(child)]
    pub position: Option<Position>,
    #[knuffel(child, unwrap(argument, str))]
    pub mode: Option<Mode>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            off: false,
            name: String::new(),
            scale: 1.,
            transform: Transform::Normal,
            position: None,
            mode: None,
        }
    }
}

/// Output transform, which goes counter-clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    Normal,
    _90,
    _180,
    _270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl FromStr for Transform {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "normal" => Ok(Self::Normal),
            "90" => Ok(Self::_90),
            "180" => Ok(Self::_180),
            "270" => Ok(Self::_270),
            "flipped" => Ok(Self::Flipped),
            "flipped-90" => Ok(Self::Flipped90),
            "flipped-180" => Ok(Self::Flipped180),
            "flipped-270" => Ok(Self::Flipped270),
            _ => Err(miette!(concat!(
                r#"invalid transform, can be "90", "180", "270", "#,
                r#""flipped", "flipped-90", "flipped-180" or "flipped-270""#
            ))),
        }
    }
}

impl From<Transform> for smithay::utils::Transform {
    fn from(value: Transform) -> Self {
        match value {
            Transform::Normal => Self::Normal,
            Transform::_90 => Self::_90,
            Transform::_180 => Self::_180,
            Transform::_270 => Self::_270,
            Transform::Flipped => Self::Flipped,
            Transform::Flipped90 => Self::Flipped90,
            Transform::Flipped180 => Self::Flipped180,
            Transform::Flipped270 => Self::Flipped270,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    #[knuffel(property)]
    pub x: i32,
    #[knuffel(property)]
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub width: u16,
    pub height: u16,
    pub refresh: Option<f64>,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Layout {
    #[knuffel(child, default)]
    pub focus_ring: FocusRing,
    #[knuffel(child, default)]
    pub border: Border,
    #[knuffel(child, unwrap(children), default)]
    pub preset_column_widths: Vec<PresetWidth>,
    #[knuffel(child)]
    pub default_column_width: Option<DefaultColumnWidth>,
    #[knuffel(child, unwrap(argument), default)]
    pub center_focused_column: CenterFocusedColumn,
    #[knuffel(child, unwrap(argument), default = Self::default().gaps)]
    pub gaps: u16,
    #[knuffel(child, default)]
    pub struts: Struts,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            focus_ring: Default::default(),
            border: Default::default(),
            preset_column_widths: Default::default(),
            default_column_width: Default::default(),
            center_focused_column: Default::default(),
            gaps: 16,
            struts: Default::default(),
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
    pub width: u16,
    #[knuffel(child, default = Self::default().active_color)]
    pub active_color: Color,
    #[knuffel(child, default = Self::default().inactive_color)]
    pub inactive_color: Color,
}

impl Default for FocusRing {
    fn default() -> Self {
        Self {
            off: false,
            width: 4,
            active_color: Color::new(127, 200, 255, 255),
            inactive_color: Color::new(80, 80, 80, 255),
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Border {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = Self::default().width)]
    pub width: u16,
    #[knuffel(child, default = Self::default().active_color)]
    pub active_color: Color,
    #[knuffel(child, default = Self::default().inactive_color)]
    pub inactive_color: Color,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            off: true,
            width: 4,
            active_color: Color::new(255, 200, 127, 255),
            inactive_color: Color::new(80, 80, 80, 255),
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
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    #[knuffel(argument)]
    pub r: u8,
    #[knuffel(argument)]
    pub g: u8,
    #[knuffel(argument)]
    pub b: u8,
    #[knuffel(argument)]
    pub a: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        let [r, g, b, a] = [c.r, c.g, c.b, c.a].map(|x| x as f32 / 255.);
        [r * a, g * a, b * a, a]
    }
}

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Cursor {
    #[knuffel(child, unwrap(argument), default = String::from("default"))]
    pub xcursor_theme: String,
    #[knuffel(child, unwrap(argument), default = 24)]
    pub xcursor_size: u8,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            xcursor_theme: String::from("default"),
            xcursor_size: 24,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub enum PresetWidth {
    Proportion(#[knuffel(argument)] f64),
    Fixed(#[knuffel(argument)] i32),
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct DefaultColumnWidth(#[knuffel(children)] pub Vec<PresetWidth>);

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Struts {
    #[knuffel(child, unwrap(argument), default)]
    pub left: u16,
    #[knuffel(child, unwrap(argument), default)]
    pub right: u16,
    #[knuffel(child, unwrap(argument), default)]
    pub top: u16,
    #[knuffel(child, unwrap(argument), default)]
    pub bottom: u16,
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyOverlay {
    #[knuffel(child)]
    pub skip_at_startup: bool,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Animations {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = 1.)]
    pub slowdown: f64,
    #[knuffel(child, default = Animation::default_workspace_switch())]
    pub workspace_switch: Animation,
    #[knuffel(child, default = Animation::default_horizontal_view_movement())]
    pub horizontal_view_movement: Animation,
    #[knuffel(child, default = Animation::default_window_open())]
    pub window_open: Animation,
    #[knuffel(child, default = Animation::default_config_notification_open_close())]
    pub config_notification_open_close: Animation,
}

impl Default for Animations {
    fn default() -> Self {
        Self {
            off: false,
            slowdown: 1.,
            workspace_switch: Animation::default_workspace_switch(),
            horizontal_view_movement: Animation::default_horizontal_view_movement(),
            window_open: Animation::default_window_open(),
            config_notification_open_close: Animation::default_config_notification_open_close(),
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Animation {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument))]
    pub duration_ms: Option<u32>,
    #[knuffel(child, unwrap(argument))]
    pub curve: Option<AnimationCurve>,
}

impl Animation {
    pub const fn unfilled() -> Self {
        Self {
            off: false,
            duration_ms: None,
            curve: None,
        }
    }

    pub const fn default() -> Self {
        Self {
            off: false,
            duration_ms: Some(250),
            curve: Some(AnimationCurve::EaseOutCubic),
        }
    }

    pub const fn default_workspace_switch() -> Self {
        Self::default()
    }

    pub const fn default_horizontal_view_movement() -> Self {
        Self::default()
    }

    pub const fn default_config_notification_open_close() -> Self {
        Self::default()
    }

    pub const fn default_window_open() -> Self {
        Self {
            duration_ms: Some(150),
            curve: Some(AnimationCurve::EaseOutExpo),
            ..Self::default()
        }
    }
}

#[derive(knuffel::DecodeScalar, Debug, Clone, Copy, PartialEq)]
pub enum AnimationCurve {
    EaseOutCubic,
    EaseOutExpo,
}

#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq)]
pub struct WindowRule {
    #[knuffel(children(name = "match"))]
    pub matches: Vec<Match>,
    #[knuffel(children(name = "exclude"))]
    pub excludes: Vec<Match>,

    #[knuffel(child)]
    pub default_column_width: Option<DefaultColumnWidth>,
    #[knuffel(child, unwrap(argument))]
    pub open_on_output: Option<String>,
}

#[derive(knuffel::Decode, Debug, Default, Clone)]
pub struct Match {
    #[knuffel(property, str)]
    pub app_id: Option<Regex>,
    #[knuffel(property, str)]
    pub title: Option<Regex>,
}

impl PartialEq for Match {
    fn eq(&self, other: &Self) -> bool {
        self.app_id.as_ref().map(Regex::as_str) == other.app_id.as_ref().map(Regex::as_str)
            && self.title.as_ref().map(Regex::as_str) == other.title.as_ref().map(Regex::as_str)
    }
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Binds(#[knuffel(children)] pub Vec<Bind>);

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Bind {
    #[knuffel(node_name)]
    pub key: Key,
    #[knuffel(children)]
    pub actions: Vec<Action>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Key {
    pub keysym: Keysym,
    pub modifiers: Modifiers,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Modifiers : u8 {
        const CTRL = 1;
        const SHIFT = 2;
        const ALT = 4;
        const SUPER = 8;
        const COMPOSITOR = 16;
    }
}

// Remember to add new actions to the CLI enum too.
#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub enum Action {
    Quit(#[knuffel(property(name = "skip-confirmation"), default)] bool),
    #[knuffel(skip)]
    ChangeVt(i32),
    Suspend,
    PowerOffMonitors,
    ToggleDebugTint,
    Spawn(#[knuffel(arguments)] Vec<String>),
    #[knuffel(skip)]
    ConfirmScreenshot,
    #[knuffel(skip)]
    CancelScreenshot,
    Screenshot,
    ScreenshotScreen,
    ScreenshotWindow,
    CloseWindow,
    FullscreenWindow,
    FocusColumnLeft,
    FocusColumnRight,
    FocusColumnFirst,
    FocusColumnLast,
    FocusWindowDown,
    FocusWindowUp,
    FocusWindowOrWorkspaceDown,
    FocusWindowOrWorkspaceUp,
    MoveColumnLeft,
    MoveColumnRight,
    MoveColumnToFirst,
    MoveColumnToLast,
    MoveWindowDown,
    MoveWindowUp,
    MoveWindowDownOrToWorkspaceDown,
    MoveWindowUpOrToWorkspaceUp,
    ConsumeOrExpelWindowLeft,
    ConsumeOrExpelWindowRight,
    ConsumeWindowIntoColumn,
    ExpelWindowFromColumn,
    CenterColumn,
    FocusWorkspaceDown,
    FocusWorkspaceUp,
    FocusWorkspace(#[knuffel(argument)] u8),
    MoveWindowToWorkspaceDown,
    MoveWindowToWorkspaceUp,
    MoveWindowToWorkspace(#[knuffel(argument)] u8),
    MoveColumnToWorkspaceDown,
    MoveColumnToWorkspaceUp,
    MoveColumnToWorkspace(#[knuffel(argument)] u8),
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
    SetWindowHeight(#[knuffel(argument, str)] SizeChange),
    SwitchPresetColumnWidth,
    MaximizeColumn,
    SetColumnWidth(#[knuffel(argument, str)] SizeChange),
    SwitchLayout(#[knuffel(argument, str)] LayoutSwitchTarget),
    ShowHotkeyOverlay,
    MoveWorkspaceToMonitorLeft,
    MoveWorkspaceToMonitorRight,
    MoveWorkspaceToMonitorDown,
    MoveWorkspaceToMonitorUp,
}

impl From<niri_ipc::Action> for Action {
    fn from(value: niri_ipc::Action) -> Self {
        match value {
            niri_ipc::Action::Quit { skip_confirmation } => Self::Quit(skip_confirmation),
            niri_ipc::Action::PowerOffMonitors => Self::PowerOffMonitors,
            niri_ipc::Action::Spawn { command } => Self::Spawn(command),
            niri_ipc::Action::Screenshot => Self::Screenshot,
            niri_ipc::Action::ScreenshotScreen => Self::ScreenshotScreen,
            niri_ipc::Action::ScreenshotWindow => Self::ScreenshotWindow,
            niri_ipc::Action::CloseWindow => Self::CloseWindow,
            niri_ipc::Action::FullscreenWindow => Self::FullscreenWindow,
            niri_ipc::Action::FocusColumnLeft => Self::FocusColumnLeft,
            niri_ipc::Action::FocusColumnRight => Self::FocusColumnRight,
            niri_ipc::Action::FocusColumnFirst => Self::FocusColumnFirst,
            niri_ipc::Action::FocusColumnLast => Self::FocusColumnLast,
            niri_ipc::Action::FocusWindowDown => Self::FocusWindowDown,
            niri_ipc::Action::FocusWindowUp => Self::FocusWindowUp,
            niri_ipc::Action::FocusWindowOrWorkspaceDown => Self::FocusWindowOrWorkspaceDown,
            niri_ipc::Action::FocusWindowOrWorkspaceUp => Self::FocusWindowOrWorkspaceUp,
            niri_ipc::Action::MoveColumnLeft => Self::MoveColumnLeft,
            niri_ipc::Action::MoveColumnRight => Self::MoveColumnRight,
            niri_ipc::Action::MoveColumnToFirst => Self::MoveColumnToFirst,
            niri_ipc::Action::MoveColumnToLast => Self::MoveColumnToLast,
            niri_ipc::Action::MoveWindowDown => Self::MoveWindowDown,
            niri_ipc::Action::MoveWindowUp => Self::MoveWindowUp,
            niri_ipc::Action::MoveWindowDownOrToWorkspaceDown => {
                Self::MoveWindowDownOrToWorkspaceDown
            }
            niri_ipc::Action::MoveWindowUpOrToWorkspaceUp => Self::MoveWindowUpOrToWorkspaceUp,
            niri_ipc::Action::ConsumeOrExpelWindowLeft => Self::ConsumeOrExpelWindowLeft,
            niri_ipc::Action::ConsumeOrExpelWindowRight => Self::ConsumeOrExpelWindowRight,
            niri_ipc::Action::ConsumeWindowIntoColumn => Self::ConsumeWindowIntoColumn,
            niri_ipc::Action::ExpelWindowFromColumn => Self::ExpelWindowFromColumn,
            niri_ipc::Action::CenterColumn => Self::CenterColumn,
            niri_ipc::Action::FocusWorkspaceDown => Self::FocusWorkspaceDown,
            niri_ipc::Action::FocusWorkspaceUp => Self::FocusWorkspaceUp,
            niri_ipc::Action::FocusWorkspace { index } => Self::FocusWorkspace(index),
            niri_ipc::Action::MoveWindowToWorkspaceDown => Self::MoveWindowToWorkspaceDown,
            niri_ipc::Action::MoveWindowToWorkspaceUp => Self::MoveWindowToWorkspaceUp,
            niri_ipc::Action::MoveWindowToWorkspace { index } => Self::MoveWindowToWorkspace(index),
            niri_ipc::Action::MoveColumnToWorkspaceDown => Self::MoveColumnToWorkspaceDown,
            niri_ipc::Action::MoveColumnToWorkspaceUp => Self::MoveColumnToWorkspaceUp,
            niri_ipc::Action::MoveColumnToWorkspace { index } => Self::MoveColumnToWorkspace(index),
            niri_ipc::Action::MoveWorkspaceDown => Self::MoveWorkspaceDown,
            niri_ipc::Action::MoveWorkspaceUp => Self::MoveWorkspaceUp,
            niri_ipc::Action::FocusMonitorLeft => Self::FocusMonitorLeft,
            niri_ipc::Action::FocusMonitorRight => Self::FocusMonitorRight,
            niri_ipc::Action::FocusMonitorDown => Self::FocusMonitorDown,
            niri_ipc::Action::FocusMonitorUp => Self::FocusMonitorUp,
            niri_ipc::Action::MoveWindowToMonitorLeft => Self::MoveWindowToMonitorLeft,
            niri_ipc::Action::MoveWindowToMonitorRight => Self::MoveWindowToMonitorRight,
            niri_ipc::Action::MoveWindowToMonitorDown => Self::MoveWindowToMonitorDown,
            niri_ipc::Action::MoveWindowToMonitorUp => Self::MoveWindowToMonitorUp,
            niri_ipc::Action::MoveColumnToMonitorLeft => Self::MoveColumnToMonitorLeft,
            niri_ipc::Action::MoveColumnToMonitorRight => Self::MoveColumnToMonitorRight,
            niri_ipc::Action::MoveColumnToMonitorDown => Self::MoveColumnToMonitorDown,
            niri_ipc::Action::MoveColumnToMonitorUp => Self::MoveColumnToMonitorUp,
            niri_ipc::Action::SetWindowHeight { change } => Self::SetWindowHeight(change),
            niri_ipc::Action::SwitchPresetColumnWidth => Self::SwitchPresetColumnWidth,
            niri_ipc::Action::MaximizeColumn => Self::MaximizeColumn,
            niri_ipc::Action::SetColumnWidth { change } => Self::SetColumnWidth(change),
            niri_ipc::Action::SwitchLayout { layout } => Self::SwitchLayout(layout),
            niri_ipc::Action::ShowHotkeyOverlay => Self::ShowHotkeyOverlay,
            niri_ipc::Action::MoveWorkspaceToMonitorLeft => Self::MoveWorkspaceToMonitorLeft,
            niri_ipc::Action::MoveWorkspaceToMonitorRight => Self::MoveWorkspaceToMonitorRight,
            niri_ipc::Action::MoveWorkspaceToMonitorDown => Self::MoveWorkspaceToMonitorDown,
            niri_ipc::Action::MoveWorkspaceToMonitorUp => Self::MoveWorkspaceToMonitorUp,
            niri_ipc::Action::ToggleDebugTint => Self::ToggleDebugTint,
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct DebugConfig {
    #[knuffel(child)]
    pub dbus_interfaces_in_non_session_instances: bool,
    #[knuffel(child)]
    pub wait_for_frame_completion_before_queueing: bool,
    #[knuffel(child)]
    pub enable_color_transformations_capability: bool,
    #[knuffel(child)]
    pub enable_overlay_planes: bool,
    #[knuffel(child)]
    pub disable_cursor_plane: bool,
    #[knuffel(child, unwrap(argument))]
    pub render_drm_device: Option<PathBuf>,
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

        let config = Self::parse("config.kdl", &contents).context("error parsing")?;
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

impl FromStr for Mode {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((width, rest)) = s.split_once('x') else {
            return Err(miette!("no 'x' separator found"));
        };

        let (height, refresh) = match rest.split_once('@') {
            Some((height, refresh)) => (height, Some(refresh)),
            None => (rest, None),
        };

        let width = width
            .parse()
            .into_diagnostic()
            .context("error parsing width")?;
        let height = height
            .parse()
            .into_diagnostic()
            .context("error parsing height")?;
        let refresh = refresh
            .map(str::parse)
            .transpose()
            .into_diagnostic()
            .context("error parsing refresh rate")?;

        Ok(Self {
            width,
            height,
            refresh,
        })
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
            } else {
                return Err(miette!("invalid modifier: {part}"));
            }
        }

        let keysym = keysym_from_name(key, KEYSYM_CASE_INSENSITIVE);
        if keysym.raw() == KEY_NoSymbol {
            return Err(miette!("invalid key: {key}"));
        }

        Ok(Key { keysym, modifiers })
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

pub fn set_miette_hook() -> Result<(), miette::InstallError> {
    miette::set_hook(Box::new(|_| Box::new(NarratableReportHandler::new())))
}

#[cfg(test)]
mod tests {
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
            r#"
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
                    accel-speed 0.2
                    accel-profile "flat"
                    tap-button-map "left-middle-right"
                }

                mouse {
                    natural-scroll
                    accel-speed 0.4
                    accel-profile "flat"
                }

                trackpoint {
                    natural-scroll
                    accel-speed 0.0
                    accel-profile "flat"
                }

                tablet {
                    map-to-output "eDP-1"
                }

                disable-power-key-handling
            }

            output "eDP-1" {
                scale 2.0
                transform "flipped-90"
                position x=10 y=20
                mode "1920x1080@144"
            }

            layout {
                focus-ring {
                    width 5
                    active-color 0 100 200 255
                    inactive-color 255 200 100 0
                }

                border {
                    width 3
                    inactive-color 255 200 100 0
                }

                preset-column-widths {
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
            }

            spawn-at-startup "alacritty" "-e" "fish"

            prefer-no-csd

            cursor {
                xcursor-theme "breeze_cursors"
                xcursor-size 16
            }

            screenshot-path "~/Screenshots/screenshot.png"

            hotkey-overlay {
                skip-at-startup
            }

            animations {
                slowdown 2.0

                workspace-switch { off; }

                horizontal-view-movement {
                    duration-ms 100
                    curve "ease-out-expo"
                }
            }

            window-rule {
                match app-id=".*alacritty"
                exclude title="~"

                open-on-output "eDP-1"
            }

            binds {
                Mod+T { spawn "alacritty"; }
                Mod+Q { close-window; }
                Mod+Shift+H { focus-monitor-left; }
                Mod+Ctrl+Shift+L { move-window-to-monitor-right; }
                Mod+Comma { consume-window-into-column; }
                Mod+1 { focus-workspace 1; }
                Mod+Shift+E { quit skip-confirmation=true; }
            }

            debug {
                render-drm-device "/dev/dri/renderD129"
            }
            "#,
            Config {
                input: Input {
                    keyboard: Keyboard {
                        xkb: Xkb {
                            layout: Some("us,ru".to_owned()),
                            options: Some("grp:win_space_toggle".to_owned()),
                            ..Default::default()
                        },
                        repeat_delay: 600,
                        repeat_rate: 25,
                        track_layout: TrackLayout::Window,
                    },
                    touchpad: Touchpad {
                        tap: true,
                        dwt: true,
                        dwtp: true,
                        natural_scroll: false,
                        accel_speed: 0.2,
                        accel_profile: Some(AccelProfile::Flat),
                        tap_button_map: Some(TapButtonMap::LeftMiddleRight),
                    },
                    mouse: Mouse {
                        natural_scroll: true,
                        accel_speed: 0.4,
                        accel_profile: Some(AccelProfile::Flat),
                    },
                    trackpoint: Trackpoint {
                        natural_scroll: true,
                        accel_speed: 0.0,
                        accel_profile: Some(AccelProfile::Flat),
                    },
                    tablet: Tablet {
                        map_to_output: Some("eDP-1".to_owned()),
                    },
                    disable_power_key_handling: true,
                },
                outputs: vec![Output {
                    off: false,
                    name: "eDP-1".to_owned(),
                    scale: 2.,
                    transform: Transform::Flipped90,
                    position: Some(Position { x: 10, y: 20 }),
                    mode: Some(Mode {
                        width: 1920,
                        height: 1080,
                        refresh: Some(144.),
                    }),
                }],
                layout: Layout {
                    focus_ring: FocusRing {
                        off: false,
                        width: 5,
                        active_color: Color {
                            r: 0,
                            g: 100,
                            b: 200,
                            a: 255,
                        },
                        inactive_color: Color {
                            r: 255,
                            g: 200,
                            b: 100,
                            a: 0,
                        },
                    },
                    border: Border {
                        off: false,
                        width: 3,
                        active_color: Color {
                            r: 255,
                            g: 200,
                            b: 127,
                            a: 255,
                        },
                        inactive_color: Color {
                            r: 255,
                            g: 200,
                            b: 100,
                            a: 0,
                        },
                    },
                    preset_column_widths: vec![
                        PresetWidth::Proportion(0.25),
                        PresetWidth::Proportion(0.5),
                        PresetWidth::Fixed(960),
                        PresetWidth::Fixed(1280),
                    ],
                    default_column_width: Some(DefaultColumnWidth(vec![PresetWidth::Proportion(
                        0.25,
                    )])),
                    gaps: 8,
                    struts: Struts {
                        left: 1,
                        right: 2,
                        top: 3,
                        bottom: 0,
                    },
                    center_focused_column: CenterFocusedColumn::OnOverflow,
                },
                spawn_at_startup: vec![SpawnAtStartup {
                    command: vec!["alacritty".to_owned(), "-e".to_owned(), "fish".to_owned()],
                }],
                prefer_no_csd: true,
                cursor: Cursor {
                    xcursor_theme: String::from("breeze_cursors"),
                    xcursor_size: 16,
                },
                screenshot_path: Some(String::from("~/Screenshots/screenshot.png")),
                hotkey_overlay: HotkeyOverlay {
                    skip_at_startup: true,
                },
                animations: Animations {
                    slowdown: 2.,
                    workspace_switch: Animation {
                        off: true,
                        ..Animation::unfilled()
                    },
                    horizontal_view_movement: Animation {
                        duration_ms: Some(100),
                        curve: Some(AnimationCurve::EaseOutExpo),
                        ..Animation::unfilled()
                    },
                    ..Default::default()
                },
                window_rules: vec![WindowRule {
                    matches: vec![Match {
                        app_id: Some(Regex::new(".*alacritty").unwrap()),
                        title: None,
                    }],
                    excludes: vec![Match {
                        app_id: None,
                        title: Some(Regex::new("~").unwrap()),
                    }],
                    open_on_output: Some("eDP-1".to_owned()),
                    ..Default::default()
                }],
                binds: Binds(vec![
                    Bind {
                        key: Key {
                            keysym: Keysym::t,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        actions: vec![Action::Spawn(vec!["alacritty".to_owned()])],
                    },
                    Bind {
                        key: Key {
                            keysym: Keysym::q,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        actions: vec![Action::CloseWindow],
                    },
                    Bind {
                        key: Key {
                            keysym: Keysym::h,
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT,
                        },
                        actions: vec![Action::FocusMonitorLeft],
                    },
                    Bind {
                        key: Key {
                            keysym: Keysym::l,
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT | Modifiers::CTRL,
                        },
                        actions: vec![Action::MoveWindowToMonitorRight],
                    },
                    Bind {
                        key: Key {
                            keysym: Keysym::comma,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        actions: vec![Action::ConsumeWindowIntoColumn],
                    },
                    Bind {
                        key: Key {
                            keysym: Keysym::_1,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        actions: vec![Action::FocusWorkspace(1)],
                    },
                    Bind {
                        key: Key {
                            keysym: Keysym::e,
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT,
                        },
                        actions: vec![Action::Quit(true)],
                    },
                ]),
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
            "2560x1600@165.004".parse::<Mode>().unwrap(),
            Mode {
                width: 2560,
                height: 1600,
                refresh: Some(165.004),
            },
        );

        assert_eq!(
            "1920x1080".parse::<Mode>().unwrap(),
            Mode {
                width: 1920,
                height: 1080,
                refresh: None,
            },
        );

        assert!("1920".parse::<Mode>().is_err());
        assert!("1920x".parse::<Mode>().is_err());
        assert!("1920x1080@".parse::<Mode>().is_err());
        assert!("1920x1080@60Hz".parse::<Mode>().is_err());
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
}
