use std::path::PathBuf;
use std::str::FromStr;

use bitflags::bitflags;
use directories::ProjectDirs;
use miette::{miette, Context, IntoDiagnostic};
use smithay::input::keyboard::keysyms::KEY_NoSymbol;
use smithay::input::keyboard::xkb::{keysym_from_name, KEYSYM_CASE_INSENSITIVE};
use smithay::input::keyboard::Keysym;

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Config {
    #[knuffel(child, default)]
    pub input: Input,
    #[knuffel(children(name = "output"))]
    pub outputs: Vec<Output>,
    #[knuffel(children(name = "spawn-at-startup"))]
    pub spawn_at_startup: Vec<SpawnAtStartup>,
    #[knuffel(child, default)]
    pub focus_ring: FocusRing,
    #[knuffel(child, default)]
    pub prefer_no_csd: bool,
    #[knuffel(child, default)]
    pub cursor: Cursor,
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
}

#[derive(knuffel::Decode, Debug, Default, PartialEq, Eq)]
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

// FIXME: Add the rest of the settings.
#[derive(knuffel::Decode, Debug, Default, PartialEq)]
pub struct Touchpad {
    #[knuffel(child)]
    pub tap: bool,
    #[knuffel(child)]
    pub natural_scroll: bool,
    #[knuffel(child, unwrap(argument), default)]
    pub accel_speed: f64,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Output {
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(child, unwrap(argument), default = 1.)]
    pub scale: f64,
    #[knuffel(child)]
    pub position: Option<Position>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            name: String::new(),
            scale: 1.,
            position: None,
        }
    }
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct Position {
    #[knuffel(property)]
    pub x: i32,
    #[knuffel(property)]
    pub y: i32,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct SpawnAtStartup {
    #[knuffel(arguments)]
    pub command: Vec<String>,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct FocusRing {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = 4)]
    pub width: u16,
    #[knuffel(child, default = Color::new(0.5, 0.8, 1.0, 1.0))]
    pub active_color: Color,
    #[knuffel(child, default = Color::new(0.3, 0.3, 0.3, 1.0))]
    pub inactive_color: Color,
}

impl Default for FocusRing {
    fn default() -> Self {
        Self {
            off: false,
            width: 4,
            active_color: Color::new(0.5, 0.8, 1.0, 1.0),
            inactive_color: Color::new(0.3, 0.3, 0.3, 1.0),
        }
    }
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq)]
pub struct Color {
    #[knuffel(argument)]
    pub r: f32,
    #[knuffel(argument)]
    pub g: f32,
    #[knuffel(argument)]
    pub b: f32,
    #[knuffel(argument)]
    pub a: f32,
}

