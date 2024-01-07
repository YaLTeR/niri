use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{bail, Context};
use bitflags::bitflags;
use directories::ProjectDirs;
use serde::Deserialize;
use serde_with::DeserializeFromStr;
use smithay::input::keyboard::keysyms::KEY_NoSymbol;
use smithay::input::keyboard::xkb::{keysym_from_name, KEYSYM_CASE_INSENSITIVE};
use smithay::input::keyboard::{Keysym, XkbConfig};

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub input: Input,
    #[serde(default)]
    pub output: HashMap<String, Output>,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub clients: Clients,
    #[serde(default)]
    pub cursor: Cursor,
    #[serde(default)]
    pub screenshot_ui: ScreenshotUi,
    // Running niri without binds doesn't make much sense.
    pub binds: HashMap<Key, Action>,
    #[serde(default)]
    pub debug: DebugConfig,
}

// FIXME: Add other devices.
#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Input {
    pub keyboard: Keyboard,
    pub touchpad: Touchpad,
    pub tablet: Tablet,
    pub disable_power_key_handling: bool,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Keyboard {
    pub xkb: Xkb,
    pub repeat_delay: u16,
    pub repeat_rate: u8,
    pub track_layout: TrackLayout,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            xkb: Default::default(),
            // The defaults were chosen to match wlroots and sway.
            repeat_delay: 600,
            repeat_rate: 25,
            track_layout: Default::default(),
        }
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Xkb {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: Option<String>,
}

impl Default for Xkb {
    fn default() -> Self {
        Self {
            rules: String::new(),
            model: String::new(),
            layout: String::from("us"),
            variant: String::new(),
            options: None,
        }
    }
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

#[derive(Deserialize, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrackLayout {
    /// The layout change is global.
    #[default]
    Global,
    /// The layout change is window local.
    Window,
}

