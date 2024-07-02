//! Types for communicating with niri via IPC.
#![warn(missing_docs)]

use std::collections::HashMap;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

mod socket;
pub use socket::{Socket, SOCKET_PATH_ENV};

/// Request from client to niri.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Request {
    /// Request the version string for the running niri instance.
    Version,
    /// Request information about connected outputs.
    Outputs,
    /// Request information about the focused window.
    FocusedWindow,
    /// Perform an action.
    Action(Action),
    /// Change output configuration temporarily.
    ///
    /// The configuration is changed temporarily and not saved into the config file. If the output
    /// configuration subsequently changes in the config file, these temporary changes will be
    /// forgotten.
    Output {
        /// Output name.
        output: String,
        /// Configuration to apply.
        action: OutputAction,
    },
    /// Request information about workspaces.
    Workspaces,
    /// Request information about the focused output.
    FocusedOutput,
    /// Respond with an error (for testing error handling).
    ReturnError,
}

/// Reply from niri to client.
///
/// Every request gets one reply.
///
/// * If an error had occurred, it will be an `Reply::Err`.
/// * If the request does not need any particular response, it will be
///   `Reply::Ok(Response::Handled)`. Kind of like an `Ok(())`.
/// * Otherwise, it will be `Reply::Ok(response)` with one of the other [`Response`] variants.
pub type Reply = Result<Response, String>;

/// Successful response from niri to client.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Response {
    /// A request that does not need a response was handled successfully.
    Handled,
    /// The version string for the running niri instance.
    Version(String),
    /// Information about connected outputs.
    ///
    /// Map from connector name to output info.
    Outputs(HashMap<String, Output>),
    /// Information about the focused window.
    FocusedWindow(Option<Window>),
    /// Output configuration change result.
    OutputConfigChanged(OutputConfigChanged),
    /// Information about workspaces.
    Workspaces(Vec<Workspace>),
    /// Information about the focused output.
    FocusedOutput(Option<Output>),
}