impl Color {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        [c.r, c.g, c.b, c.a]
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

#[derive(knuffel::Decode, Debug, Default, PartialEq, Eq)]
pub struct Binds(#[knuffel(children)] pub Vec<Bind>);

#[derive(knuffel::Decode, Debug, PartialEq, Eq)]
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

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub enum Action {
    #[knuffel(skip)]
    None,
    Quit,
    #[knuffel(skip)]
    ChangeVt(i32),
    Suspend,
    ToggleDebugTint,
    Spawn(#[knuffel(arguments)] Vec<String>),
    Screenshot,
    CloseWindow,
    FullscreenWindow,
    FocusColumnLeft,
    FocusColumnRight,
    FocusWindowDown,
    FocusWindowUp,
    MoveColumnLeft,
    MoveColumnRight,
    MoveWindowDown,
    MoveWindowUp,
    ConsumeWindowIntoColumn,
    ExpelWindowFromColumn,
    FocusWorkspaceDown,
    FocusWorkspaceUp,
    FocusWorkspace(#[knuffel(argument)] u8),
    MoveWindowToWorkspaceDown,
    MoveWindowToWorkspaceUp,
    MoveWindowToWorkspace(#[knuffel(argument)] u8),
    FocusMonitorLeft,
    FocusMonitorRight,
    FocusMonitorDown,
    FocusMonitorUp,
    MoveWindowToMonitorLeft,
    MoveWindowToMonitorRight,
    MoveWindowToMonitorDown,
    MoveWindowToMonitorUp,
    SwitchPresetColumnWidth,
    MaximizeColumn,
}

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct DebugConfig {
    #[knuffel(child, unwrap(argument), default = 1.)]
    pub animation_slowdown: f64,
    #[knuffel(child)]
    pub dbus_interfaces_in_non_session_instances: bool,
    #[knuffel(child)]
    pub wait_for_frame_completion_before_queueing: bool,
    #[knuffel(child)]
    pub enable_color_transformations_capability: bool,
    #[knuffel(child)]
    pub enable_overlay_planes: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            animation_slowdown: 1.,
            dbus_interfaces_in_non_session_instances: false,
            wait_for_frame_completion_before_queueing: false,
            enable_color_transformations_capability: false,
            enable_overlay_planes: false,
        }
    }
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> miette::Result<(Self, PathBuf)> {
        let path = if let Some(path) = path {
            path
        } else {
            let mut path = ProjectDirs::from("", "", "niri")
                .ok_or_else(|| miette!("error retrieving home directory"))?
                .config_dir()
                .to_owned();
            path.push("config.kdl");
            path
        };

        let contents = std::fs::read_to_string(&path)
            .into_diagnostic()
            .with_context(|| format!("error reading {path:?}"))?;

        let config = Self::parse("config.kdl", &contents).context("error parsing")?;
        debug!("loaded config from {path:?}");
        Ok((config, path))
    }

    pub fn parse(filename: &str, text: &str) -> Result<Self, knuffel::Error> {
        knuffel::parse(filename, text)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::parse(
            "default-config.kdl",
            include_str!("../resources/default-config.kdl"),
        )
        .unwrap()
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

#[cfg(test)]
mod tests {
    use miette::NarratableReportHandler;

    use super::*;

    #[track_caller]
    fn check(text: &str, expected: Config) {
        let _ = miette::set_hook(Box::new(|_| Box::new(NarratableReportHandler::new())));

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
                    xkb {
                        layout "us,ru"
                        options "grp:win_space_toggle"
                    }
                }

                touchpad {
                    tap
                    accel-speed 0.2
                }
            }

            output "eDP-1" {
                scale 2.0
                position x=10 y=20
            }

            spawn-at-startup "alacritty" "-e" "fish"

            focus-ring {
                width 5
                active-color 0.0 0.25 0.5 1.0
                inactive-color 1.0 0.5 0.25 0.0
            }

            prefer-no-csd

            cursor {
                xcursor-theme "breeze_cursors"
                xcursor-size 16
            }

            binds {
                Mod+T { spawn "alacritty"; }
                Mod+Q { close-window; }
                Mod+Shift+H { focus-monitor-left; }
                Mod+Ctrl+Shift+L { move-window-to-monitor-right; }
                Mod+Comma { consume-window-into-column; }
                Mod+1 { focus-workspace 1;}
            }

            debug {
                animation-slowdown 2.0
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
                    },
                    touchpad: Touchpad {
                        tap: true,
                        natural_scroll: false,
                        accel_speed: 0.2,
                    },
                },
                outputs: vec![Output {
                    name: "eDP-1".to_owned(),
                    scale: 2.,
                    position: Some(Position { x: 10, y: 20 }),
                }],
                spawn_at_startup: vec![SpawnAtStartup {
                    command: vec!["alacritty".to_owned(), "-e".to_owned(), "fish".to_owned()],
                }],
                focus_ring: FocusRing {
                    off: false,
                    width: 5,
                    active_color: Color {
                        r: 0.,
                        g: 0.25,
                        b: 0.5,
                        a: 1.,
                    },
                    inactive_color: Color {
                        r: 1.,
                        g: 0.5,
                        b: 0.25,
                        a: 0.,
                    },
                },
                prefer_no_csd: true,
                cursor: Cursor {
                    xcursor_theme: String::from("breeze_cursors"),
                    xcursor_size: 16,
                },
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
                ]),
                debug: DebugConfig {
                    animation_slowdown: 2.,
                    ..Default::default()
                },
            },
        );
    }

    #[test]
    fn can_create_default_config() {
        let _ = Config::default();
    }
}
