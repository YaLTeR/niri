use std::path::PathBuf;
use std::str::FromStr;

use bitflags::bitflags;
use directories::ProjectDirs;
use miette::{miette, Context, IntoDiagnostic};
use smithay::input::keyboard::xkb::{keysym_from_name, KEY_NoSymbol, KEYSYM_CASE_INSENSITIVE};
use smithay::input::keyboard::Keysym;

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Config {
    #[knuffel(child, default)]
    pub input: Input,
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
    MoveWindowToWorkspaceDown,
    MoveWindowToWorkspaceUp,
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
    pub screen_cast_in_non_session_instances: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            animation_slowdown: 1.,
            screen_cast_in_non_session_instances: false,
        }
    }
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> miette::Result<Self> {
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
        Ok(config)
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
        if keysym == KEY_NoSymbol {
            return Err(miette!("invalid key: {key}"));
        }

        Ok(Key { keysym, modifiers })
    }
}

#[cfg(test)]
mod tests {
    use smithay::input::keyboard::xkb::keysyms::*;

    use super::*;

    #[track_caller]
    fn check(text: &str, expected: Config) {
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

            binds {
                Mod+T { spawn "alacritty"; }
                Mod+Q { close-window; }
                Mod+Shift+H { focus-monitor-left; }
                Mod+Ctrl+Shift+L { move-window-to-monitor-right; }
                Mod+Comma { consume-window-into-column; }
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
                    },
                    touchpad: Touchpad {
                        tap: true,
                        natural_scroll: false,
                        accel_speed: 0.2,
                    },
                },
                binds: Binds(vec![
                    Bind {
                        key: Key {
                            keysym: KEY_t,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        actions: vec![Action::Spawn(vec!["alacritty".to_owned()])],
                    },
                    Bind {
                        key: Key {
                            keysym: KEY_q,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        actions: vec![Action::CloseWindow],
                    },
                    Bind {
                        key: Key {
                            keysym: KEY_h,
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT,
                        },
                        actions: vec![Action::FocusMonitorLeft],
                    },
                    Bind {
                        key: Key {
                            keysym: KEY_l,
                            modifiers: Modifiers::COMPOSITOR | Modifiers::SHIFT | Modifiers::CTRL,
                        },
                        actions: vec![Action::MoveWindowToMonitorRight],
                    },
                    Bind {
                        key: Key {
                            keysym: KEY_comma,
                            modifiers: Modifiers::COMPOSITOR,
                        },
                        actions: vec![Action::ConsumeWindowIntoColumn],
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