/// Actions that niri can perform.
// Variants in this enum should match the spelling of the ones in niri-config. Most, but not all,
// variants from niri-config should be present here.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
#[cfg_attr(feature = "clap", command(subcommand_value_name = "ACTION"))]
#[cfg_attr(feature = "clap", command(subcommand_help_heading = "Actions"))]
pub enum Action {
    /// Exit niri.
    Quit {
        /// Skip the "Press Enter to confirm" prompt.
        #[cfg_attr(feature = "clap", arg(short, long))]
        skip_confirmation: bool,
    },
    /// Power off all monitors via DPMS.
    PowerOffMonitors,
    /// Spawn a command.
    Spawn {
        /// Command to spawn.
        #[cfg_attr(feature = "clap", arg(last = true, required = true))]
        command: Vec<String>,
    },
    /// Do a screen transition.
    DoScreenTransition {
        /// Delay in milliseconds for the screen to freeze before starting the transition.
        #[cfg_attr(feature = "clap", arg(short, long))]
        delay_ms: Option<u16>,
    },
    /// Open the screenshot UI.
    Screenshot,
    /// Screenshot the focused screen.
    ScreenshotScreen,
    /// Screenshot the focused window.
    ScreenshotWindow,
    /// Close the focused window.
    CloseWindow,
    /// Toggle fullscreen on the focused window.
    FullscreenWindow,
    /// Focus the column to the left.
    FocusColumnLeft,
    /// Focus the column to the right.
    FocusColumnRight,
    /// Focus the first column.
    FocusColumnFirst,
    /// Focus the last column.
    FocusColumnLast,
    /// Focus the next column to the right, looping if at end.
    FocusColumnRightOrFirst,
    /// Focus the next column to the left, looping if at start.
    FocusColumnLeftOrLast,
    /// Focus the column or the monitor above.
    FocusWindowOrMonitorUp,
    /// Focus the column or the monitor below.
    FocusWindowOrMonitorDown,
    /// Focus the column or the monitor to the left.
    FocusColumnOrMonitorLeft,
    /// Focus the column or the monitor to the right.
    FocusColumnOrMonitorRight,
    /// Focus the window below.
    FocusWindowDown,
    /// Focus the window above.
    FocusWindowUp,
    /// Focus the window below or the column to the left.
    FocusWindowDownOrColumnLeft,
    /// Focus the window below or the column to the right.
    FocusWindowDownOrColumnRight,
    /// Focus the window above or the column to the left.
    FocusWindowUpOrColumnLeft,
    /// Focus the window above or the column to the right.
    FocusWindowUpOrColumnRight,
    /// Focus the window or the workspace above.
    FocusWindowOrWorkspaceDown,
    /// Focus the window or the workspace above.
    FocusWindowOrWorkspaceUp,
    /// Move the focused column to the left.
    MoveColumnLeft,
    /// Move the focused column to the right.
    MoveColumnRight,
    /// Move the focused column to the start of the workspace.
    MoveColumnToFirst,
    /// Move the focused column to the end of the workspace.
    MoveColumnToLast,
    /// Move the focused window down in a column.
    MoveWindowDown,
    /// Move the focused window up in a column.
    MoveWindowUp,
    /// Move the focused window down in a column or to the workspace below.
    MoveWindowDownOrToWorkspaceDown,
    /// Move the focused window up in a column or to the workspace above.
    MoveWindowUpOrToWorkspaceUp,
    /// Consume or expel the focused window left.
    ConsumeOrExpelWindowLeft,
    /// Consume or expel the focused window right.
    ConsumeOrExpelWindowRight,
    /// Consume the window to the right into the focused column.
    ConsumeWindowIntoColumn,
    /// Expel the focused window from the column.
    ExpelWindowFromColumn,
    /// Center the focused column on the screen.
    CenterColumn,
    /// Focus the workspace below.
    FocusWorkspaceDown,
    /// Focus the workspace above.
    FocusWorkspaceUp,
    /// Focus a workspace by reference (index or name).
    FocusWorkspace {
        /// Reference (index or name) of the workspace to focus.
        #[cfg_attr(feature = "clap", arg())]
        reference: WorkspaceReferenceArg,
    },
    /// Focus the previous workspace.
    FocusWorkspacePrevious,
    /// Move the focused window to the workspace below.
    MoveWindowToWorkspaceDown,
    /// Move the focused window to the workspace above.
    MoveWindowToWorkspaceUp,
    /// Move the focused window to a workspace by reference (index or name).
    MoveWindowToWorkspace {
        /// Reference (index or name) of the workspace to move the window to.
        #[cfg_attr(feature = "clap", arg())]
        reference: WorkspaceReferenceArg,
    },
    /// Move the focused column to the workspace below.
    MoveColumnToWorkspaceDown,
    /// Move the focused column to the workspace above.
    MoveColumnToWorkspaceUp,
    /// Move the focused column to a workspace by reference (index or name).
    MoveColumnToWorkspace {
        /// Reference (index or name) of the workspace to move the column to.
        #[cfg_attr(feature = "clap", arg())]
        reference: WorkspaceReferenceArg,
    },
    /// Move the focused workspace down.
    MoveWorkspaceDown,
    /// Move the focused workspace up.
    MoveWorkspaceUp,
    /// Focus the monitor to the left.
    FocusMonitorLeft,
    /// Focus the monitor to the right.
    FocusMonitorRight,
    /// Focus the monitor below.
    FocusMonitorDown,
    /// Focus the monitor above.
    FocusMonitorUp,
    /// Move the focused window to the monitor to the left.
    MoveWindowToMonitorLeft,
    /// Move the focused window to the monitor to the right.
    MoveWindowToMonitorRight,
    /// Move the focused window to the monitor below.
    MoveWindowToMonitorDown,
    /// Move the focused window to the monitor above.
    MoveWindowToMonitorUp,
    /// Move the focused column to the monitor to the left.
    MoveColumnToMonitorLeft,
    /// Move the focused column to the monitor to the right.
    MoveColumnToMonitorRight,
    /// Move the focused column to the monitor below.
    MoveColumnToMonitorDown,
    /// Move the focused column to the monitor above.
    MoveColumnToMonitorUp,
    /// Change the height of the focused window.
    SetWindowHeight {
        /// How to change the height.
        #[cfg_attr(feature = "clap", arg())]
        change: SizeChange,
    },
    /// Reset the height of the focused window back to automatic.
    ResetWindowHeight,
    /// Switch between preset column widths.
    SwitchPresetColumnWidth,
    /// Toggle the maximized state of the focused column.
    MaximizeColumn,
    /// Change the width of the focused column.
    SetColumnWidth {
        /// How to change the width.
        #[cfg_attr(feature = "clap", arg())]
        change: SizeChange,
    },
    /// Switch between keyboard layouts.
    SwitchLayout {
        /// Layout to switch to.
        #[cfg_attr(feature = "clap", arg())]
        layout: LayoutSwitchTarget,
    },
    /// Show the hotkey overlay.
    ShowHotkeyOverlay,
    /// Move the focused workspace to the monitor to the left.
    MoveWorkspaceToMonitorLeft,
    /// Move the focused workspace to the monitor to the right.
    MoveWorkspaceToMonitorRight,
    /// Move the focused workspace to the monitor below.
    MoveWorkspaceToMonitorDown,
    /// Move the focused workspace to the monitor above.
    MoveWorkspaceToMonitorUp,
    /// Toggle a debug tint on windows.
    ToggleDebugTint,
    /// Toggle visualization of render element opaque regions.
    DebugToggleOpaqueRegions,
    /// Toggle visualization of output damage.
    DebugToggleDamage,
}

