#[macro_use]
extern crate tracing;

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use layer_rule::LayerRule;
use miette::{Context, IntoDiagnostic};
#[cfg(test)]
use niri_ipc::SizeChange;
#[cfg(test)]
use smithay::input::keyboard::Keysym;

pub mod layer_rule;

pub mod core;
pub use core::*;

mod utils;
pub use utils::{expect_only_children, RegexEq};

pub mod appearance;
pub use appearance::*;

pub mod animation;
pub use animation::*;

pub mod input;
pub use input::*;

pub mod bindings;
pub use bindings::*;

pub mod layout;
pub use layout::*;

pub mod output;
pub use output::*;

pub mod workspace;
pub use workspace::*;

pub mod rules;
pub use rules::*;

pub mod gestures;
pub use gestures::*;

pub mod startup;
pub use startup::*;

pub mod debug;
pub use debug::*;

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Config {
    #[knuffel(child, default)]
    pub input: Input,
    #[knuffel(children(name = "output"))]
    pub outputs: Outputs,
    #[knuffel(children(name = "spawn-at-startup"))]
    pub spawn_at_startup: Vec<SpawnAtStartup>,
    #[knuffel(children(name = "layout"))]
    pub layouts: Vec<Layout>,
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
    pub clipboard: Clipboard,
    #[knuffel(child, default)]
    pub hotkey_overlay: HotkeyOverlay,
    #[knuffel(children(name = "animations"))]
    pub animations_list: Vec<Animations>,
    #[knuffel(children(name = "gestures"))]
    pub gestures_list: Vec<Gestures>,
    #[knuffel(child, default)]
    pub overview: Overview,
    #[knuffel(children(name = "environment"))]
    pub environment_list: Vec<Environment>,
    #[knuffel(child, default)]
    pub xwayland_satellite: XwaylandSatellite,
    #[knuffel(children(name = "window-rule"))]
    pub window_rules: Vec<WindowRule>,
    #[knuffel(children(name = "layer-rule"))]
    pub layer_rules: Vec<LayerRule>,
    #[knuffel(children(name = "binds"))]
    pub binds_list: Vec<Binds>,
    #[knuffel(child, default)]
    pub switch_events: SwitchBinds,
    #[knuffel(children(name = "debug"))]
    pub debug_list: Vec<DebugConfig>,
    #[knuffel(children(name = "workspace"))]
    pub workspaces: Vec<Workspace>,
}

impl Config {
    pub fn layout(&self) -> Layout {
        let mut layout = Layout::default();
        for l in &self.layouts {
            layout.merge_with(l);
        }
        layout
    }

    pub fn animations(&self) -> Animations {
        let mut animations = Animations::default();
        for a in &self.animations_list {
            animations.merge_with(a);
        }
        animations
    }

    pub fn gestures(&self) -> Gestures {
        let mut gestures = Gestures::default();
        for g in &self.gestures_list {
            gestures.merge_with(g);
        }
        gestures
    }

    pub fn binds(&self) -> Binds {
        let mut binds = Binds::default();
        for b in &self.binds_list {
            binds.merge_with(b);
        }
        binds
    }

    pub fn debug(&self) -> DebugConfig {
        let mut debug = DebugConfig::default();
        for d in &self.debug_list {
            debug.merge_with(d);
        }
        debug
    }

    pub fn environment(&self) -> Environment {
        let mut environment = Environment::default();
        for e in &self.environment_list {
            environment.merge_with(e);
        }
        environment
    }

    pub fn load(path: &Path) -> miette::Result<Self> {
        let _span = tracy_client::span!("Config::load");
        Self::load_internal(path).context("error loading config")
    }

