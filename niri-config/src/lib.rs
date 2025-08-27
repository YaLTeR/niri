#[macro_use]
extern crate tracing;

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use bitflags::bitflags;
use knuffel::errors::DecodeError;
use miette::{miette, Context, IntoDiagnostic};
use niri_ipc::{
    ColumnDisplay, LayoutSwitchTarget, PositionChange, SizeChange, WorkspaceReferenceArg,
};
use smithay::input::keyboard::keysyms::KEY_NoSymbol;
use smithay::input::keyboard::xkb::{keysym_from_name, KEYSYM_CASE_INSENSITIVE};
use smithay::input::keyboard::Keysym;

pub mod animations;
pub mod appearance;
pub mod debug;
pub mod gestures;
pub mod input;
pub mod layer_rule;
pub mod layout;
pub mod output;
pub mod utils;
pub mod window_rule;

pub use crate::animations::{
    Animation, AnimationCurve, AnimationKind, Animations, EasingParams, SpringParams,
};
pub use crate::appearance::*;
pub use crate::debug::DebugConfig;
pub use crate::gestures::Gestures;
pub use crate::input::{Input, ModKey, ScrollMethod, TrackLayout, WarpMouseToFocusMode, Xkb};
pub use crate::layer_rule::LayerRule;
pub use crate::layout::*;
pub use crate::output::{Output, OutputName, Outputs, Position, Vrr};
pub use crate::window_rule::{FloatingPosition, RelativeTo, WindowRule};