/// Change in window or column size.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum SizeChange {
    /// Set the size in logical pixels.
    SetFixed(i32),
    /// Set the size as a proportion of the working area.
    SetProportion(f64),
    /// Add or subtract to the current size in logical pixels.
    AdjustFixed(i32),
    /// Add or subtract to the current size as a proportion of the working area.
    AdjustProportion(f64),
}

/// Workspace reference (index or name) to operate on.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceReferenceArg {
    /// Index of the workspace.
    Index(u8),
    /// Name of the workspace.
    Name(String),
}

/// Layout to switch to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSwitchTarget {
    /// The next configured layout.
    Next,
    /// The previous configured layout.
    Prev,
}

/// Output actions that niri can perform.
// Variants in this enum should match the spelling of the ones in niri-config. Most thigs from
// niri-config should be present here.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
#[cfg_attr(feature = "clap", command(subcommand_value_name = "ACTION"))]
#[cfg_attr(feature = "clap", command(subcommand_help_heading = "Actions"))]
pub enum OutputAction {
    /// Turn off the output.
    Off,
    /// Turn on the output.
    On,
    /// Set the output mode.
    Mode {
        /// Mode to set, or "auto" for automatic selection.
        ///
        /// Run `niri msg outputs` to see the available modes.
        #[cfg_attr(feature = "clap", arg())]
        mode: ModeToSet,
    },
    /// Set the output scale.
    Scale {
        /// Scale factor to set, or "auto" for automatic selection.
        #[cfg_attr(feature = "clap", arg())]
        scale: ScaleToSet,
    },
    /// Set the output transform.
    Transform {
        /// Transform to set, counter-clockwise.
        #[cfg_attr(feature = "clap", arg())]
        transform: Transform,
    },
    /// Set the output position.
    Position {
        /// Position to set, or "auto" for automatic selection.
        #[cfg_attr(feature = "clap", command(subcommand))]
        position: PositionToSet,
    },
    /// Toggle variable refresh rate.
    Vrr {
        /// Whether to enable variable refresh rate.
        #[cfg_attr(
            feature = "clap",
            arg(
                value_name = "ON|OFF",
                action = clap::ArgAction::Set,
                value_parser = clap::builder::BoolishValueParser::new(),
            ),
        )]
        enable: bool,
    },
}

/// Output mode to set.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ModeToSet {
    /// Niri will pick the mode automatically.
    Automatic,
    /// Specific mode.
    Specific(ConfiguredMode),
}

/// Output mode as set in the config file.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct ConfiguredMode {
    /// Width in physical pixels.
    pub width: u16,
    /// Height in physical pixels.
    pub height: u16,
    /// Refresh rate.
    pub refresh: Option<f64>,
}

/// Output scale to set.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ScaleToSet {
    /// Niri will pick the scale automatically.
    Automatic,
    /// Specific scale.
    Specific(f64),
}

/// Output position to set.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Subcommand))]
#[cfg_attr(feature = "clap", command(subcommand_value_name = "POSITION"))]
#[cfg_attr(feature = "clap", command(subcommand_help_heading = "Position Values"))]
pub enum PositionToSet {
    /// Position the output automatically.
    #[cfg_attr(feature = "clap", command(name = "auto"))]
    Automatic,
    /// Set a specific position.
    #[cfg_attr(feature = "clap", command(name = "set"))]
    Specific(ConfiguredPosition),
}

/// Output position as set in the config file.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct ConfiguredPosition {
    /// Logical X position.
    pub x: i32,
    /// Logical Y position.
    pub y: i32,
}

/// Connected output.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Output {
    /// Name of the output.
    pub name: String,
    /// Textual description of the manufacturer.
    pub make: String,
    /// Textual description of the model.
    pub model: String,
    /// Physical width and height of the output in millimeters, if known.
    pub physical_size: Option<(u32, u32)>,
    /// Available modes for the output.
    pub modes: Vec<Mode>,
    /// Index of the current mode in [`Self::modes`].
    ///
    /// `None` if the output is disabled.
    pub current_mode: Option<usize>,
    /// Whether the output supports variable refresh rate.
    pub vrr_supported: bool,
    /// Whether variable refresh rate is enabled on the output.
    pub vrr_enabled: bool,
    /// Logical output information.
    ///
    /// `None` if the output is not mapped to any logical output (for example, if it is disabled).
    pub logical: Option<LogicalOutput>,
}

/// Output mode.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct Mode {
    /// Width in physical pixels.
    pub width: u16,
    /// Height in physical pixels.
    pub height: u16,
    /// Refresh rate in millihertz.
    pub refresh_rate: u32,
    /// Whether this mode is preferred by the monitor.
    pub is_preferred: bool,
}