    fn load_internal(path: &Path) -> miette::Result<Self> {
        let contents = fs::read_to_string(path)
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

#[derive(Debug, Clone)]
pub enum ConfigPath {
    /// Explicitly set config path.
    ///
    /// Load the config only from this path, never create it.
    Explicit(PathBuf),

    /// Default config path.
    ///
    /// Prioritize the user path, fallback to the system path, fallback to creating the user path
    /// at compositor startup.
    Regular {
        /// User config path, usually `$XDG_CONFIG_HOME/niri/config.kdl`.
        user_path: PathBuf,
        /// System config path, usually `/etc/niri/config.kdl`.
        system_path: PathBuf,
    },
}

impl ConfigPath {
    /// Load the config, or return an error if it doesn't exist.
    pub fn load(&self) -> miette::Result<Config> {
        let _span = tracy_client::span!("ConfigPath::load");

        self.load_inner(|user_path, system_path| {
            Err(miette::miette!(
                "no config file found; create one at {user_path:?} or {system_path:?}",
            ))
        })
        .context("error loading config")
    }

    /// Load the config, or create it if it doesn't exist.
    ///
    /// Returns a tuple containing the path that was created, if any, and the loaded config.
    ///
    /// If the config was created, but for some reason could not be read afterwards,
    /// this may return `(Some(_), Err(_))`.
    pub fn load_or_create(&self) -> (Option<&Path>, miette::Result<Config>) {
        let _span = tracy_client::span!("ConfigPath::load_or_create");

        let mut created_at = None;

        let result = self
            .load_inner(|user_path, _| {
                Self::create(user_path, &mut created_at)
                    .map(|()| user_path)
                    .with_context(|| format!("error creating config at {user_path:?}"))
            })
            .context("error loading config");

        (created_at, result)
    }

    fn load_inner<'a>(
        &'a self,
        maybe_create: impl FnOnce(&'a Path, &'a Path) -> miette::Result<&'a Path>,
    ) -> miette::Result<Config> {
        let path = match self {
            ConfigPath::Explicit(path) => path.as_path(),
            ConfigPath::Regular {
                user_path,
                system_path,
            } => {
                if user_path.exists() {
                    user_path.as_path()
                } else if system_path.exists() {
                    system_path.as_path()
                } else {
                    maybe_create(user_path.as_path(), system_path.as_path())?
                }
            }
        };
        Config::load(path)
    }

    fn create<'a>(path: &'a Path, created_at: &mut Option<&'a Path>) -> miette::Result<()> {
        if let Some(default_parent) = path.parent() {
            fs::create_dir_all(default_parent)
                .into_diagnostic()
                .with_context(|| format!("error creating config directory {default_parent:?}"))?;
        }

        // Create the config and fill it with the default config if it doesn't exist.
        let mut new_file = match File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
        {
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
            res => res,
        }
        .into_diagnostic()
        .with_context(|| format!("error opening config file at {path:?}"))?;

        *created_at = Some(path);

        let default = include_bytes!("../../resources/default-config.kdl");

        new_file
            .write_all(default)
            .into_diagnostic()
            .with_context(|| format!("error writing default config to {path:?}"))?;

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Config::parse(
            "default-config.kdl",
            include_str!("../../resources/default-config.kdl"),
        )
        .unwrap();

        if config.layouts.is_empty() {
            config.layouts.push(Layout::default());
        }
        if config.animations_list.is_empty() {
            config.animations_list.push(Animations::default());
        }
        if config.gestures_list.is_empty() {
            config.gestures_list.push(Gestures::default());
        }
        if config.binds_list.is_empty() {
            config.binds_list.push(Binds::default());
        }
        if config.debug_list.is_empty() {
            config.debug_list.push(DebugConfig::default());
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use niri_ipc::{ConfiguredMode, PositionChange};
    use pretty_assertions::assert_eq;

    use super::*;

    #[track_caller]
    fn do_parse(text: &str) -> Config {
        Config::parse("test.kdl", text)
            .map_err(miette::Report::new)
            .unwrap()
    }

    #[test]
    fn parse() {
        let parsed = do_parse(
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
                    drag true
                    click-method "clickfinger"
                    accel-speed 0.2
                    accel-profile "flat"
                    scroll-method "two-finger"
                    scroll-button 272
                    scroll-button-lock
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
                    scroll-button-lock
                    left-handed
                    middle-emulation
                }

                tablet {
                    map-to-output "eDP-1"
                    calibration-matrix 1.0 2.0 3.0 \
                                       4.0 5.0 6.0
                }

                touch {
                    map-to-output "eDP-1"
                }

                disable-power-key-handling

                warp-mouse-to-focus
                focus-follows-mouse
                workspace-auto-back-and-forth

                mod-key "Mod5"
                mod-key-nested "Super"
            }

            output "eDP-1" {
                focus-at-startup
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

                shadow {
                    offset x=10 y=-20
                }

                tab-indicator {
                    width 10
                    position "top"
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

                default-column-display "tabbed"

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

            clipboard {
                disable-primary
            }

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

            gestures {
                dnd-edge-view-scroll {
                    trigger-width 10
                    max-speed 50
                }
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
                default-column-display "tabbed"
                default-floating-position x=100 y=-200 relative-to="bottom-left"

                focus-ring {
                    off
                    width 3
                }

                border {
                    on
                    width 8.5
                }

                tab-indicator {
                    active-color "#f00"
                }
            }

            layer-rule {
                match namespace="^notifications$"
                block-out-from "screencast"
            }

            binds {
                Mod+Escape hotkey-overlay-title="Inhibit" { toggle-keyboard-shortcuts-inhibit; }
                Mod+Shift+Escape allow-inhibiting=true { toggle-keyboard-shortcuts-inhibit; }
                Mod+T allow-when-locked=true { spawn "alacritty"; }
                Mod+Q hotkey-overlay-title=null { close-window; }
                Mod+Shift+H { focus-monitor-left; }
                Mod+Shift+O { focus-monitor "eDP-1"; }
                Mod+Ctrl+Shift+L { move-window-to-monitor-right; }
                Mod+Ctrl+Alt+O { move-window-to-monitor "eDP-1"; }
                Mod+Ctrl+Alt+P { move-column-to-monitor "DP-1"; }
                Mod+Comma { consume-window-into-column; }
                Mod+1 { focus-workspace 1; }
                Mod+Shift+1 { focus-workspace "workspace-1"; }
                Mod+Shift+E allow-inhibiting=false { quit skip-confirmation=true; }
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
        );

        assert_debug_snapshot!(parsed, @r#"
        Config {
            input: Input {
                keyboard: Keyboard {
                    xkb: Xkb {
                        rules: "",
                        model: "",
                        layout: "us,ru",
                        variant: "",
                        options: Some(
                            "grp:win_space_toggle",
                        ),
                        file: None,
                    },
                    repeat_delay: 600,
                    repeat_rate: 25,
                    track_layout: Window,
                    numlock: false,
                },
                touchpad: Touchpad {
                    off: false,
                    tap: true,
                    dwt: true,
                    dwtp: true,
                    drag: Some(
                        true,
                    ),
                    drag_lock: false,
                    natural_scroll: false,
                    click_method: Some(
                        Clickfinger,
                    ),
                    accel_speed: FloatOrInt(
                        0.2,
                    ),
                    accel_profile: Some(
                        Flat,
                    ),
                    scroll_method: Some(
                        TwoFinger,
                    ),
                    scroll_button: Some(
                        272,
                    ),
                    scroll_button_lock: true,
                    tap_button_map: Some(
                        LeftMiddleRight,
                    ),
                    left_handed: false,
                    disabled_on_external_mouse: true,
                    middle_emulation: false,
                    scroll_factor: Some(
                        FloatOrInt(
                            0.9,
                        ),
                    ),
                },
                mouse: Mouse {
                    off: false,
                    natural_scroll: true,
                    accel_speed: FloatOrInt(
                        0.4,
                    ),
                    accel_profile: Some(
                        Flat,
                    ),
                    scroll_method: Some(
                        NoScroll,
                    ),
                    scroll_button: Some(
                        273,
                    ),
                    scroll_button_lock: false,
                    left_handed: false,
                    middle_emulation: true,
                    scroll_factor: Some(
                        FloatOrInt(
                            0.2,
                        ),
                    ),
                },
                trackpoint: Trackpoint {
                    off: true,
                    natural_scroll: true,
                    accel_speed: FloatOrInt(
                        0.0,
                    ),
                    accel_profile: Some(
                        Flat,
                    ),
                    scroll_method: Some(
                        OnButtonDown,
                    ),
                    scroll_button: Some(
                        274,
                    ),
                    scroll_button_lock: false,
                    left_handed: false,
                    middle_emulation: false,
                },
                trackball: Trackball {
                    off: true,
                    natural_scroll: true,
                    accel_speed: FloatOrInt(
                        0.0,
                    ),
                    accel_profile: Some(
                        Flat,
                    ),
                    scroll_method: Some(
                        Edge,
                    ),
                    scroll_button: Some(
                        275,
                    ),
                    scroll_button_lock: true,
                    left_handed: true,
                    middle_emulation: true,
                },
                tablet: Tablet {
                    off: false,
                    calibration_matrix: Some(
                        [
                            1.0,
                            2.0,
                            3.0,
                            4.0,
                            5.0,
                            6.0,
                        ],
                    ),
                    map_to_output: Some(
                        "eDP-1",
                    ),
                    left_handed: false,
                },
                touch: Touch {
                    off: false,
                    map_to_output: Some(
                        "eDP-1",
                    ),
                },
                disable_power_key_handling: true,
                warp_mouse_to_focus: Some(
                    WarpMouseToFocus {
                        mode: None,
                    },
                ),
                focus_follows_mouse: Some(
                    FocusFollowsMouse {
                        max_scroll_amount: None,
                    },
                ),
                workspace_auto_back_and_forth: true,
                mod_key: Some(
                    IsoLevel3Shift,
                ),
                mod_key_nested: Some(
                    Super,
                ),
            },
            outputs: Outputs(
                [
                    Output {
                        off: false,
                        name: "eDP-1",
                        scale: Some(
                            FloatOrInt(
                                2.0,
                            ),
                        ),
                        transform: Flipped90,
                        position: Some(
                            Position {
                                x: 10,
                                y: 20,
                            },
                        ),
                        mode: Some(
                            ConfiguredMode {
                                width: 1920,
                                height: 1080,
                                refresh: Some(
                                    144.0,
                                ),
                            },
                        ),
                        variable_refresh_rate: Some(
                            Vrr {
                                on_demand: true,
                            },
                        ),
                        focus_at_startup: true,
                        background_color: Some(
                            Color {
                                r: 0.09803922,
                                g: 0.09803922,
                                b: 0.4,
                                a: 1.0,
                            },
                        ),
                        backdrop_color: None,
                    },
                ],
            ),
            spawn_at_startup: [
                SpawnAtStartup {
                    command: [
                        "alacritty",
                        "-e",
                        "fish",
                    ],
                },
            ],
            layouts: [
                Layout {
                    focus_ring: FocusRing {
                        off: false,
                        width: FloatOrInt(
                            5.0,
                        ),
                        active_color: Color {
                            r: 0.0,
                            g: 0.39215687,
                            b: 0.78431374,
                            a: 1.0,
                        },
                        inactive_color: Color {
                            r: 1.0,
                            g: 0.78431374,
                            b: 0.39215687,
                            a: 0.0,
                        },
                        urgent_color: Color {
                            r: 0.60784316,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        },
                        active_gradient: Some(
                            Gradient {
                                from: Color {
                                    r: 0.039215688,
                                    g: 0.078431375,
                                    b: 0.11764706,
                                    a: 1.0,
                                },
                                to: Color {
                                    r: 0.0,
                                    g: 0.5019608,
                                    b: 1.0,
                                    a: 1.0,
                                },
                                angle: 180,
                                relative_to: WorkspaceView,
                                in_: GradientInterpolation {
                                    color_space: Srgb,
                                    hue_interpolation: Shorter,
                                },
                            },
                        ),
                        inactive_gradient: None,
                        urgent_gradient: None,
                    },
                    border: Border {
                        off: false,
                        width: FloatOrInt(
                            3.0,
                        ),
                        active_color: Color {
                            r: 1.0,
                            g: 0.78431374,
                            b: 0.49803922,
                            a: 1.0,
                        },
                        inactive_color: Color {
                            r: 1.0,
                            g: 0.78431374,
                            b: 0.39215687,
                            a: 0.0,
                        },
                        urgent_color: Color {
                            r: 0.60784316,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        },
                        active_gradient: None,
                        inactive_gradient: None,
                        urgent_gradient: None,
                    },
                    shadow: Shadow {
                        on: false,
                        offset: ShadowOffset {
                            x: FloatOrInt(
                                10.0,
                            ),
                            y: FloatOrInt(
                                -20.0,
                            ),
                        },
                        softness: FloatOrInt(
                            30.0,
                        ),
                        spread: FloatOrInt(
                            5.0,
                        ),
                        draw_behind_window: false,
                        color: Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.4392157,
                        },
                        inactive_color: None,
                    },
                    tab_indicator: TabIndicator {
                        off: false,
                        hide_when_single_tab: false,
                        place_within_column: false,
                        gap: FloatOrInt(
                            5.0,
                        ),
                        width: FloatOrInt(
                            10.0,
                        ),
                        length: TabIndicatorLength {
                            total_proportion: Some(
                                0.5,
                            ),
                        },
                        position: Top,
                        gaps_between_tabs: FloatOrInt(
                            0.0,
                        ),
                        corner_radius: FloatOrInt(
                            0.0,
                        ),
                        active_color: None,
                        inactive_color: None,
                        urgent_color: None,
                        active_gradient: None,
                        inactive_gradient: None,
                        urgent_gradient: None,
                    },
                    insert_hint: InsertHint {
                        off: false,
                        color: Color {
                            r: 1.0,
                            g: 0.78431374,
                            b: 0.49803922,
                            a: 1.0,
                        },
                        gradient: Some(
                            Gradient {
                                from: Color {
                                    r: 0.039215688,
                                    g: 0.078431375,
                                    b: 0.11764706,
                                    a: 1.0,
                                },
                                to: Color {
                                    r: 0.0,
                                    g: 0.5019608,
                                    b: 1.0,
                                    a: 1.0,
                                },
                                angle: 180,
                                relative_to: WorkspaceView,
                                in_: GradientInterpolation {
                                    color_space: Srgb,
                                    hue_interpolation: Shorter,
                                },
                            },
                        ),
                    },
                    preset_column_widths: [
                        Proportion(
                            0.25,
                        ),
                        Proportion(
                            0.5,
                        ),
                        Fixed(
                            960,
                        ),
                        Fixed(
                            1280,
                        ),
                    ],
                    default_column_width: Some(
                        DefaultPresetSize(
                            Some(
                                Proportion(
                                    0.25,
                                ),
                            ),
                        ),
                    ),
                    preset_window_heights: [
                        Proportion(
                            0.25,
                        ),
                        Proportion(
                            0.5,
                        ),
                        Fixed(
                            960,
                        ),
                        Fixed(
                            1280,
                        ),
                    ],
                    center_focused_column: OnOverflow,
                    always_center_single_column: false,
                    empty_workspace_above_first: false,
                    default_column_display: Tabbed,
                    gaps: MaybeSet {
                        value: FloatOrInt(
                            8.0,
                        ),
                        is_set: true,
                    },
                    struts: Struts {
                        left: FloatOrInt(
                            1.0,
                        ),
                        right: FloatOrInt(
                            2.0,
                        ),
                        top: FloatOrInt(
                            3.0,
                        ),
                        bottom: FloatOrInt(
                            0.0,
                        ),
                    },
                    background_color: Color {
                        r: 0.25,
                        g: 0.25,
                        b: 0.25,
                        a: 1.0,
                    },
                },
            ],
            prefer_no_csd: true,
            cursor: Cursor {
                xcursor_theme: "breeze_cursors",
                xcursor_size: 16,
                hide_when_typing: true,
                hide_after_inactive_ms: Some(
                    3000,
                ),
            },
            screenshot_path: Some(
                "~/Screenshots/screenshot.png",
            ),
            clipboard: Clipboard {
                disable_primary: true,
            },
            hotkey_overlay: HotkeyOverlay {
                skip_at_startup: true,
                hide_not_bound: false,
            },
            animations_list: [
                Animations {
                    off: false,
                    slowdown: MaybeSet {
                        value: FloatOrInt(
                            2.0,
                        ),
                        is_set: true,
                    },
                    workspace_switch: WorkspaceSwitchAnim(
                        Animation {
                            off: false,
                            kind: Spring(
                                SpringParams {
                                    damping_ratio: 1.0,
                                    stiffness: 1000,
                                    epsilon: 0.0001,
                                },
                            ),
                        },
                    ),
                    window_open: WindowOpenAnim {
                        anim: Animation {
                            off: true,
                            kind: Easing(
                                EasingParams {
                                    duration_ms: 150,
                                    curve: EaseOutExpo,
                                },
                            ),
                        },
                        custom_shader: None,
                    },
                    window_close: WindowCloseAnim {
                        anim: Animation {
                            off: false,
                            kind: Easing(
                                EasingParams {
                                    duration_ms: 150,
                                    curve: EaseOutQuad,
                                },
                            ),
                        },
                        custom_shader: None,
                    },
                    horizontal_view_movement: HorizontalViewMovementAnim(
                        Animation {
                            off: false,
                            kind: Easing(
                                EasingParams {
                                    duration_ms: 100,
                                    curve: EaseOutExpo,
                                },
                            ),
                        },
                    ),
                    window_movement: WindowMovementAnim(
                        Animation {
                            off: false,
                            kind: Spring(
                                SpringParams {
                                    damping_ratio: 1.0,
                                    stiffness: 800,
                                    epsilon: 0.0001,
                                },
                            ),
                        },
                    ),
                    window_resize: WindowResizeAnim {
                        anim: Animation {
                            off: false,
                            kind: Spring(
                                SpringParams {
                                    damping_ratio: 1.0,
                                    stiffness: 800,
                                    epsilon: 0.0001,
                                },
                            ),
                        },
                        custom_shader: None,
                    },
                    config_notification_open_close: ConfigNotificationOpenCloseAnim(
                        Animation {
                            off: false,
                            kind: Spring(
                                SpringParams {
                                    damping_ratio: 0.6,
                                    stiffness: 1000,
                                    epsilon: 0.001,
                                },
                            ),
                        },
                    ),
                    screenshot_ui_open: ScreenshotUiOpenAnim(
                        Animation {
                            off: false,
                            kind: Easing(
                                EasingParams {
                                    duration_ms: 200,
                                    curve: EaseOutQuad,
                                },
                            ),
                        },
                    ),
                    overview_open_close: OverviewOpenCloseAnim(
                        Animation {
                            off: false,
                            kind: Spring(
                                SpringParams {
                                    damping_ratio: 1.0,
                                    stiffness: 800,
                                    epsilon: 0.0001,
                                },
                            ),
                        },
                    ),
                },
            ],
            gestures_list: [
                Gestures {
                    dnd_edge_view_scroll: DndEdgeViewScroll {
                        trigger_width: MaybeSet {
                            value: FloatOrInt(
                                10.0,
                            ),
                            is_set: true,
                        },
                        delay_ms: MaybeSet {
                            value: 0,
                            is_set: false,
                        },
                        max_speed: MaybeSet {
                            value: FloatOrInt(
                                50.0,
                            ),
                            is_set: true,
                        },
                    },
                    dnd_edge_workspace_switch: DndEdgeWorkspaceSwitch {
                        trigger_height: MaybeSet {
                            value: FloatOrInt(
                                50.0,
                            ),
                            is_set: false,
                        },
                        delay_ms: MaybeSet {
                            value: 100,
                            is_set: false,
                        },
                        max_speed: MaybeSet {
                            value: FloatOrInt(
                                1500.0,
                            ),
                            is_set: false,
                        },
                    },
                    hot_corners: HotCorners {
                        off: false,
                    },
                },
            ],
            overview: Overview {
                zoom: FloatOrInt(
                    0.5,
                ),
                backdrop_color: Color {
                    r: 0.15,
                    g: 0.15,
                    b: 0.15,
                    a: 1.0,
                },
                workspace_shadow: WorkspaceShadow {
                    off: false,
                    offset: ShadowOffset {
                        x: FloatOrInt(
                            0.0,
                        ),
                        y: FloatOrInt(
                            10.0,
                        ),
                    },
                    softness: FloatOrInt(
                        40.0,
                    ),
                    spread: FloatOrInt(
                        10.0,
                    ),
                    color: Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.3137255,
                    },
                },
            },
            environment_list: [
                Environment(
                    [
                        EnvironmentVariable {
                            name: "QT_QPA_PLATFORM",
                            value: Some(
                                "wayland",
                            ),
                        },
                        EnvironmentVariable {
                            name: "DISPLAY",
                            value: None,
                        },
                    ],
                ),
            ],
            xwayland_satellite: XwaylandSatellite {
                off: false,
                path: "xwayland-satellite",
            },
            window_rules: [
                WindowRule {
                    matches: [
                        Match {
                            app_id: Some(
                                RegexEq(
                                    Regex(
                                        ".*alacritty",
                                    ),
                                ),
                            ),
                            title: None,
                            is_active: None,
                            is_focused: None,
                            is_active_in_column: None,
                            is_floating: None,
                            is_window_cast_target: None,
                            is_urgent: None,
                            at_startup: None,
                        },
                    ],
                    excludes: [
                        Match {
                            app_id: None,
                            title: Some(
                                RegexEq(
                                    Regex(
                                        "~",
                                    ),
                                ),
                            ),
                            is_active: None,
                            is_focused: None,
                            is_active_in_column: None,
                            is_floating: None,
                            is_window_cast_target: None,
                            is_urgent: None,
                            at_startup: None,
                        },
                        Match {
                            app_id: None,
                            title: None,
                            is_active: Some(
                                true,
                            ),
                            is_focused: Some(
                                false,
                            ),
                            is_active_in_column: None,
                            is_floating: None,
                            is_window_cast_target: None,
                            is_urgent: None,
                            at_startup: None,
                        },
                    ],
                    default_column_width: None,
                    default_window_height: Some(
                        DefaultPresetSize(
                            Some(
                                Fixed(
                                    500,
                                ),
                            ),
                        ),
                    ),
                    open_on_output: Some(
                        "eDP-1",
                    ),
                    open_on_workspace: None,
                    open_maximized: Some(
                        true,
                    ),
                    open_fullscreen: Some(
                        false,
                    ),
                    open_floating: Some(
                        false,
                    ),
                    open_focused: Some(
                        true,
                    ),
                    min_width: None,
                    min_height: None,
                    max_width: None,
                    max_height: None,
                    focus_ring: BorderRule {
                        off: true,
                        on: false,
                        width: Some(
                            FloatOrInt(
                                3.0,
                            ),
                        ),
                        active_color: None,
                        inactive_color: None,
                        urgent_color: None,
                        active_gradient: None,
                        inactive_gradient: None,
                        urgent_gradient: None,
                    },
                    border: BorderRule {
                        off: false,
                        on: true,
                        width: Some(
                            FloatOrInt(
                                8.5,
                            ),
                        ),
                        active_color: None,
                        inactive_color: None,
                        urgent_color: None,
                        active_gradient: None,
                        inactive_gradient: None,
                        urgent_gradient: None,
                    },
                    shadow: ShadowRule {
                        off: false,
                        on: false,
                        offset: None,
                        softness: None,
                        spread: None,
                        draw_behind_window: None,
                        color: None,
                        inactive_color: None,
                    },
                    tab_indicator: TabIndicatorRule {
                        active_color: Some(
                            Color {
                                r: 1.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            },
                        ),
                        inactive_color: None,
                        urgent_color: None,
                        active_gradient: None,
                        inactive_gradient: None,
                        urgent_gradient: None,
                    },
                    draw_border_with_background: None,
                    opacity: None,
                    geometry_corner_radius: None,
                    clip_to_geometry: None,
                    baba_is_float: None,
                    block_out_from: None,
                    variable_refresh_rate: None,
                    default_column_display: Some(
                        Tabbed,
                    ),
                    default_floating_position: Some(
                        FloatingPosition {
                            x: FloatOrInt(
                                100.0,
                            ),
                            y: FloatOrInt(
                                -200.0,
                            ),
                            relative_to: BottomLeft,
                        },
                    ),
                    scroll_factor: None,
                    tiled_state: None,
                },
            ],
            layer_rules: [
                LayerRule {
                    matches: [
                        Match {
                            namespace: Some(
                                RegexEq(
                                    Regex(
                                        "^notifications$",
                                    ),
                                ),
                            ),
                            at_startup: None,
                        },
                    ],
                    excludes: [],
                    opacity: None,
                    block_out_from: Some(
                        Screencast,
                    ),
                    shadow: ShadowRule {
                        off: false,
                        on: false,
                        offset: None,
                        softness: None,
                        spread: None,
                        draw_behind_window: None,
                        color: None,
                        inactive_color: None,
                    },
                    geometry_corner_radius: None,
                    place_within_backdrop: None,
                    baba_is_float: None,
                },
            ],
            binds_list: [
                Binds(
                    [
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_Escape,
                                ),
                                modifiers: Modifiers(
                                    COMPOSITOR,
                                ),
                            },
                            action: ToggleKeyboardShortcutsInhibit,
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: false,
                            hotkey_overlay_title: Some(
                                Some(
                                    "Inhibit",
                                ),
                            ),
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_Escape,
                                ),
                                modifiers: Modifiers(
                                    SHIFT | COMPOSITOR,
                                ),
                            },
                            action: ToggleKeyboardShortcutsInhibit,
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: false,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_t,
                                ),
                                modifiers: Modifiers(
                                    COMPOSITOR,
                                ),
                            },
                            action: Spawn(
                                [
                                    "alacritty",
                                ],
                            ),
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: true,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_q,
                                ),
                                modifiers: Modifiers(
                                    COMPOSITOR,
                                ),
                            },
                            action: CloseWindow,
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: Some(
                                None,
                            ),
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_h,
                                ),
                                modifiers: Modifiers(
                                    SHIFT | COMPOSITOR,
                                ),
                            },
                            action: FocusMonitorLeft,
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_o,
                                ),
                                modifiers: Modifiers(
                                    SHIFT | COMPOSITOR,
                                ),
                            },
                            action: FocusMonitor(
                                "eDP-1",
                            ),
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_l,
                                ),
                                modifiers: Modifiers(
                                    CTRL | SHIFT | COMPOSITOR,
                                ),
                            },
                            action: MoveWindowToMonitorRight,
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_o,
                                ),
                                modifiers: Modifiers(
                                    CTRL | ALT | COMPOSITOR,
                                ),
                            },
                            action: MoveWindowToMonitor(
                                "eDP-1",
                            ),
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_p,
                                ),
                                modifiers: Modifiers(
                                    CTRL | ALT | COMPOSITOR,
                                ),
                            },
                            action: MoveColumnToMonitor(
                                "DP-1",
                            ),
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_comma,
                                ),
                                modifiers: Modifiers(
                                    COMPOSITOR,
                                ),
                            },
                            action: ConsumeWindowIntoColumn,
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_1,
                                ),
                                modifiers: Modifiers(
                                    COMPOSITOR,
                                ),
                            },
                            action: FocusWorkspace(
                                Index(
                                    1,
                                ),
                            ),
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_1,
                                ),
                                modifiers: Modifiers(
                                    SHIFT | COMPOSITOR,
                                ),
                            },
                            action: FocusWorkspace(
                                Name(
                                    "workspace-1",
                                ),
                            ),
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: Keysym(
                                    XK_e,
                                ),
                                modifiers: Modifiers(
                                    SHIFT | COMPOSITOR,
                                ),
                            },
                            action: Quit(
                                true,
                            ),
                            repeat: true,
                            cooldown: None,
                            allow_when_locked: false,
                            allow_inhibiting: false,
                            hotkey_overlay_title: None,
                        },
                        Bind {
                            key: Key {
                                trigger: WheelScrollDown,
                                modifiers: Modifiers(
                                    COMPOSITOR,
                                ),
                            },
                            action: FocusWorkspaceDown,
                            repeat: true,
                            cooldown: Some(
                                150ms,
                            ),
                            allow_when_locked: false,
                            allow_inhibiting: true,
                            hotkey_overlay_title: None,
                        },
                    ],
                ),
            ],
            switch_events: SwitchBinds {
                lid_open: None,
                lid_close: None,
                tablet_mode_on: Some(
                    SwitchAction {
                        spawn: [
                            "bash",
                            "-c",
                            "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled true",
                        ],
                    },
                ),
                tablet_mode_off: Some(
                    SwitchAction {
                        spawn: [
                            "bash",
                            "-c",
                            "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled false",
                        ],
                    },
                ),
            },
            debug_list: [
                DebugConfig {
                    preview_render: None,
                    dbus_interfaces_in_non_session_instances: false,
                    wait_for_frame_completion_before_queueing: false,
                    enable_overlay_planes: false,
                    disable_cursor_plane: false,
                    disable_direct_scanout: false,
                    restrict_primary_scanout_to_matching_format: false,
                    render_drm_device: Some(
                        "/dev/dri/renderD129",
                    ),
                    force_pipewire_invalid_modifier: false,
                    emulate_zero_presentation_time: false,
                    disable_resize_throttling: false,
                    disable_transactions: false,
                    keep_laptop_panel_on_when_lid_is_closed: false,
                    disable_monitor_names: false,
                    strict_new_window_focus_policy: false,
                    honor_xdg_activation_with_invalid_serial: false,
                    deactivate_unfocused_windows: false,
                    skip_cursor_only_updates_during_vrr: false,
                },
            ],
            workspaces: [
                Workspace {
                    name: WorkspaceName(
                        "workspace-1",
                    ),
                    open_on_output: Some(
                        "eDP-1",
                    ),
                },
                Workspace {
                    name: WorkspaceName(
                        "workspace-2",
                    ),
                    open_on_output: None,
                },
                Workspace {
                    name: WorkspaceName(
                        "workspace-3",
                    ),
                    open_on_output: None,
                },
            ],
        }
        "#);
    }

    #[test]
    fn can_create_default_config() {
        let _ = Config::default();
    }

    #[test]
    fn layout_merging_works() {
        let config = do_parse(
            r##"
            layout {
                gaps 10
            }
            layout {
                gaps 20
            }
        "##,
        );

        let merged_layout = config.layout();
        assert_eq!(merged_layout.gaps.get().0, 20.0);
    }

    #[test]
    fn environment_merging_works() {
        let config = do_parse(
            r##"
            environment {
                XDG_CURRENT_DESKTOP "niri"
                DISPLAY ":0"
            }
            environment {
                XDG_CURRENT_DESKTOP "niri-merged"
                WAYLAND_DISPLAY "wayland-1"
            }
        "##,
        );

        let merged_env = config.environment();
        assert_eq!(merged_env.0.len(), 3);

        // XDG_CURRENT_DESKTOP should be overridden to "niri-merged"
        let xdg_desktop = merged_env
            .0
            .iter()
            .find(|v| v.name == "XDG_CURRENT_DESKTOP")
            .unwrap();
        assert_eq!(xdg_desktop.value, Some("niri-merged".to_string()));

        // DISPLAY should remain ":0"
        let display = merged_env.0.iter().find(|v| v.name == "DISPLAY").unwrap();
        assert_eq!(display.value, Some(":0".to_string()));

        // WAYLAND_DISPLAY should be added
        let wayland_display = merged_env
            .0
            .iter()
            .find(|v| v.name == "WAYLAND_DISPLAY")
            .unwrap();
        assert_eq!(wayland_display.value, Some("wayland-1".to_string()));
    }

    #[test]
    fn animations_merging_works() {
        let config = do_parse(
            r##"
            animations {
                slowdown 2.0
                window-open {
                    duration-ms 200
                }
            }
            animations {
                window-open {
                    duration-ms 300
                    curve "ease-out-expo"
                }
                window-close {
                    duration-ms 150
                }
            }
        "##,
        );

        let merged_animations = config.animations();
        assert_eq!(merged_animations.slowdown.get().0, 2.0);

        // Window open should be merged with later values taking precedence
        match merged_animations.window_open.anim.kind {
            crate::animation::AnimationKind::Easing(params) => {
                assert_eq!(params.duration_ms, 300);
                assert_eq!(params.curve, crate::animation::AnimationCurve::EaseOutExpo);
            }
            _ => panic!("Expected easing animation"),
        }

        // Window close should be from the second animations block
        match merged_animations.window_close.anim.kind {
            crate::animation::AnimationKind::Easing(params) => {
                assert_eq!(params.duration_ms, 150);
            }
            _ => panic!("Expected easing animation"),
        }
    }

    #[test]
    fn binds_merging_works() {
        let config = do_parse(
            r##"
            binds {
                Mod+T { spawn "terminal"; }
            }
            binds {
                Mod+Q { close-window; }
                Mod+R { spawn "runner"; }
            }
        "##,
        );

        let merged_binds = config.binds();
        assert_eq!(merged_binds.0.len(), 3);

        // Check that all binds are present by looking for the expected actions
        let actions: Vec<_> = merged_binds.0.iter().map(|bind| &bind.action).collect();

        let has_terminal = actions
            .iter()
            .any(|action| matches!(action, Action::Spawn(command) if command[0] == "terminal"));
        let has_close_window = actions
            .iter()
            .any(|action| matches!(action, Action::CloseWindow));
        let has_runner = actions
            .iter()
            .any(|action| matches!(action, Action::Spawn(command) if command[0] == "runner"));

        assert!(has_terminal);
        assert!(has_close_window);
        assert!(has_runner);
    }

    #[test]
    fn binds_duplicate_key_last_wins() {
        let config = do_parse(
            r##"
            binds {
                Mod+T { spawn "first"; }
                Mod+Q { close-window; }
            }
            binds {
                Mod+T { spawn "second"; }
                Mod+Return { spawn "terminal"; }
            }
        "##,
        );

        let merged_binds = config.binds();
        assert_eq!(merged_binds.0.len(), 3);

        let spawn_actions: Vec<_> = merged_binds
            .0
            .iter()
            .filter_map(|bind| match &bind.action {
                Action::Spawn(command) => Some(&command[0]),
                _ => None,
            })
            .collect();

        assert_eq!(spawn_actions.len(), 2);
        assert!(spawn_actions.contains(&&"second".to_string()));
        assert!(spawn_actions.contains(&&"terminal".to_string()));
        assert!(!spawn_actions.contains(&&"first".to_string()));
    }

    #[test]
    fn debug_merging_works() {
        let config = do_parse(
            r##"
            debug {
                disable-resize-throttling
            }
            debug {
                disable-transactions
            }
        "##,
        );

        let merged_debug = config.debug();
        assert!(merged_debug.disable_resize_throttling);
        assert!(merged_debug.disable_transactions);
    }

    #[test]
    fn semantic_default_comparison_works() {
        let config = do_parse(
            r##"
            animations {
                slowdown 1.0
            }
            animations {
                slowdown 2.0
            }
        "##,
        );

        let merged_animations = config.animations();
        assert_eq!(merged_animations.slowdown.get().0, 2.0);
    }

    #[test]
    fn can_reset_to_default() {
        let config = do_parse(
            r##"
            animations {
                slowdown 2.0
            }
            animations {
                slowdown 1.0
            }
        "##,
        );

        assert_eq!(config.animations().slowdown.get().0, 1.0);
    }

    #[test]
    fn layout_gaps_reset_to_default() {
        let config = do_parse(
            r##"
            layout {
                gaps 32
            }
            layout {
                gaps 16
            }
        "##,
        );

        assert_eq!(config.layout().gaps.get().0, 16.0);
    }

    #[test]
    fn gestures_merging_works() {
        let config = do_parse(
            r##"
            gestures {
                dnd-edge-view-scroll {
                    trigger-width 20
                }
            }
            gestures {
                dnd-edge-view-scroll {
                    delay-ms 100
                }
                hot-corners {
                    off
                }
            }
        "##,
        );

        let merged_gestures = config.gestures();
        assert_eq!(
            merged_gestures.dnd_edge_view_scroll.trigger_width.get().0,
            20.0
        );
        assert_eq!(*merged_gestures.dnd_edge_view_scroll.delay_ms, 100);
        assert!(merged_gestures.hot_corners.off);
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

    #[test]
    fn config_path_explicit_works() {
        // Test that explicit paths work correctly
        let temp_dir = std::env::temp_dir();
        let test_config = temp_dir.join("test_niri_config.kdl");

        // Write a minimal config
        std::fs::write(&test_config, "// test config").unwrap();

        let config_path = ConfigPath::Explicit(test_config.clone());

        // Should be able to load the explicit config
        let result = config_path.load();
        assert!(result.is_ok());

        // Cleanup
        std::fs::remove_file(test_config).unwrap();
    }

    #[test]
    fn config_path_regular_fallback_works() {
        // Test that regular paths with fallback work
        let temp_dir = std::env::temp_dir();
        let user_path = temp_dir.join("nonexistent_user_config.kdl");
        let system_path = temp_dir.join("test_system_config.kdl");

        // Write a minimal config to system path
        std::fs::write(&system_path, "// system config").unwrap();

        let config_path = ConfigPath::Regular {
            user_path: user_path.clone(),
            system_path: system_path.clone(),
        };

        // Should fallback to system path since user path doesn't exist
        let result = config_path.load();
        assert!(result.is_ok());

        // Cleanup
        std::fs::remove_file(system_path).unwrap();
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
}