#[derive(knuffel::Decode, Debug, PartialEq)]
pub struct Config {
    #[knuffel(child, default)]
    pub input: Input,
    #[knuffel(children(name = "output"))]
    pub outputs: Outputs,
    #[knuffel(children(name = "spawn-at-startup"))]
    pub spawn_at_startup: Vec<SpawnAtStartup>,
    #[knuffel(children(name = "spawn-sh-at-startup"))]
    pub spawn_sh_at_startup: Vec<SpawnShAtStartup>,
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
    pub clipboard: Clipboard,
    #[knuffel(child, default)]
    pub hotkey_overlay: HotkeyOverlay,
    #[knuffel(child, default)]
    pub config_notification: ConfigNotification,
    #[knuffel(child, default)]
    pub animations: Animations,
    #[knuffel(child, default)]
    pub gestures: Gestures,
    #[knuffel(child, default)]
    pub overview: Overview,
    #[knuffel(child, default)]
    pub environment: Environment,
    #[knuffel(child, default)]
    pub xwayland_satellite: XwaylandSatellite,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Percent(pub f64);

// MIN and MAX generics are only used during parsing to check the value.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FloatOrInt<const MIN: i32, const MAX: i32>(pub f64);

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct SpawnAtStartup {
    #[knuffel(arguments)]
    pub command: Vec<String>,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct SpawnShAtStartup {
    #[knuffel(argument)]
    pub command: String,
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

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyOverlay {
    #[knuffel(child)]
    pub skip_at_startup: bool,
    #[knuffel(child)]
    pub hide_not_bound: bool,
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ConfigNotification {
    #[knuffel(child)]
    pub disable_failed: bool,
}

#[derive(knuffel::Decode, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Clipboard {
    #[knuffel(child)]
    pub disable_primary: bool,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq)]
pub struct Overview {
    #[knuffel(child, unwrap(argument), default = Self::default().zoom)]
    pub zoom: FloatOrInt<0, 1>,
    #[knuffel(child, default = Self::default().backdrop_color)]
    pub backdrop_color: Color,
    #[knuffel(child, default)]
    pub workspace_shadow: appearance::WorkspaceShadow,
}

impl Default for Overview {
    fn default() -> Self {
        Self {
            zoom: FloatOrInt(0.5),
            backdrop_color: appearance::DEFAULT_BACKDROP_COLOR,
            workspace_shadow: appearance::WorkspaceShadow::default(),
        }
    }
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
pub struct XwaylandSatellite {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child, unwrap(argument), default = Self::default().path)]
    pub path: String,
}

impl Default for XwaylandSatellite {
    fn default() -> Self {
        Self {
            off: false,
            path: String::from("xwayland-satellite"),
        }
    }
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

#[derive(Debug, Default, PartialEq)]
pub struct Binds(pub Vec<Bind>);

#[derive(Debug, Clone, PartialEq)]
pub struct Bind {
    pub key: Key,
    pub action: Action,
    pub repeat: bool,
    pub cooldown: Option<Duration>,
    pub allow_when_locked: bool,
    pub allow_inhibiting: bool,
    pub hotkey_overlay_title: Option<Option<String>>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Key {
    pub trigger: Trigger,
    pub modifiers: Modifiers,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Trigger {
    Keysym(Keysym),
    MouseLeft,
    MouseRight,
    MouseMiddle,
    MouseBack,
    MouseForward,
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
    SpawnSh(#[knuffel(argument)] String),
    DoScreenTransition(#[knuffel(property(name = "delay-ms"))] Option<u16>),
    #[knuffel(skip)]
    ConfirmScreenshot {
        write_to_disk: bool,
    },
    #[knuffel(skip)]
    CancelScreenshot,
    #[knuffel(skip)]
    ScreenshotTogglePointer,
    Screenshot(#[knuffel(property(name = "show-pointer"), default = true)] bool),
    ScreenshotScreen(
        #[knuffel(property(name = "write-to-disk"), default = true)] bool,
        #[knuffel(property(name = "show-pointer"), default = true)] bool,
    ),
    ScreenshotWindow(#[knuffel(property(name = "write-to-disk"), default = true)] bool),
    #[knuffel(skip)]
    ScreenshotWindowById {
        id: u64,
        write_to_disk: bool,
    },
    ToggleKeyboardShortcutsInhibit,
    CloseWindow,
    #[knuffel(skip)]
    CloseWindowById(u64),
    FullscreenWindow,
    #[knuffel(skip)]
    FullscreenWindowById(u64),
    ToggleWindowedFullscreen,
    #[knuffel(skip)]
    ToggleWindowedFullscreenById(u64),
    #[knuffel(skip)]
    FocusWindow(u64),
    FocusWindowInColumn(#[knuffel(argument)] u8),
    FocusWindowPrevious,
    FocusColumnLeft,
    #[knuffel(skip)]
    FocusColumnLeftUnderMouse,
    FocusColumnRight,
    #[knuffel(skip)]
    FocusColumnRightUnderMouse,
    FocusColumnFirst,
    FocusColumnLast,
    FocusColumnRightOrFirst,
    FocusColumnLeftOrLast,
    FocusColumn(#[knuffel(argument)] usize),
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
    FocusWindowTop,
    FocusWindowBottom,
    FocusWindowDownOrTop,
    FocusWindowUpOrBottom,
    MoveColumnLeft,
    MoveColumnRight,
    MoveColumnToFirst,
    MoveColumnToLast,
    MoveColumnLeftOrToMonitorLeft,
    MoveColumnRightOrToMonitorRight,
    MoveColumnToIndex(#[knuffel(argument)] usize),
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
    SwapWindowLeft,
    SwapWindowRight,
    ToggleColumnTabbedDisplay,
    SetColumnDisplay(#[knuffel(argument, str)] ColumnDisplay),
    CenterColumn,
    CenterWindow,
    #[knuffel(skip)]
    CenterWindowById(u64),
    CenterVisibleColumns,
    FocusWorkspaceDown,
    #[knuffel(skip)]
    FocusWorkspaceDownUnderMouse,
    FocusWorkspaceUp,
    #[knuffel(skip)]
    FocusWorkspaceUpUnderMouse,
    FocusWorkspace(#[knuffel(argument)] WorkspaceReference),
    FocusWorkspacePrevious,
    MoveWindowToWorkspaceDown(#[knuffel(property(name = "focus"), default = true)] bool),
    MoveWindowToWorkspaceUp(#[knuffel(property(name = "focus"), default = true)] bool),
    MoveWindowToWorkspace(
        #[knuffel(argument)] WorkspaceReference,
        #[knuffel(property(name = "focus"), default = true)] bool,
    ),
    #[knuffel(skip)]
    MoveWindowToWorkspaceById {
        window_id: u64,
        reference: WorkspaceReference,
        focus: bool,
    },
    MoveColumnToWorkspaceDown(#[knuffel(property(name = "focus"), default = true)] bool),
    MoveColumnToWorkspaceUp(#[knuffel(property(name = "focus"), default = true)] bool),
    MoveColumnToWorkspace(
        #[knuffel(argument)] WorkspaceReference,
        #[knuffel(property(name = "focus"), default = true)] bool,
    ),
    MoveWorkspaceDown,
    MoveWorkspaceUp,
    MoveWorkspaceToIndex(#[knuffel(argument)] usize),
    #[knuffel(skip)]
    MoveWorkspaceToIndexByRef {
        new_idx: usize,
        reference: WorkspaceReference,
    },
    #[knuffel(skip)]
    MoveWorkspaceToMonitorByRef {
        output_name: String,
        reference: WorkspaceReference,
    },
    MoveWorkspaceToMonitor(#[knuffel(argument)] String),
    SetWorkspaceName(#[knuffel(argument)] String),
    #[knuffel(skip)]
    SetWorkspaceNameByRef {
        name: String,
        reference: WorkspaceReference,
    },
    UnsetWorkspaceName,
    #[knuffel(skip)]
    UnsetWorkSpaceNameByRef(#[knuffel(argument)] WorkspaceReference),
    FocusMonitorLeft,
    FocusMonitorRight,
    FocusMonitorDown,
    FocusMonitorUp,
    FocusMonitorPrevious,
    FocusMonitorNext,
    FocusMonitor(#[knuffel(argument)] String),
    MoveWindowToMonitorLeft,
    MoveWindowToMonitorRight,
    MoveWindowToMonitorDown,
    MoveWindowToMonitorUp,
    MoveWindowToMonitorPrevious,
    MoveWindowToMonitorNext,
    MoveWindowToMonitor(#[knuffel(argument)] String),
    #[knuffel(skip)]
    MoveWindowToMonitorById {
        id: u64,
        output: String,
    },
    MoveColumnToMonitorLeft,
    MoveColumnToMonitorRight,
    MoveColumnToMonitorDown,
    MoveColumnToMonitorUp,
    MoveColumnToMonitorPrevious,
    MoveColumnToMonitorNext,
    MoveColumnToMonitor(#[knuffel(argument)] String),
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
    ExpandColumnToAvailableWidth,
    SwitchLayout(#[knuffel(argument, str)] LayoutSwitchTarget),
    ShowHotkeyOverlay,
    MoveWorkspaceToMonitorLeft,
    MoveWorkspaceToMonitorRight,
    MoveWorkspaceToMonitorDown,
    MoveWorkspaceToMonitorUp,
    MoveWorkspaceToMonitorPrevious,
    MoveWorkspaceToMonitorNext,
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
    ToggleWindowRuleOpacity,
    #[knuffel(skip)]
    ToggleWindowRuleOpacityById(u64),
    SetDynamicCastWindow,
    #[knuffel(skip)]
    SetDynamicCastWindowById(u64),
    SetDynamicCastMonitor(#[knuffel(argument)] Option<String>),
    ClearDynamicCastTarget,
    ToggleOverview,
    OpenOverview,
    CloseOverview,
    #[knuffel(skip)]
    ToggleWindowUrgent(u64),
    #[knuffel(skip)]
    SetWindowUrgent(u64),
    #[knuffel(skip)]
    UnsetWindowUrgent(u64),
    #[knuffel(skip)]
    LoadConfigFile,
}

impl From<niri_ipc::Action> for Action {
    fn from(value: niri_ipc::Action) -> Self {
        match value {
            niri_ipc::Action::Quit { skip_confirmation } => Self::Quit(skip_confirmation),
            niri_ipc::Action::PowerOffMonitors {} => Self::PowerOffMonitors,
            niri_ipc::Action::PowerOnMonitors {} => Self::PowerOnMonitors,
            niri_ipc::Action::Spawn { command } => Self::Spawn(command),
            niri_ipc::Action::SpawnSh { command } => Self::SpawnSh(command),
            niri_ipc::Action::DoScreenTransition { delay_ms } => Self::DoScreenTransition(delay_ms),
            niri_ipc::Action::Screenshot { show_pointer } => Self::Screenshot(show_pointer),
            niri_ipc::Action::ScreenshotScreen {
                write_to_disk,
                show_pointer,
            } => Self::ScreenshotScreen(write_to_disk, show_pointer),
            niri_ipc::Action::ScreenshotWindow {
                id: None,
                write_to_disk,
            } => Self::ScreenshotWindow(write_to_disk),
            niri_ipc::Action::ScreenshotWindow {
                id: Some(id),
                write_to_disk,
            } => Self::ScreenshotWindowById { id, write_to_disk },
            niri_ipc::Action::ToggleKeyboardShortcutsInhibit {} => {
                Self::ToggleKeyboardShortcutsInhibit
            }
            niri_ipc::Action::CloseWindow { id: None } => Self::CloseWindow,
            niri_ipc::Action::CloseWindow { id: Some(id) } => Self::CloseWindowById(id),
            niri_ipc::Action::FullscreenWindow { id: None } => Self::FullscreenWindow,
            niri_ipc::Action::FullscreenWindow { id: Some(id) } => Self::FullscreenWindowById(id),
            niri_ipc::Action::ToggleWindowedFullscreen { id: None } => {
                Self::ToggleWindowedFullscreen
            }
            niri_ipc::Action::ToggleWindowedFullscreen { id: Some(id) } => {
                Self::ToggleWindowedFullscreenById(id)
            }
            niri_ipc::Action::FocusWindow { id } => Self::FocusWindow(id),
            niri_ipc::Action::FocusWindowInColumn { index } => Self::FocusWindowInColumn(index),
            niri_ipc::Action::FocusWindowPrevious {} => Self::FocusWindowPrevious,
            niri_ipc::Action::FocusColumnLeft {} => Self::FocusColumnLeft,
            niri_ipc::Action::FocusColumnRight {} => Self::FocusColumnRight,
            niri_ipc::Action::FocusColumnFirst {} => Self::FocusColumnFirst,
            niri_ipc::Action::FocusColumnLast {} => Self::FocusColumnLast,
            niri_ipc::Action::FocusColumnRightOrFirst {} => Self::FocusColumnRightOrFirst,
            niri_ipc::Action::FocusColumnLeftOrLast {} => Self::FocusColumnLeftOrLast,
            niri_ipc::Action::FocusColumn { index } => Self::FocusColumn(index),
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
            niri_ipc::Action::FocusWindowTop {} => Self::FocusWindowTop,
            niri_ipc::Action::FocusWindowBottom {} => Self::FocusWindowBottom,
            niri_ipc::Action::FocusWindowDownOrTop {} => Self::FocusWindowDownOrTop,
            niri_ipc::Action::FocusWindowUpOrBottom {} => Self::FocusWindowUpOrBottom,
            niri_ipc::Action::MoveColumnLeft {} => Self::MoveColumnLeft,
            niri_ipc::Action::MoveColumnRight {} => Self::MoveColumnRight,
            niri_ipc::Action::MoveColumnToFirst {} => Self::MoveColumnToFirst,
            niri_ipc::Action::MoveColumnToLast {} => Self::MoveColumnToLast,
            niri_ipc::Action::MoveColumnToIndex { index } => Self::MoveColumnToIndex(index),
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
            niri_ipc::Action::SwapWindowRight {} => Self::SwapWindowRight,
            niri_ipc::Action::SwapWindowLeft {} => Self::SwapWindowLeft,
            niri_ipc::Action::ToggleColumnTabbedDisplay {} => Self::ToggleColumnTabbedDisplay,
            niri_ipc::Action::SetColumnDisplay { display } => Self::SetColumnDisplay(display),
            niri_ipc::Action::CenterColumn {} => Self::CenterColumn,
            niri_ipc::Action::CenterWindow { id: None } => Self::CenterWindow,
            niri_ipc::Action::CenterWindow { id: Some(id) } => Self::CenterWindowById(id),
            niri_ipc::Action::CenterVisibleColumns {} => Self::CenterVisibleColumns,
            niri_ipc::Action::FocusWorkspaceDown {} => Self::FocusWorkspaceDown,
            niri_ipc::Action::FocusWorkspaceUp {} => Self::FocusWorkspaceUp,
            niri_ipc::Action::FocusWorkspace { reference } => {
                Self::FocusWorkspace(WorkspaceReference::from(reference))
            }
            niri_ipc::Action::FocusWorkspacePrevious {} => Self::FocusWorkspacePrevious,
            niri_ipc::Action::MoveWindowToWorkspaceDown { focus } => {
                Self::MoveWindowToWorkspaceDown(focus)
            }
            niri_ipc::Action::MoveWindowToWorkspaceUp { focus } => {
                Self::MoveWindowToWorkspaceUp(focus)
            }
            niri_ipc::Action::MoveWindowToWorkspace {
                window_id: None,
                reference,
                focus,
            } => Self::MoveWindowToWorkspace(WorkspaceReference::from(reference), focus),
            niri_ipc::Action::MoveWindowToWorkspace {
                window_id: Some(window_id),
                reference,
                focus,
            } => Self::MoveWindowToWorkspaceById {
                window_id,
                reference: WorkspaceReference::from(reference),
                focus,
            },
            niri_ipc::Action::MoveColumnToWorkspaceDown { focus } => {
                Self::MoveColumnToWorkspaceDown(focus)
            }
            niri_ipc::Action::MoveColumnToWorkspaceUp { focus } => {
                Self::MoveColumnToWorkspaceUp(focus)
            }
            niri_ipc::Action::MoveColumnToWorkspace { reference, focus } => {
                Self::MoveColumnToWorkspace(WorkspaceReference::from(reference), focus)
            }
            niri_ipc::Action::MoveWorkspaceDown {} => Self::MoveWorkspaceDown,
            niri_ipc::Action::MoveWorkspaceUp {} => Self::MoveWorkspaceUp,
            niri_ipc::Action::SetWorkspaceName {
                name,
                workspace: None,
            } => Self::SetWorkspaceName(name),
            niri_ipc::Action::SetWorkspaceName {
                name,
                workspace: Some(reference),
            } => Self::SetWorkspaceNameByRef {
                name,
                reference: WorkspaceReference::from(reference),
            },
            niri_ipc::Action::UnsetWorkspaceName { reference: None } => Self::UnsetWorkspaceName,
            niri_ipc::Action::UnsetWorkspaceName {
                reference: Some(reference),
            } => Self::UnsetWorkSpaceNameByRef(WorkspaceReference::from(reference)),
            niri_ipc::Action::FocusMonitorLeft {} => Self::FocusMonitorLeft,
            niri_ipc::Action::FocusMonitorRight {} => Self::FocusMonitorRight,
            niri_ipc::Action::FocusMonitorDown {} => Self::FocusMonitorDown,
            niri_ipc::Action::FocusMonitorUp {} => Self::FocusMonitorUp,
            niri_ipc::Action::FocusMonitorPrevious {} => Self::FocusMonitorPrevious,
            niri_ipc::Action::FocusMonitorNext {} => Self::FocusMonitorNext,
            niri_ipc::Action::FocusMonitor { output } => Self::FocusMonitor(output),
            niri_ipc::Action::MoveWindowToMonitorLeft {} => Self::MoveWindowToMonitorLeft,
            niri_ipc::Action::MoveWindowToMonitorRight {} => Self::MoveWindowToMonitorRight,
            niri_ipc::Action::MoveWindowToMonitorDown {} => Self::MoveWindowToMonitorDown,
            niri_ipc::Action::MoveWindowToMonitorUp {} => Self::MoveWindowToMonitorUp,
            niri_ipc::Action::MoveWindowToMonitorPrevious {} => Self::MoveWindowToMonitorPrevious,
            niri_ipc::Action::MoveWindowToMonitorNext {} => Self::MoveWindowToMonitorNext,
            niri_ipc::Action::MoveWindowToMonitor { id: None, output } => {
                Self::MoveWindowToMonitor(output)
            }
            niri_ipc::Action::MoveWindowToMonitor {
                id: Some(id),
                output,
            } => Self::MoveWindowToMonitorById { id, output },
            niri_ipc::Action::MoveColumnToMonitorLeft {} => Self::MoveColumnToMonitorLeft,
            niri_ipc::Action::MoveColumnToMonitorRight {} => Self::MoveColumnToMonitorRight,
            niri_ipc::Action::MoveColumnToMonitorDown {} => Self::MoveColumnToMonitorDown,
            niri_ipc::Action::MoveColumnToMonitorUp {} => Self::MoveColumnToMonitorUp,
            niri_ipc::Action::MoveColumnToMonitorPrevious {} => Self::MoveColumnToMonitorPrevious,
            niri_ipc::Action::MoveColumnToMonitorNext {} => Self::MoveColumnToMonitorNext,
            niri_ipc::Action::MoveColumnToMonitor { output } => Self::MoveColumnToMonitor(output),
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
            niri_ipc::Action::ExpandColumnToAvailableWidth {} => Self::ExpandColumnToAvailableWidth,
            niri_ipc::Action::SwitchLayout { layout } => Self::SwitchLayout(layout),
            niri_ipc::Action::ShowHotkeyOverlay {} => Self::ShowHotkeyOverlay,
            niri_ipc::Action::MoveWorkspaceToMonitorLeft {} => Self::MoveWorkspaceToMonitorLeft,
            niri_ipc::Action::MoveWorkspaceToMonitorRight {} => Self::MoveWorkspaceToMonitorRight,
            niri_ipc::Action::MoveWorkspaceToMonitorDown {} => Self::MoveWorkspaceToMonitorDown,
            niri_ipc::Action::MoveWorkspaceToMonitorUp {} => Self::MoveWorkspaceToMonitorUp,
            niri_ipc::Action::MoveWorkspaceToMonitorPrevious {} => {
                Self::MoveWorkspaceToMonitorPrevious
            }
            niri_ipc::Action::MoveWorkspaceToIndex {
                index,
                reference: Some(reference),
            } => Self::MoveWorkspaceToIndexByRef {
                new_idx: index,
                reference: WorkspaceReference::from(reference),
            },
            niri_ipc::Action::MoveWorkspaceToIndex {
                index,
                reference: None,
            } => Self::MoveWorkspaceToIndex(index),
            niri_ipc::Action::MoveWorkspaceToMonitor {
                output,
                reference: Some(reference),
            } => Self::MoveWorkspaceToMonitorByRef {
                output_name: output,
                reference: WorkspaceReference::from(reference),
            },
            niri_ipc::Action::MoveWorkspaceToMonitor {
                output,
                reference: None,
            } => Self::MoveWorkspaceToMonitor(output),
            niri_ipc::Action::MoveWorkspaceToMonitorNext {} => Self::MoveWorkspaceToMonitorNext,
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
            niri_ipc::Action::ToggleWindowRuleOpacity { id: None } => Self::ToggleWindowRuleOpacity,
            niri_ipc::Action::ToggleWindowRuleOpacity { id: Some(id) } => {
                Self::ToggleWindowRuleOpacityById(id)
            }
            niri_ipc::Action::SetDynamicCastWindow { id: None } => Self::SetDynamicCastWindow,
            niri_ipc::Action::SetDynamicCastWindow { id: Some(id) } => {
                Self::SetDynamicCastWindowById(id)
            }
            niri_ipc::Action::SetDynamicCastMonitor { output } => {
                Self::SetDynamicCastMonitor(output)
            }
            niri_ipc::Action::ClearDynamicCastTarget {} => Self::ClearDynamicCastTarget,
            niri_ipc::Action::ToggleOverview {} => Self::ToggleOverview,
            niri_ipc::Action::OpenOverview {} => Self::OpenOverview,
            niri_ipc::Action::CloseOverview {} => Self::CloseOverview,
            niri_ipc::Action::ToggleWindowUrgent { id } => Self::ToggleWindowUrgent(id),
            niri_ipc::Action::SetWindowUrgent { id } => Self::SetWindowUrgent(id),
            niri_ipc::Action::UnsetWindowUrgent { id } => Self::UnsetWindowUrgent(id),
            niri_ipc::Action::LoadConfigFile {} => Self::LoadConfigFile,
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

impl Config {
    pub fn load(path: &Path) -> miette::Result<Self> {
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

impl Default for Config {
    fn default() -> Self {
        Config::parse(
            "default-config.kdl",
            include_str!("../../resources/default-config.kdl"),
        )
        .unwrap()
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
                        format!("duplicate named workspace: {s}"),
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
        let mut allow_inhibiting = true;
        let mut hotkey_overlay_title = None;
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
                "allow-inhibiting" => {
                    allow_inhibiting = knuffel::traits::DecodeScalar::decode(val, ctx)?;
                }
                "hotkey-overlay-title" => {
                    hotkey_overlay_title = Some(knuffel::traits::DecodeScalar::decode(val, ctx)?);
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
            allow_inhibiting: true,
            hotkey_overlay_title: None,
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
                    if !matches!(action, Action::Spawn(_) | Action::SpawnSh(_)) {
                        if let Some(node) = allow_when_locked_node {
                            ctx.emit_error(DecodeError::unexpected(
                                node,
                                "property",
                                "allow-when-locked can only be set on spawn binds",
                            ));
                        }
                    }

                    // The toggle-inhibit action must always be uninhibitable.
                    // Otherwise, it would be impossible to trigger it.
                    if matches!(action, Action::ToggleKeyboardShortcutsInhibit) {
                        allow_inhibiting = false;
                    }

                    Ok(Self {
                        key,
                        action,
                        repeat,
                        cooldown,
                        allow_when_locked,
                        allow_inhibiting,
                        hotkey_overlay_title,
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

        let trigger = if key.eq_ignore_ascii_case("MouseLeft") {
            Trigger::MouseLeft
        } else if key.eq_ignore_ascii_case("MouseRight") {
            Trigger::MouseRight
        } else if key.eq_ignore_ascii_case("MouseMiddle") {
            Trigger::MouseMiddle
        } else if key.eq_ignore_ascii_case("MouseBack") {
            Trigger::MouseBack
        } else if key.eq_ignore_ascii_case("MouseForward") {
            Trigger::MouseForward
        } else if key.eq_ignore_ascii_case("WheelScrollDown") {
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

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use pretty_assertions::assert_eq;

    use super::*;

    #[track_caller]
    fn do_parse(text: &str) -> Config {
        Config::parse("test.kdl", text)
            .map_err(miette::Report::new)
            .unwrap()
    }

    #[test]
    fn parse_scroll_factor_combined() {
        // Test combined scroll-factor syntax
        let parsed = do_parse(
            r#"
            input {
                mouse {
                    scroll-factor 2.0
                }
                touchpad {
                    scroll-factor 1.5
                }
            }
            "#,
        );

        assert_debug_snapshot!(parsed.input.mouse.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: Some(
                    FloatOrInt(
                        2.0,
                    ),
                ),
                horizontal: None,
                vertical: None,
            },
        )
        "#);
        assert_debug_snapshot!(parsed.input.touchpad.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: Some(
                    FloatOrInt(
                        1.5,
                    ),
                ),
                horizontal: None,
                vertical: None,
            },
        )
        "#);
    }

    #[test]
    fn parse_scroll_factor_split() {
        // Test split horizontal/vertical syntax
        let parsed = do_parse(
            r#"
            input {
                mouse {
                    scroll-factor horizontal=2.0 vertical=-1.0
                }
                touchpad {
                    scroll-factor horizontal=-1.5 vertical=0.5
                }
            }
            "#,
        );

        assert_debug_snapshot!(parsed.input.mouse.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: None,
                horizontal: Some(
                    FloatOrInt(
                        2.0,
                    ),
                ),
                vertical: Some(
                    FloatOrInt(
                        -1.0,
                    ),
                ),
            },
        )
        "#);
        assert_debug_snapshot!(parsed.input.touchpad.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: None,
                horizontal: Some(
                    FloatOrInt(
                        -1.5,
                    ),
                ),
                vertical: Some(
                    FloatOrInt(
                        0.5,
                    ),
                ),
            },
        )
        "#);
    }

    #[test]
    fn parse_scroll_factor_partial() {
        // Test partial specification (only one axis)
        let parsed = do_parse(
            r#"
            input {
                mouse {
                    scroll-factor horizontal=2.0
                }
                touchpad {
                    scroll-factor vertical=-1.5
                }
            }
            "#,
        );

        assert_debug_snapshot!(parsed.input.mouse.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: None,
                horizontal: Some(
                    FloatOrInt(
                        2.0,
                    ),
                ),
                vertical: None,
            },
        )
        "#);
        assert_debug_snapshot!(parsed.input.touchpad.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: None,
                horizontal: None,
                vertical: Some(
                    FloatOrInt(
                        -1.5,
                    ),
                ),
            },
        )
        "#);
    }

    #[test]
    fn parse_scroll_factor_mixed() {
        // Test mixed base + override syntax
        let parsed = do_parse(
            r#"
            input {
                mouse {
                    scroll-factor 2 vertical=-1
                }
                touchpad {
                    scroll-factor 1.5 horizontal=3
                }
            }
            "#,
        );

        assert_debug_snapshot!(parsed.input.mouse.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: Some(
                    FloatOrInt(
                        2.0,
                    ),
                ),
                horizontal: None,
                vertical: Some(
                    FloatOrInt(
                        -1.0,
                    ),
                ),
            },
        )
        "#);
        assert_debug_snapshot!(parsed.input.touchpad.scroll_factor, @r#"
        Some(
            ScrollFactor {
                base: Some(
                    FloatOrInt(
                        1.5,
                    ),
                ),
                horizontal: Some(
                    FloatOrInt(
                        3.0,
                    ),
                ),
                vertical: None,
            },
        )
        "#);
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
            spawn-sh-at-startup "qs -c ~/source/qs/MyAwesomeShell"

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
                Super+Alt+S allow-when-locked=true { spawn-sh "pkill orca || exec orca"; }
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
                        ScrollFactor {
                            base: Some(
                                FloatOrInt(
                                    0.9,
                                ),
                            ),
                            horizontal: None,
                            vertical: None,
                        },
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
                        ScrollFactor {
                            base: Some(
                                FloatOrInt(
                                    0.2,
                                ),
                            ),
                            horizontal: None,
                            vertical: None,
                        },
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
            spawn_sh_at_startup: [
                SpawnShAtStartup {
                    command: "qs -c ~/source/qs/MyAwesomeShell",
                },
            ],
            layout: Layout {
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
                gaps: FloatOrInt(
                    8.0,
                ),
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
            config_notification: ConfigNotification {
                disable_failed: false,
            },
            animations: Animations {
                off: false,
                slowdown: FloatOrInt(
                    2.0,
                ),
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
                exit_confirmation_open_close: ExitConfirmationOpenCloseAnim(
                    Animation {
                        off: false,
                        kind: Spring(
                            SpringParams {
                                damping_ratio: 0.6,
                                stiffness: 500,
                                epsilon: 0.01,
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
            gestures: Gestures {
                dnd_edge_view_scroll: DndEdgeViewScroll {
                    trigger_width: FloatOrInt(
                        10.0,
                    ),
                    delay_ms: 100,
                    max_speed: FloatOrInt(
                        50.0,
                    ),
                },
                dnd_edge_workspace_switch: DndEdgeWorkspaceSwitch {
                    trigger_height: FloatOrInt(
                        50.0,
                    ),
                    delay_ms: 100,
                    max_speed: FloatOrInt(
                        1500.0,
                    ),
                },
                hot_corners: HotCorners {
                    off: false,
                },
            },
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
            environment: Environment(
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
            binds: Binds(
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
                    Bind {
                        key: Key {
                            trigger: Keysym(
                                XK_s,
                            ),
                            modifiers: Modifiers(
                                ALT | SUPER,
                            ),
                        },
                        action: SpawnSh(
                            "pkill orca || exec orca",
                        ),
                        repeat: true,
                        cooldown: None,
                        allow_when_locked: true,
                        allow_inhibiting: true,
                        hotkey_overlay_title: None,
                    },
                ],
            ),
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
            debug: DebugConfig {
                preview_render: None,
                dbus_interfaces_in_non_session_instances: false,
                wait_for_frame_completion_before_queueing: false,
                enable_overlay_planes: false,
                disable_cursor_plane: false,
                disable_direct_scanout: false,
                keep_max_bpc_unchanged: false,
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
}