// FIXME: Add the rest of the settings.
#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Touchpad {
    pub tap: bool,
    pub natural_scroll: bool,
    pub accel_speed: f64,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Tablet {
    pub map_to_output: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Output {
    pub off: bool,
    pub scale: f64,
    pub position: Option<Position>,
    pub mode: Option<Mode>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            off: false,
            scale: 1.,
            position: None,
            mode: None,
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(DeserializeFromStr, Debug, Clone, PartialEq)]
pub struct Mode {
    pub width: u16,
    pub height: u16,
    pub refresh: Option<f64>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Layout {
    pub focus_ring: FocusRing,
    pub border: FocusRing,
    pub preset_column_widths: Vec<PresetWidth>,
    // TODO
    pub default_column_width: Option<PresetWidth>,
    pub gaps: u16,
    pub struts: Struts,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            focus_ring: Default::default(),
            border: default_border(),
            preset_column_widths: vec![
                PresetWidth::Proportion(0.333),
                PresetWidth::Proportion(0.5),
                PresetWidth::Proportion(0.667),
            ],
            default_column_width: Some(PresetWidth::Proportion(0.5)),
            gaps: 16,
            struts: Default::default(),
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct FocusRing {
    pub off: bool,
    pub width: u16,
    pub active_color: Color,
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

pub const fn default_border() -> FocusRing {
    FocusRing {
        off: true,
        width: 4,
        active_color: Color::new(255, 200, 127, 255),
        inactive_color: Color::new(80, 80, 80, 255),
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(from = "[u8; 4]")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        [c.r, c.g, c.b, c.a].map(|x| x as f32 / 255.)
    }
}

impl From<[u8; 4]> for Color {
    fn from(value: [u8; 4]) -> Self {
        let [r, g, b, a] = value;
        Self { r, g, b, a }
    }
}

#[derive(Deserialize, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Clients {
    pub prefer_no_csd: bool,
    pub spawn_at_startup: Vec<SpawnAtStartup>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpawnAtStartup {
    pub command: Vec<String>,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Cursor {
    pub xcursor_theme: String,
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

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PresetWidth {
    Proportion(f64),
    Fixed(i32),
}

#[derive(Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Struts {
    pub left: u16,
    pub right: u16,
    pub top: u16,
    pub bottom: u16,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct ScreenshotUi {
    pub disable_saving_to_disk: bool,
    pub screenshot_path: String,
}

impl Default for ScreenshotUi {
    fn default() -> Self {
        Self {
            disable_saving_to_disk: false,
            screenshot_path: String::from(
                "~/Pictures/Screenshots/Screenshot from %Y-%m-%d %H-%M-%S.png",
            ),
        }
    }
}

#[derive(DeserializeFromStr, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub keysym: Keysym,
    pub modifiers: Modifiers,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers : u8 {
        const CTRL = 1;
        const SHIFT = 2;
        const ALT = 4;
        const SUPER = 8;
        const COMPOSITOR = 16;
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Quit,
    #[serde(skip)]
    ChangeVt(i32),
    Suspend,
    PowerOffMonitors,
    ToggleDebugTint,
    Spawn(Vec<String>),
    #[serde(skip)]
    ConfirmScreenshot,
    #[serde(skip)]
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
    ConsumeWindowIntoColumn,
    ExpelWindowFromColumn,
    CenterColumn,
    FocusWorkspaceDown,
    FocusWorkspaceUp,
    FocusWorkspace(u8),
    MoveWindowToWorkspaceDown,
    MoveWindowToWorkspaceUp,
    MoveWindowToWorkspace(u8),
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
    SetWindowHeight(SizeChange),
    SwitchPresetColumnWidth,
    MaximizeColumn,
    SetColumnWidth(SizeChange),
    SwitchLayout(LayoutAction),
}

#[derive(DeserializeFromStr, Debug, Clone, Copy, PartialEq)]
pub enum SizeChange {
    SetFixed(i32),
    SetProportion(f64),
    AdjustFixed(i32),
    AdjustProportion(f64),
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutAction {
    Next,
    Prev,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct DebugConfig {
    pub animation_slowdown: f64,
    pub dbus_interfaces_in_non_session_instances: bool,
    pub wait_for_frame_completion_before_queueing: bool,
    pub enable_color_transformations_capability: bool,
    pub enable_overlay_planes: bool,
    pub disable_cursor_plane: bool,
    pub render_drm_device: Option<PathBuf>,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            animation_slowdown: 1.,
            dbus_interfaces_in_non_session_instances: false,
            wait_for_frame_completion_before_queueing: false,
            enable_color_transformations_capability: false,
            enable_overlay_planes: false,
            disable_cursor_plane: false,
            render_drm_device: None,
        }
    }
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<(Self, PathBuf)> {
        let path = if let Some(path) = path {
            path
        } else {
            let mut path = ProjectDirs::from("", "", "niri")
                .context("error retrieving home directory")?
                .config_dir()
                .to_owned();
            path.push("config.kdl");
            path
        };

        let contents =
            std::fs::read_to_string(&path).with_context(|| format!("error reading {path:?}"))?;

        let config = Self::parse(&contents).context("error parsing")?;
        debug!("loaded config from {path:?}");
        Ok((config, path))
    }

    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::parse(include_str!("../resources/default-config.kdl"))
            .context("error parsing default config")
            .unwrap()
    }
}

impl FromStr for Mode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (width, rest) = s.split_once('x').context("no 'x' separator found")?;

        let (height, refresh) = match rest.split_once('@') {
            Some((height, refresh)) => (height, Some(refresh)),
            None => (rest, None),
        };

        let width = width.parse().context("error parsing width")?;
        let height = height.parse().context("error parsing height")?;
        let refresh = refresh
            .map(str::parse)
            .transpose()
            .context("error parsing refresh rate")?;

        Ok(Self {
            width,
            height,
            refresh,
        })
    }
}

impl FromStr for Key {
    type Err = anyhow::Error;

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
                bail!("invalid modifier: {part}");
            }
        }

        let keysym = keysym_from_name(key, KEYSYM_CASE_INSENSITIVE);
        if keysym.raw() == KEY_NoSymbol {
            bail!("invalid key: {key}");
        }

        Ok(Key { keysym, modifiers })
    }
}