/// Logical output in the compositor's coordinate space.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct LogicalOutput {
    /// Logical X position.
    pub x: i32,
    /// Logical Y position.
    pub y: i32,
    /// Width in logical pixels.
    pub width: u32,
    /// Height in logical pixels.
    pub height: u32,
    /// Scale factor.
    pub scale: f64,
    /// Transform.
    pub transform: Transform,
}

/// Output transform, which goes counter-clockwise.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Transform {
    /// Untransformed.
    Normal,
    /// Rotated by 90°.
    #[serde(rename = "90")]
    _90,
    /// Rotated by 180°.
    #[serde(rename = "180")]
    _180,
    /// Rotated by 270°.
    #[serde(rename = "270")]
    _270,
    /// Flipped horizontally.
    Flipped,
    /// Rotated by 90° and flipped horizontally.
    #[cfg_attr(feature = "clap", value(name("flipped-90")))]
    Flipped90,
    /// Flipped vertically.
    #[cfg_attr(feature = "clap", value(name("flipped-180")))]
    Flipped180,
    /// Rotated by 270° and flipped horizontally.
    #[cfg_attr(feature = "clap", value(name("flipped-270")))]
    Flipped270,
}

/// Toplevel window.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Window {
    /// Title, if set.
    pub title: Option<String>,
    /// Application ID, if set.
    pub app_id: Option<String>,
}

/// Output configuration change result.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputConfigChanged {
    /// The target output was connected and the change was applied.
    Applied,
    /// The target output was not found, the change will be applied when it is connected.
    OutputWasMissing,
}

/// A workspace.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// Index of the workspace on its monitor.
    ///
    /// This is the same index you can use for requests like `niri msg action focus-workspace`.
    pub idx: u8,
    /// Optional name of the workspace.
    pub name: Option<String>,
    /// Name of the output that the workspace is on.
    ///
    /// Can be `None` if no outputs are currently connected.
    pub output: Option<String>,
    /// Whether the workspace is currently active on its output.
    pub is_active: bool,
}

impl FromStr for WorkspaceReferenceArg {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let reference = if let Ok(index) = s.parse::<i32>() {
            if let Ok(idx) = u8::try_from(index) {
                Self::Index(idx)
            } else {
                return Err("workspace indexes must be between 0 and 255");
            }
        } else {
            Self::Name(s.to_string())
        };

        Ok(reference)
    }
}

impl FromStr for SizeChange {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('%') {
            Some((value, empty)) => {
                if !empty.is_empty() {
                    return Err("trailing characters after '%' are not allowed");
                }

                match value.bytes().next() {
                    Some(b'-' | b'+') => {
                        let value = value.parse().map_err(|_| "error parsing value")?;
                        Ok(Self::AdjustProportion(value))
                    }
                    Some(_) => {
                        let value = value.parse().map_err(|_| "error parsing value")?;
                        Ok(Self::SetProportion(value))
                    }
                    None => Err("value is missing"),
                }
            }
            None => {
                let value = s;
                match value.bytes().next() {
                    Some(b'-' | b'+') => {
                        let value = value.parse().map_err(|_| "error parsing value")?;
                        Ok(Self::AdjustFixed(value))
                    }
                    Some(_) => {
                        let value = value.parse().map_err(|_| "error parsing value")?;
                        Ok(Self::SetFixed(value))
                    }
                    None => Err("value is missing"),
                }
            }
        }
    }
}

impl FromStr for LayoutSwitchTarget {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "next" => Ok(Self::Next),
            "prev" => Ok(Self::Prev),
            _ => Err(r#"invalid layout action, can be "next" or "prev""#),
        }
    }
}

impl FromStr for Transform {
    type Err = &'static str;

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
            _ => Err(concat!(
                r#"invalid transform, can be "90", "180", "270", "#,
                r#""flipped", "flipped-90", "flipped-180" or "flipped-270""#
            )),
        }
    }
}

impl FromStr for ModeToSet {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            return Ok(Self::Automatic);
        }

        let mode = s.parse()?;
        Ok(Self::Specific(mode))
    }
}

impl FromStr for ConfiguredMode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((width, rest)) = s.split_once('x') else {
            return Err("no 'x' separator found");
        };

        let (height, refresh) = match rest.split_once('@') {
            Some((height, refresh)) => (height, Some(refresh)),
            None => (rest, None),
        };

        let width = width.parse().map_err(|_| "error parsing width")?;
        let height = height.parse().map_err(|_| "error parsing height")?;
        let refresh = refresh
            .map(str::parse)
            .transpose()
            .map_err(|_| "error parsing refresh rate")?;

        Ok(Self {
            width,
            height,
            refresh,
        })
    }
}

impl FromStr for ScaleToSet {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            return Ok(Self::Automatic);
        }

        let scale = s.parse().map_err(|_| "error parsing scale")?;
        Ok(Self::Specific(scale))
    }
}