impl FromStr for SizeChange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('%') {
            Some((value, empty)) => {
                if !empty.is_empty() {
                    bail!("trailing characters after '%' are not allowed");
                }

                match value.bytes().next() {
                    Some(b'-' | b'+') => {
                        let value = value.parse().context("error parsing value")?;
                        Ok(Self::AdjustProportion(value))
                    }
                    Some(_) => {
                        let value = value.parse().context("error parsing value")?;
                        Ok(Self::SetProportion(value))
                    }
                    None => bail!("value is missing"),
                }
            }
            None => {
                let value = s;
                match value.bytes().next() {
                    Some(b'-' | b'+') => {
                        let value = value.parse().context("error parsing value")?;
                        Ok(Self::AdjustFixed(value))
                    }
                    Some(_) => {
                        let value = value.parse().context("error parsing value")?;
                        Ok(Self::SetFixed(value))
                    }
                    None => bail!("value is missing"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check(text: &str, expected: Config) {
        let parsed = Config::parse(text).map_err(anyhow::Error::new).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse() {
        check(
            r#"
            [input]
            disable_power_key_handling = true

            [input.keyboard]
            repeat_delay = 600
            repeat_rate = 25
            track_layout = 'window'
            xkb.layout = 'us,ru'
            xkb.options = 'grp:win_space_toggle'

            [input.touchpad]
            tap = true
            accel_speed = 0.2

            [input.tablet]
            map_to_output = 'eDP-1'

            [output.'eDP-1']
            scale = 2.0
            position = { x = 10, y = 20 }
            mode = '1920x1080@144'

            [layout]
            gaps = 8
            preset_column_widths = [
                { proportion = 0.25 },
                { proportion = 0.5 },
                { fixed = 960 },
                { fixed = 1280 },
            ]
            default_column_width = { proportion = 0.25 }
            struts = { left = 1, right = 2, top = 3 }

            [layout.focus_ring]
            width = 5
            active_color = [0, 100, 200, 255]
            inactive_color = [255, 200, 100, 0]

            [layout.border]
            width = 3
            active_color = [0, 100, 200, 255]
            inactive_color = [255, 200, 100, 0]

            [clients]
            prefer_no_csd = true
            spawn_at_startup = [
                { command = ['alacritty', '-e', 'fish'] },
            ]

            [cursor]
            xcursor_theme = 'breeze_cursors'
            xcursor_size = 16

            [screenshot_ui]
            screenshot_path = '~/Screenshots/screenshot.png'
            disable_saving_to_disk = true

            [binds]
            'Mod+T' = { spawn = ['alacritty'] }
            'Mod+Q' = 'close_window'
            'Mod+Shift+H' = 'focus_monitor_left'
            'Mod+Ctrl+Shift+L' = 'move_window_to_monitor_right'
            'Mod+Comma' = 'consume_window_into_column'
            'Mod+1' = { focus_workspace = 1 }

            [debug]
            animation_slowdown = 2.0
            render_drm_device = '/dev/dri/renderD129'
            "#,
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
                        tap: true,
                        natural_scroll: false,
                        accel_speed: 0.2,
                    },
                    tablet: Tablet {
                        map_to_output: Some("eDP-1".to_owned()),
                    },
                    disable_power_key_handling: true,
                },
                output: HashMap::from([(
                    "eDP-1".to_owned(),
                    Output {
                        off: false,
                        scale: 2.,
                        position: Some(Position { x: 10, y: 20 }),
                        mode: Some(Mode {
                            width: 1920,
                            height: 1080,
                            refresh: Some(144.),
                        }),
                    },
                )]),
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
                    border: FocusRing {
                        off: false,
                        width: 3,
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
                    preset_column_widths: vec![
                        PresetWidth::Proportion(0.25),
                        PresetWidth::Proportion(0.5),
                        PresetWidth::Fixed(960),
                        PresetWidth::Fixed(1280),
                    ],
                    default_column_width: Some(PresetWidth::Proportion(0.25)),
                    gaps: 8,
                    struts: Struts {
                        left: 1,
                        right: 2,
                        top: 3,
                        bottom: 0,
                    },
                },
                clients: Clients {
                    spawn_at_startup: vec![SpawnAtStartup {
                        command: vec!["alacritty".to_owned(), "-e".to_owned(), "fish".to_owned()],
                    }],
                    prefer_no_csd: true,
                },
                cursor: Cursor {
                    xcursor_theme: String::from("breeze_cursors"),
                    xcursor_size: 16,
                },
                screenshot_ui: ScreenshotUi {
                    disable_saving_to_disk: true,
                    screenshot_path: String::from("~/Screenshots/screenshot.png"),
                },
                binds: HashMap::from([
                    (
                        Key {
                            keysym: Keysym::t,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        Action::Spawn(vec!["alacritty".to_owned()]),
                    ),
                    (
                        Key {
                            keysym: Keysym::q,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        Action::CloseWindow,
                    ),
                    (
                        Key {
                            keysym: Keysym::h,
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT,
                        },
                        Action::FocusMonitorLeft,
                    ),
                    (
                        Key {
                            keysym: Keysym::l,
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT | Modifiers::CTRL,
                        },
                        Action::MoveWindowToMonitorRight,
                    ),
                    (
                        Key {
                            keysym: Keysym::comma,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        Action::ConsumeWindowIntoColumn,
                    ),
                    (
                        Key {
                            keysym: Keysym::_1,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        Action::FocusWorkspace(1),
                    ),
                ]),
                debug: DebugConfig {
                    animation_slowdown: 2.,
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
