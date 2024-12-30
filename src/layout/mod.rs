//! Window layout logic.
//!
//! Niri implements scrollable tiling with workspaces. There's one primary output, and potentially
//! multiple other outputs.
//!
//! Our layout has the following invariants:
//!
//! 1. Disconnecting and reconnecting the same output must not change the layout.
//!    * This includes both secondary outputs and the primary output.
//! 2. Connecting an output must not change the layout for any workspaces that were never on that
//!    output.
//!
//! Therefore, we implement the following logic: every workspace keeps track of which output it
//! originated on. When an output disconnects, its workspace (or workspaces, in case of the primary
//! output disconnecting) are appended to the (potentially new) primary output, but remember their
//! original output. Then, if the original output connects again, all workspaces originally from
//! there move back to that output.
//!
//! In order to avoid surprising behavior, if the user creates or moves any new windows onto a
//! workspace, it forgets its original output, and its current output becomes its original output.
//! Imagine a scenario: the user works with a laptop and a monitor at home, then takes their laptop
//! with them, disconnecting the monitor, and keeps working as normal, using the second monitor's
//! workspace just like any other. Then they come back, reconnect the second monitor, and now we
//! don't want an unassuming workspace to end up on it.
//!
//! ## Workspaces-only-on-primary considerations
//!
//! If this logic results in more than one workspace present on a secondary output, then as a
//! compromise we only keep the first workspace there, and move the rest to the primary output,
//! making the primary output their original output.

use std::cmp::min;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;
use std::time::Duration;

use monitor::MonitorAddWindowTarget;
use niri_config::{
    CenterFocusedColumn, Config, CornerRadius, FloatOrInt, PresetSize, Struts,
    Workspace as WorkspaceConfig,
};
use niri_ipc::{PositionChange, SizeChange};
use scrolling::{Column, ColumnWidth, InsertHint, InsertPosition};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::output::{self, Output};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Scale, Serial, Size, Transform};
use tile::{Tile, TileRenderElement};
use workspace::{WorkspaceAddWindowTarget, WorkspaceId};

pub use self::monitor::MonitorRenderElement;
use self::monitor::{Monitor, WorkspaceSwitch};
use self::workspace::{OutputId, Workspace};
use crate::animation::Clock;
use crate::niri_render_elements;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::snapshot::RenderSnapshot;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::texture::TextureBuffer;
use crate::render_helpers::{BakedBuffer, RenderTarget, SplitElements};
use crate::rubber_band::RubberBand;
use crate::utils::transaction::{Transaction, TransactionBlocker};
use crate::utils::{
    ensure_min_max_size_maybe_zero, output_matches_name, output_size,
    round_logical_in_physical_max1, ResizeEdge,
};
use crate::window::ResolvedWindowRules;

pub mod closing_window;
pub mod floating;
pub mod focus_ring;
pub mod insert_hint_element;
pub mod monitor;
pub mod opening_window;
pub mod scrolling;
pub mod tile;
pub mod workspace;

/// Size changes up to this many pixels don't animate.
pub const RESIZE_ANIMATION_THRESHOLD: f64 = 10.;

/// Pointer needs to move this far to pull a window from the layout.
const INTERACTIVE_MOVE_START_THRESHOLD: f64 = 256. * 256.;

/// Size-relative units.
pub struct SizeFrac;

niri_render_elements! {
    LayoutElementRenderElement<R> => {
        Wayland = WaylandSurfaceRenderElement<R>,
        SolidColor = SolidColorRenderElement,
    }
}

pub type LayoutElementRenderSnapshot =
    RenderSnapshot<BakedBuffer<TextureBuffer<GlesTexture>>, BakedBuffer<SolidColorBuffer>>;

pub trait LayoutElement {
    /// Type that can be used as a unique ID of this element.
    type Id: PartialEq + std::fmt::Debug + Clone;

    /// Unique ID of this element.
    fn id(&self) -> &Self::Id;

    /// Visual size of the element.
    ///
    /// This is what the user would consider the size, i.e. excluding CSD shadows and whatnot.
    /// Corresponds to the Wayland window geometry size.
    fn size(&self) -> Size<i32, Logical>;

    /// Returns the location of the element's buffer relative to the element's visual geometry.
    ///
    /// I.e. if the element has CSD shadows, its buffer location will have negative coordinates.
    fn buf_loc(&self) -> Point<i32, Logical>;

    /// Checks whether a point is in the element's input region.
    ///
    /// The point is relative to the element's visual geometry.
    fn is_in_input_region(&self, point: Point<f64, Logical>) -> bool;

    /// Renders the element at the given visual location.
    ///
    /// The element should be rendered in such a way that its visual geometry ends up at the given
    /// location.
    fn render<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> SplitElements<LayoutElementRenderElement<R>>;

    /// Renders the non-popup parts of the element.
    fn render_normal<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> Vec<LayoutElementRenderElement<R>> {
        self.render(renderer, location, scale, alpha, target).normal
    }

    /// Renders the popups of the element.
    fn render_popups<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        location: Point<f64, Logical>,
        scale: Scale<f64>,
        alpha: f32,
        target: RenderTarget,
    ) -> Vec<LayoutElementRenderElement<R>> {
        self.render(renderer, location, scale, alpha, target).popups
    }

    /// Requests the element to change its size.
    ///
    /// The size request is stored and will be continuously sent to the element on any further
    /// state changes.
    fn request_size(
        &mut self,
        size: Size<i32, Logical>,
        animate: bool,
        transaction: Option<Transaction>,
    );

    /// Requests the element to change size once, clearing the request afterwards.
    fn request_size_once(&mut self, size: Size<i32, Logical>, animate: bool) {
        self.request_size(size, animate, None);
    }

    fn request_fullscreen(&mut self, size: Size<i32, Logical>);

    fn min_size(&self) -> Size<i32, Logical>;
    fn max_size(&self) -> Size<i32, Logical>;
    fn is_wl_surface(&self, wl_surface: &WlSurface) -> bool;
    fn has_ssd(&self) -> bool;
    fn set_preferred_scale_transform(&self, scale: output::Scale, transform: Transform);
    fn output_enter(&self, output: &Output);
    fn output_leave(&self, output: &Output);
    fn set_offscreen_element_id(&self, id: Option<Id>);
    fn set_activated(&mut self, active: bool);
    fn set_active_in_column(&mut self, active: bool);
    fn set_floating(&mut self, floating: bool);
    fn set_bounds(&self, bounds: Size<i32, Logical>);

    fn configure_intent(&self) -> ConfigureIntent;
    fn send_pending_configure(&mut self);

    /// Whether the element is currently fullscreen.
    ///
    /// This will *not* switch immediately after a [`LayoutElement::request_fullscreen()`] call.
    fn is_fullscreen(&self) -> bool;

    /// Whether we're requesting the element to be fullscreen.
    ///
    /// This *will* switch immediately after a [`LayoutElement::request_fullscreen()`] call.
    fn is_pending_fullscreen(&self) -> bool;

    /// Size previously requested through [`LayoutElement::request_size()`].
    fn requested_size(&self) -> Option<Size<i32, Logical>>;

    /// Non-fullscreen size that we expect this window has or will shortly have.
    ///
    /// This can be different from [`requested_size()`](LayoutElement::requested_size()). For
    /// example, for floating windows this will generally return the current window size, rather
    /// than the last size that we requested, since we want floating windows to be able to change
    /// size freely. But not always: if we just requested a floating window to resize and it hasn't
    /// responded to it yet, this will return the newly requested size.
    ///
    /// This function should never return a 0 size component. `None` means there's no known
    /// expected size (for example, the window is fullscreen).
    ///
    /// The default impl is for testing only, it will not preserve the window's own size changes.
    fn expected_size(&self) -> Option<Size<i32, Logical>> {
        if self.is_fullscreen() {
            return None;
        }

        let mut requested = self.requested_size().unwrap_or_default();
        let current = self.size();
        if requested.w == 0 {
            requested.w = current.w;
        }
        if requested.h == 0 {
            requested.h = current.h;
        }
        Some(requested)
    }

    fn is_child_of(&self, parent: &Self) -> bool;

    fn rules(&self) -> &ResolvedWindowRules;

    /// Runs periodic clean-up tasks.
    fn refresh(&self);

    fn animation_snapshot(&self) -> Option<&LayoutElementRenderSnapshot>;
    fn take_animation_snapshot(&mut self) -> Option<LayoutElementRenderSnapshot>;

    fn set_interactive_resize(&mut self, data: Option<InteractiveResizeData>);
    fn cancel_interactive_resize(&mut self);
    fn interactive_resize_data(&self) -> Option<InteractiveResizeData>;

    fn on_commit(&mut self, serial: Serial);
}

#[derive(Debug)]
pub struct Layout<W: LayoutElement> {
    /// Monitors and workspaes in the layout.
    monitor_set: MonitorSet<W>,
    /// Whether the layout should draw as active.
    ///
    /// This normally indicates that the layout has keyboard focus, but not always. E.g. when the
    /// screenshot UI is open, it keeps the layout drawing as active.
    is_active: bool,
    /// Map from monitor name to id of its last active workspace.
    ///
    /// This data is stored upon monitor removal and is used to restore the active workspace when
    /// the monitor is reconnected.
    ///
    /// The workspace id does not necessarily point to a valid workspace. If it doesn't, then it is
    /// simply ignored.
    last_active_workspace_id: HashMap<String, WorkspaceId>,
    /// Ongoing interactive move.
    interactive_move: Option<InteractiveMoveState<W>>,
    /// Clock for driving animations.
    clock: Clock,
    /// Time that we last updated render elements for.
    update_render_elements_time: Duration,
    /// Configurable properties of the layout.
    options: Rc<Options>,
}

#[derive(Debug)]
enum MonitorSet<W: LayoutElement> {
    /// At least one output is connected.
    Normal {
        /// Connected monitors.
        monitors: Vec<Monitor<W>>,
        /// Index of the primary monitor.
        primary_idx: usize,
        /// Index of the active monitor.
        active_monitor_idx: usize,
    },
    /// No outputs are connected, and these are the workspaces.
    NoOutputs {
        /// The workspaces.
        workspaces: Vec<Workspace<W>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Padding around windows in logical pixels.
    pub gaps: f64,
    /// Extra padding around the working area in logical pixels.
    pub struts: Struts,
    pub focus_ring: niri_config::FocusRing,
    pub border: niri_config::Border,
    pub insert_hint: niri_config::InsertHint,
    pub center_focused_column: CenterFocusedColumn,
    pub always_center_single_column: bool,
    pub empty_workspace_above_first: bool,
    /// Column widths that `toggle_width()` switches between.
    pub preset_column_widths: Vec<ColumnWidth>,
    /// Initial width for new columns.
    pub default_column_width: Option<ColumnWidth>,
    /// Window height that `toggle_window_height()` switches between.
    pub preset_window_heights: Vec<PresetSize>,
    pub animations: niri_config::Animations,
    // Debug flags.
    pub disable_resize_throttling: bool,
    pub disable_transactions: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            gaps: 16.,
            struts: Default::default(),
            focus_ring: Default::default(),
            border: Default::default(),
            insert_hint: Default::default(),
            center_focused_column: Default::default(),
            always_center_single_column: false,
            empty_workspace_above_first: false,
            preset_column_widths: vec![
                ColumnWidth::Proportion(1. / 3.),
                ColumnWidth::Proportion(0.5),
                ColumnWidth::Proportion(2. / 3.),
            ],
            default_column_width: None,
            animations: Default::default(),
            disable_resize_throttling: false,
            disable_transactions: false,
            preset_window_heights: vec![
                PresetSize::Proportion(1. / 3.),
                PresetSize::Proportion(0.5),
                PresetSize::Proportion(2. / 3.),
            ],
        }
    }
}

#[derive(Debug)]
enum InteractiveMoveState<W: LayoutElement> {
    /// Initial rubberbanding; the window remains in the layout.
    Starting {
        /// The window we're moving.
        window_id: W::Id,
        /// Current pointer delta from the starting location.
        pointer_delta: Point<f64, Logical>,
        /// Pointer location within the visual window geometry as ratio from geometry size.
        ///
        /// This helps the pointer remain inside the window as it resizes.
        pointer_ratio_within_window: (f64, f64),
    },
    /// Moving; the window is no longer in the layout.
    Moving(InteractiveMoveData<W>),
}

#[derive(Debug)]
struct InteractiveMoveData<W: LayoutElement> {
    /// The window being moved.
    pub(self) tile: Tile<W>,
    /// Output where the window is currently located/rendered.
    pub(self) output: Output,
    /// Current pointer position within output.
    pub(self) pointer_pos_within_output: Point<f64, Logical>,
    /// Window column width.
    pub(self) width: ColumnWidth,
    /// Whether the window column was full-width.
    pub(self) is_full_width: bool,
    /// Whether the window targets the floating layout.
    pub(self) is_floating: bool,
    /// Pointer location within the visual window geometry as ratio from geometry size.
    ///
    /// This helps the pointer remain inside the window as it resizes.
    pub(self) pointer_ratio_within_window: (f64, f64),
}

#[derive(Debug, Clone, Copy)]
pub struct InteractiveResizeData {
    pub(self) edges: ResizeEdge,
}

#[derive(Debug, Clone, Copy)]
pub enum ConfigureIntent {
    /// A configure is not needed (no changes to server pending state).
    NotNeeded,
    /// A configure is throttled (due to resizing too fast for example).
    Throttled,
    /// Can send the configure if it isn't throttled externally (only size changed).
    CanSend,
    /// Should send the configure regardless of external throttling (something other than size
    /// changed).
    ShouldSend,
}

/// Tile that was just removed from the layout.
pub struct RemovedTile<W: LayoutElement> {
    tile: Tile<W>,
    /// Width of the column the tile was in.
    width: ColumnWidth,
    /// Whether the column the tile was in was full-width.
    is_full_width: bool,
    /// Whether the tile was floating.
    is_floating: bool,
}

/// Whether to activate a newly added window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActivateWindow {
    /// Activate unconditionally.
    Yes,
    /// Activate based on heuristics.
    #[default]
    Smart,
    /// Do not activate.
    No,
}

/// Where to put a newly added window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AddWindowTarget<'a, W: LayoutElement> {
    /// No particular preference.
    #[default]
    Auto,
    /// On this output.
    Output(&'a Output),
    /// On this workspace.
    Workspace(WorkspaceId),
    /// Next to this existing window.
    NextTo(&'a W::Id),
}

impl<W: LayoutElement> InteractiveMoveState<W> {
    fn moving(&self) -> Option<&InteractiveMoveData<W>> {
        match self {
            InteractiveMoveState::Moving(move_) => Some(move_),
            _ => None,
        }
    }
}

impl<W: LayoutElement> InteractiveMoveData<W> {
    fn tile_render_location(&self) -> Point<f64, Logical> {
        let scale = Scale::from(self.output.current_scale().fractional_scale());
        let window_size = self.tile.window_size();
        let pointer_offset_within_window = Point::from((
            window_size.w * self.pointer_ratio_within_window.0,
            window_size.h * self.pointer_ratio_within_window.1,
        ));
        let pos =
            self.pointer_pos_within_output - pointer_offset_within_window - self.tile.window_loc()
                + self.tile.render_offset();
        // Round to physical pixels.
        pos.to_physical_precise_round(scale).to_logical(scale)
    }
}

impl ActivateWindow {
    pub fn map_smart(self, f: impl FnOnce() -> bool) -> bool {
        match self {
            ActivateWindow::Yes => true,
            ActivateWindow::Smart => f(),
            ActivateWindow::No => false,
        }
    }
}

impl Options {
    fn from_config(config: &Config) -> Self {
        let layout = &config.layout;

        let preset_column_widths = if layout.preset_column_widths.is_empty() {
            Options::default().preset_column_widths
        } else {
            layout
                .preset_column_widths
                .iter()
                .copied()
                .map(ColumnWidth::from)
                .collect()
        };
        let preset_window_heights = if layout.preset_window_heights.is_empty() {
            Options::default().preset_window_heights
        } else {
            layout.preset_window_heights.clone()
        };

        // Missing default_column_width maps to Some(ColumnWidth::Proportion(0.5)),
        // while present, but empty, maps to None.
        let default_column_width = layout
            .default_column_width
            .as_ref()
            .map(|w| w.0.map(ColumnWidth::from))
            .unwrap_or(Some(ColumnWidth::Proportion(0.5)));

        Self {
            gaps: layout.gaps.0,
            struts: layout.struts,
            focus_ring: layout.focus_ring,
            border: layout.border,
            insert_hint: layout.insert_hint,
            center_focused_column: layout.center_focused_column,
            always_center_single_column: layout.always_center_single_column,
            empty_workspace_above_first: layout.empty_workspace_above_first,
            preset_column_widths,
            default_column_width,
            animations: config.animations.clone(),
            disable_resize_throttling: config.debug.disable_resize_throttling,
            disable_transactions: config.debug.disable_transactions,
            preset_window_heights,
        }
    }

    fn adjusted_for_scale(mut self, scale: f64) -> Self {
        let round = |logical: f64| round_logical_in_physical_max1(scale, logical);

        self.gaps = round(self.gaps);
        self.focus_ring.width = FloatOrInt(round(self.focus_ring.width.0));
        self.border.width = FloatOrInt(round(self.border.width.0));

        self
    }
}

impl<W: LayoutElement> Layout<W> {
    pub fn new(clock: Clock, config: &Config) -> Self {
        Self::with_options_and_workspaces(clock, config, Options::from_config(config))
    }

    pub fn with_options(clock: Clock, options: Options) -> Self {
        Self {
            monitor_set: MonitorSet::NoOutputs { workspaces: vec![] },
            is_active: true,
            last_active_workspace_id: HashMap::new(),
            interactive_move: None,
            clock,
            update_render_elements_time: Duration::ZERO,
            options: Rc::new(options),
        }
    }

    fn with_options_and_workspaces(clock: Clock, config: &Config, options: Options) -> Self {
        let opts = Rc::new(options);

        let workspaces = config
            .workspaces
            .iter()
            .map(|ws| {
                Workspace::new_with_config_no_outputs(Some(ws.clone()), clock.clone(), opts.clone())
            })
            .collect();

        Self {
            monitor_set: MonitorSet::NoOutputs { workspaces },
            is_active: true,
            last_active_workspace_id: HashMap::new(),
            interactive_move: None,
            clock,
            update_render_elements_time: Duration::ZERO,
            options: opts,
        }
    }

    pub fn add_output(&mut self, output: Output) {
        self.monitor_set = match mem::take(&mut self.monitor_set) {
            MonitorSet::Normal {
                mut monitors,
                primary_idx,
                active_monitor_idx,
            } => {
                let primary = &mut monitors[primary_idx];

                let ws_id_to_activate = self.last_active_workspace_id.remove(&output.name());
                let mut active_workspace_idx = None;

                let mut stopped_primary_ws_switch = false;

                let mut workspaces = vec![];
                for i in (0..primary.workspaces.len()).rev() {
                    if primary.workspaces[i].original_output.matches(&output) {
                        let ws = primary.workspaces.remove(i);

                        // FIXME: this can be coded in a way that the workspace switch won't be
                        // affected if the removed workspace is invisible. But this is good enough
                        // for now.
                        if primary.workspace_switch.is_some() {
                            primary.workspace_switch = None;
                            stopped_primary_ws_switch = true;
                        }

                        // The user could've closed a window while remaining on this workspace, on
                        // another monitor. However, we will add an empty workspace in the end
                        // instead.
                        if ws.has_windows_or_name() {
                            if Some(ws.id()) == ws_id_to_activate {
                                active_workspace_idx = Some(workspaces.len());
                            }

                            workspaces.push(ws);
                        }

                        if i <= primary.active_workspace_idx
                            // Generally when moving the currently active workspace, we want to
                            // fall back to the workspace above, so as not to end up on the last
                            // empty workspace. However, with empty workspace above first, when
                            // moving the workspace at index 1 (first non-empty), we want to stay
                            // at index 1, so as once again not to end up on an empty workspace.
                            //
                            // This comes into play at compositor startup when having named
                            // workspaces set up across multiple monitors. Without this check, the
                            // first monitor to connect can end up with the first empty workspace
                            // focused instead of the first named workspace.
                            && !(self.options.empty_workspace_above_first
                                && primary.active_workspace_idx == 1)
                        {
                            primary.active_workspace_idx =
                                primary.active_workspace_idx.saturating_sub(1);
                        }
                    }
                }

                // If we stopped a workspace switch, then we might need to clean up workspaces.
                // Also if empty_workspace_above_first is set and there are only 2 workspaces left,
                // both will be empty and one of them needs to be removed. clean_up_workspaces
                // takes care of this.

                if stopped_primary_ws_switch
                    || (primary.options.empty_workspace_above_first
                        && primary.workspaces.len() == 2)
                {
                    primary.clean_up_workspaces();
                }

                workspaces.reverse();

                if let Some(idx) = &mut active_workspace_idx {
                    *idx = workspaces.len() - *idx - 1;
                }
                let mut active_workspace_idx = active_workspace_idx.unwrap_or(0);

                // Make sure there's always an empty workspace.
                workspaces.push(Workspace::new(
                    output.clone(),
                    self.clock.clone(),
                    self.options.clone(),
                ));

                if self.options.empty_workspace_above_first && workspaces.len() > 1 {
                    workspaces.insert(
                        0,
                        Workspace::new(output.clone(), self.clock.clone(), self.options.clone()),
                    );
                    active_workspace_idx += 1;
                }

                for ws in &mut workspaces {
                    ws.set_output(Some(output.clone()));
                }

                let mut monitor =
                    Monitor::new(output, workspaces, self.clock.clone(), self.options.clone());
                monitor.active_workspace_idx = active_workspace_idx;
                monitors.push(monitor);

                MonitorSet::Normal {
                    monitors,
                    primary_idx,
                    active_monitor_idx,
                }
            }
            MonitorSet::NoOutputs { mut workspaces } => {
                // We know there are no empty workspaces there, so add one.
                workspaces.push(Workspace::new(
                    output.clone(),
                    self.clock.clone(),
                    self.options.clone(),
                ));

                let mut active_workspace_idx = 0;
                if self.options.empty_workspace_above_first && workspaces.len() > 1 {
                    workspaces.insert(
                        0,
                        Workspace::new(output.clone(), self.clock.clone(), self.options.clone()),
                    );
                    active_workspace_idx += 1;
                }

                let ws_id_to_activate = self.last_active_workspace_id.remove(&output.name());

                for (i, workspace) in workspaces.iter_mut().enumerate() {
                    workspace.set_output(Some(output.clone()));

                    if Some(workspace.id()) == ws_id_to_activate {
                        active_workspace_idx = i;
                    }
                }

                let mut monitor =
                    Monitor::new(output, workspaces, self.clock.clone(), self.options.clone());
                monitor.active_workspace_idx = active_workspace_idx;

                MonitorSet::Normal {
                    monitors: vec![monitor],
                    primary_idx: 0,
                    active_monitor_idx: 0,
                }
            }
        }
    }

    pub fn remove_output(&mut self, output: &Output) {
        self.monitor_set = match mem::take(&mut self.monitor_set) {
            MonitorSet::Normal {
                mut monitors,
                mut primary_idx,
                mut active_monitor_idx,
            } => {
                let idx = monitors
                    .iter()
                    .position(|mon| &mon.output == output)
                    .expect("trying to remove non-existing output");
                let monitor = monitors.remove(idx);

                self.last_active_workspace_id.insert(
                    monitor.output_name().clone(),
                    monitor.workspaces[monitor.active_workspace_idx].id(),
                );

                let mut workspaces = monitor.workspaces;

                for ws in &mut workspaces {
                    ws.set_output(None);
                }

                // Get rid of empty workspaces.
                workspaces.retain(|ws| ws.has_windows_or_name());

                if monitors.is_empty() {
                    // Removed the last monitor.
                    MonitorSet::NoOutputs { workspaces }
                } else {
                    if primary_idx >= idx {
                        // Update primary_idx to either still point at the same monitor, or at some
                        // other monitor if the primary has been removed.
                        primary_idx = primary_idx.saturating_sub(1);
                    }
                    if active_monitor_idx >= idx {
                        // Update active_monitor_idx to either still point at the same monitor, or
                        // at some other monitor if the active monitor has
                        // been removed.
                        active_monitor_idx = active_monitor_idx.saturating_sub(1);
                    }

                    let primary = &mut monitors[primary_idx];
                    for ws in &mut workspaces {
                        ws.set_output(Some(primary.output.clone()));
                    }

                    let mut stopped_primary_ws_switch = false;
                    if !workspaces.is_empty() && primary.workspace_switch.is_some() {
                        // FIXME: if we're adding workspaces to currently invisible positions
                        // (outside the workspace switch), we don't need to cancel it.
                        primary.workspace_switch = None;
                        stopped_primary_ws_switch = true;
                    }

                    let empty_was_focused =
                        primary.active_workspace_idx == primary.workspaces.len() - 1;

                    // Push the workspaces from the removed monitor in the end, right before the
                    // last, empty, workspace.
                    let empty = primary.workspaces.remove(primary.workspaces.len() - 1);
                    primary.workspaces.extend(workspaces);
                    primary.workspaces.push(empty);

                    // If empty_workspace_above_first is set and the first workspace is now no
                    // longer empty, add a new empty workspace on top.
                    if primary.options.empty_workspace_above_first
                        && primary.workspaces[0].has_windows_or_name()
                    {
                        primary.add_workspace_top();
                    }

                    // If the empty workspace was focused on the primary monitor, keep it focused.
                    if empty_was_focused {
                        primary.active_workspace_idx = primary.workspaces.len() - 1;
                    }

                    if stopped_primary_ws_switch {
                        primary.clean_up_workspaces();
                    }

                    MonitorSet::Normal {
                        monitors,
                        primary_idx,
                        active_monitor_idx,
                    }
                }
            }
            MonitorSet::NoOutputs { .. } => {
                panic!("tried to remove output when there were already none")
            }
        }
    }

    pub fn add_column_by_idx(
        &mut self,
        monitor_idx: usize,
        workspace_idx: usize,
        column: Column<W>,
        activate: bool,
    ) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            panic!()
        };

        monitors[monitor_idx].add_column(workspace_idx, column, activate);

        if activate {
            *active_monitor_idx = monitor_idx;
        }
    }

    /// Adds a new window to the layout.
    ///
    /// Returns an output that the window was added to, if there were any outputs.
    #[allow(clippy::too_many_arguments)]
    pub fn add_window(
        &mut self,
        window: W,
        target: AddWindowTarget<W>,
        width: Option<ColumnWidth>,
        height: Option<PresetSize>,
        is_full_width: bool,
        is_floating: bool,
        activate: ActivateWindow,
    ) -> Option<&Output> {
        let resolved_width = self.resolve_default_width(&window, width, is_floating);
        let resolved_height = height.map(|h| match h {
            PresetSize::Proportion(prop) => SizeChange::SetProportion(prop * 100.),
            PresetSize::Fixed(fixed) => SizeChange::SetFixed(fixed),
        });
        let id = window.id().clone();

        match &mut self.monitor_set {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                let (mon_idx, target) = match target {
                    AddWindowTarget::Auto => (*active_monitor_idx, MonitorAddWindowTarget::Auto),
                    AddWindowTarget::Output(output) => {
                        let mon_idx = monitors
                            .iter()
                            .position(|mon| mon.output == *output)
                            .unwrap();

                        (mon_idx, MonitorAddWindowTarget::Auto)
                    }
                    AddWindowTarget::Workspace(ws_id) => {
                        let mon_idx = monitors
                            .iter()
                            .position(|mon| mon.workspaces.iter().any(|ws| ws.id() == ws_id))
                            .unwrap();

                        (
                            mon_idx,
                            MonitorAddWindowTarget::Workspace {
                                id: ws_id,
                                column_idx: None,
                            },
                        )
                    }
                    AddWindowTarget::NextTo(next_to) => {
                        if let Some(output) = self
                            .interactive_move
                            .as_ref()
                            .and_then(|move_| {
                                if let InteractiveMoveState::Moving(move_) = move_ {
                                    Some(move_)
                                } else {
                                    None
                                }
                            })
                            .filter(|move_| next_to == move_.tile.window().id())
                            .map(|move_| move_.output.clone())
                        {
                            // The next_to window is being interactively moved.
                            let mon_idx = monitors
                                .iter()
                                .position(|mon| mon.output == output)
                                .unwrap_or(*active_monitor_idx);

                            (mon_idx, MonitorAddWindowTarget::Auto)
                        } else {
                            let mon_idx = monitors
                                .iter()
                                .position(|mon| {
                                    mon.workspaces.iter().any(|ws| ws.has_window(next_to))
                                })
                                .unwrap();
                            (mon_idx, MonitorAddWindowTarget::NextTo(next_to))
                        }
                    }
                };
                let mon = &mut monitors[mon_idx];

                mon.add_window(
                    window,
                    target,
                    activate,
                    resolved_width,
                    is_full_width,
                    is_floating,
                );

                if activate.map_smart(|| false) {
                    *active_monitor_idx = mon_idx;
                }

                // Set the default height for scrolling windows.
                if !is_floating {
                    if let Some(change) = resolved_height {
                        let ws = mon
                            .workspaces
                            .iter_mut()
                            .find(|ws| ws.has_window(&id))
                            .unwrap();
                        ws.set_window_height(Some(&id), change);
                    }
                }

                Some(&mon.output)
            }
            MonitorSet::NoOutputs { workspaces } => {
                let (ws_idx, target) = match target {
                    AddWindowTarget::Auto => {
                        if workspaces.is_empty() {
                            workspaces.push(Workspace::new_no_outputs(
                                self.clock.clone(),
                                self.options.clone(),
                            ));
                        }

                        (0, WorkspaceAddWindowTarget::Auto)
                    }
                    AddWindowTarget::Output(_) => panic!(),
                    AddWindowTarget::Workspace(ws_id) => {
                        let ws_idx = workspaces.iter().position(|ws| ws.id() == ws_id).unwrap();
                        (ws_idx, WorkspaceAddWindowTarget::Auto)
                    }
                    AddWindowTarget::NextTo(next_to) => {
                        if self
                            .interactive_move
                            .as_ref()
                            .and_then(|move_| {
                                if let InteractiveMoveState::Moving(move_) = move_ {
                                    Some(move_)
                                } else {
                                    None
                                }
                            })
                            .filter(|move_| next_to == move_.tile.window().id())
                            .is_some()
                        {
                            // The next_to window is being interactively moved.
                            (0, WorkspaceAddWindowTarget::Auto)
                        } else {
                            let ws_idx = workspaces
                                .iter()
                                .position(|ws| ws.has_window(next_to))
                                .unwrap();
                            (ws_idx, WorkspaceAddWindowTarget::NextTo(next_to))
                        }
                    }
                };
                let ws = &mut workspaces[ws_idx];

                let tile = ws.make_tile(window);
                ws.add_tile(
                    tile,
                    target,
                    activate,
                    resolved_width,
                    is_full_width,
                    is_floating,
                );

                // Set the default height for scrolling windows.
                if !is_floating {
                    if let Some(change) = resolved_height {
                        ws.set_window_height(Some(&id), change);
                    }
                }

                None
            }
        }
    }

    pub fn remove_window(
        &mut self,
        window: &W::Id,
        transaction: Transaction,
    ) -> Option<RemovedTile<W>> {
        if let Some(state) = &self.interactive_move {
            match state {
                InteractiveMoveState::Starting { window_id, .. } => {
                    if window_id == window {
                        self.interactive_move_end(window);
                    }
                }
                InteractiveMoveState::Moving(move_) => {
                    if move_.tile.window().id() == window {
                        let Some(InteractiveMoveState::Moving(move_)) =
                            self.interactive_move.take()
                        else {
                            unreachable!()
                        };
                        return Some(RemovedTile {
                            tile: move_.tile,
                            width: move_.width,
                            is_full_width: move_.is_full_width,
                            is_floating: false,
                        });
                    }
                }
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for (idx, ws) in mon.workspaces.iter_mut().enumerate() {
                        if ws.has_window(window) {
                            let removed = ws.remove_tile(window, transaction);

                            // Clean up empty workspaces that are not active and not last.
                            if !ws.has_windows_or_name()
                                && idx != mon.active_workspace_idx
                                && idx != mon.workspaces.len() - 1
                                && mon.workspace_switch.is_none()
                            {
                                mon.workspaces.remove(idx);

                                if idx < mon.active_workspace_idx {
                                    mon.active_workspace_idx -= 1;
                                }
                            }

                            // Special case handling when empty_workspace_above_first is set and all
                            // workspaces are empty.
                            if mon.options.empty_workspace_above_first
                                && mon.workspaces.len() == 2
                                && mon.workspace_switch.is_none()
                            {
                                assert!(!mon.workspaces[0].has_windows_or_name());
                                assert!(!mon.workspaces[1].has_windows_or_name());
                                mon.workspaces.remove(1);
                                mon.active_workspace_idx = 0;
                            }
                            return Some(removed);
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for (idx, ws) in workspaces.iter_mut().enumerate() {
                    if ws.has_window(window) {
                        let removed = ws.remove_tile(window, transaction);

                        // Clean up empty workspaces.
                        if !ws.has_windows_or_name() {
                            workspaces.remove(idx);
                        }

                        return Some(removed);
                    }
                }
            }
        }

        None
    }

    pub fn descendants_added(&mut self, id: &W::Id) -> bool {
        for ws in self.workspaces_mut() {
            if ws.descendants_added(id) {
                return true;
            }
        }

        false
    }

    pub fn update_window(&mut self, window: &W::Id, serial: Option<Serial>) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if move_.tile.window().id() == window {
                move_.tile.update_window();
                return;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.update_window(window, serial);
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.update_window(window, serial);
                        return;
                    }
                }
            }
        }
    }

    pub fn find_window_and_output(&self, wl_surface: &WlSurface) -> Option<(&W, &Output)> {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().is_wl_surface(wl_surface) {
                return Some((move_.tile.window(), &move_.output));
            }
        }

        if let MonitorSet::Normal { monitors, .. } = &self.monitor_set {
            for mon in monitors {
                for ws in &mon.workspaces {
                    if let Some(window) = ws.find_wl_surface(wl_surface) {
                        return Some((window, &mon.output));
                    }
                }
            }
        }

        None
    }

    pub fn find_workspace_by_id(&self, id: WorkspaceId) -> Option<(usize, &Workspace<W>)> {
        match &self.monitor_set {
            MonitorSet::Normal { ref monitors, .. } => {
                for mon in monitors {
                    if let Some((index, workspace)) = mon
                        .workspaces
                        .iter()
                        .enumerate()
                        .find(|(_, w)| w.id() == id)
                    {
                        return Some((index, workspace));
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces } => {
                if let Some((index, workspace)) =
                    workspaces.iter().enumerate().find(|(_, w)| w.id() == id)
                {
                    return Some((index, workspace));
                }
            }
        }

        None
    }

    pub fn find_workspace_by_name(&self, workspace_name: &str) -> Option<(usize, &Workspace<W>)> {
        match &self.monitor_set {
            MonitorSet::Normal { ref monitors, .. } => {
                for mon in monitors {
                    if let Some((index, workspace)) =
                        mon.workspaces.iter().enumerate().find(|(_, w)| {
                            w.name
                                .as_ref()
                                .map_or(false, |name| name.eq_ignore_ascii_case(workspace_name))
                        })
                    {
                        return Some((index, workspace));
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces } => {
                if let Some((index, workspace)) = workspaces.iter().enumerate().find(|(_, w)| {
                    w.name
                        .as_ref()
                        .map_or(false, |name| name.eq_ignore_ascii_case(workspace_name))
                }) {
                    return Some((index, workspace));
                }
            }
        }

        None
    }

    pub fn unname_workspace(&mut self, workspace_name: &str) {
        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    if mon.unname_workspace(workspace_name) {
                        if mon.workspace_switch.is_none() {
                            mon.clean_up_workspaces();
                        }
                        return;
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces } => {
                for (idx, ws) in workspaces.iter_mut().enumerate() {
                    if ws
                        .name
                        .as_ref()
                        .map_or(false, |name| name.eq_ignore_ascii_case(workspace_name))
                    {
                        ws.unname();

                        // Clean up empty workspaces.
                        if !ws.has_windows() {
                            workspaces.remove(idx);
                        }

                        return;
                    }
                }
            }
        }
    }

    pub fn find_window_and_output_mut(
        &mut self,
        wl_surface: &WlSurface,
    ) -> Option<(&mut W, Option<&Output>)> {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if move_.tile.window().is_wl_surface(wl_surface) {
                return Some((move_.tile.window_mut(), Some(&move_.output)));
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if let Some(window) = ws.find_wl_surface_mut(wl_surface) {
                            return Some((window, Some(&mon.output)));
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces } => {
                for ws in workspaces {
                    if let Some(window) = ws.find_wl_surface_mut(wl_surface) {
                        return Some((window, None));
                    }
                }
            }
        }

        None
    }

    /// Computes the window-geometry-relative target rect for popup unconstraining.
    ///
    /// We will try to fit popups inside this rect.
    pub fn popup_target_rect(&self, window: &W::Id) -> Rectangle<f64, Logical> {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                // Follow the scrolling layout logic and fit the popup horizontally within the
                // window geometry.
                let width = move_.tile.window_size().w;
                let height = output_size(&move_.output).h;
                let mut target = Rectangle::from_loc_and_size((0., 0.), (width, height));
                // FIXME: ideally this shouldn't include the tile render offset, but the code
                // duplication would be a bit annoying for this edge case.
                target.loc.y -= move_.tile_render_location().y;
                target.loc.y -= move_.tile.window_loc().y;
                return target;
            }
        }

        self.workspaces()
            .find_map(|(_, _, ws)| ws.popup_target_rect(window))
            .unwrap()
    }

    pub fn update_output_size(&mut self, output: &Output) {
        let _span = tracy_client::span!("Layout::update_output_size");

        let MonitorSet::Normal { monitors, .. } = &mut self.monitor_set else {
            panic!()
        };

        for mon in monitors {
            if &mon.output == output {
                for ws in &mut mon.workspaces {
                    ws.update_output_size();
                }

                break;
            }
        }
    }

    pub fn scroll_amount_to_activate(&self, window: &W::Id) -> f64 {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return 0.;
            }
        }

        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return 0.;
        };

        for mon in monitors {
            for ws in &mon.workspaces {
                if ws.has_window(window) {
                    return ws.scroll_amount_to_activate(window);
                }
            }
        }

        0.
    }

    pub fn should_trigger_focus_follows_mouse_on(&self, window: &W::Id) -> bool {
        // During an animation, it's easy to trigger focus-follows-mouse on the previous workspace,
        // especially when clicking to switch workspace on a bar of some kind. This cancels the
        // workspace switch, which is annoying and not intended.
        //
        // This function allows focus-follows-mouse to trigger only on the animation target
        // workspace.
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return true;
            }
        }

        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return true;
        };

        let (mon, ws_idx) = monitors
            .iter()
            .find_map(|mon| {
                mon.workspaces
                    .iter()
                    .position(|ws| ws.has_window(window))
                    .map(|ws_idx| (mon, ws_idx))
            })
            .unwrap();

        // During a gesture, focus-follows-mouse does not cause any unintended workspace switches.
        if let Some(WorkspaceSwitch::Gesture(_)) = mon.workspace_switch {
            return true;
        }

        ws_idx == mon.active_workspace_idx
    }

    pub fn activate_window(&mut self, window: &W::Id) {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return;
            }
        }

        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            return;
        };

        for (monitor_idx, mon) in monitors.iter_mut().enumerate() {
            for (workspace_idx, ws) in mon.workspaces.iter_mut().enumerate() {
                if ws.activate_window(window) {
                    *active_monitor_idx = monitor_idx;

                    // If currently in the middle of a vertical swipe between the target workspace
                    // and some other, don't switch the workspace.
                    match &mon.workspace_switch {
                        Some(WorkspaceSwitch::Gesture(gesture))
                            if gesture.current_idx.floor() == workspace_idx as f64
                                || gesture.current_idx.ceil() == workspace_idx as f64 => {}
                        _ => mon.switch_workspace(workspace_idx),
                    }

                    return;
                }
            }
        }
    }

    pub fn activate_window_without_raising(&mut self, window: &W::Id) {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return;
            }
        }

        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            return;
        };

        for (monitor_idx, mon) in monitors.iter_mut().enumerate() {
            for (workspace_idx, ws) in mon.workspaces.iter_mut().enumerate() {
                if ws.activate_window_without_raising(window) {
                    *active_monitor_idx = monitor_idx;

                    // If currently in the middle of a vertical swipe between the target workspace
                    // and some other, don't switch the workspace.
                    match &mon.workspace_switch {
                        Some(WorkspaceSwitch::Gesture(gesture))
                            if gesture.current_idx.floor() == workspace_idx as f64
                                || gesture.current_idx.ceil() == workspace_idx as f64 => {}
                        _ => mon.switch_workspace(workspace_idx),
                    }

                    return;
                }
            }
        }
    }

    pub fn activate_output(&mut self, output: &Output) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            return;
        };

        let idx = monitors
            .iter()
            .position(|mon| &mon.output == output)
            .unwrap();
        *active_monitor_idx = idx;
    }

    pub fn active_output(&self) -> Option<&Output> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &self.monitor_set
        else {
            return None;
        };

        Some(&monitors[*active_monitor_idx].output)
    }

    pub fn active_workspace(&self) -> Option<&Workspace<W>> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &self.monitor_set
        else {
            return None;
        };

        let mon = &monitors[*active_monitor_idx];
        Some(&mon.workspaces[mon.active_workspace_idx])
    }

    pub fn active_workspace_mut(&mut self) -> Option<&mut Workspace<W>> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            return None;
        };

        let mon = &mut monitors[*active_monitor_idx];
        Some(&mut mon.workspaces[mon.active_workspace_idx])
    }

    pub fn windows_for_output(&self, output: &Output) -> impl Iterator<Item = &W> + '_ {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            panic!()
        };

        let moving_window = self
            .interactive_move
            .as_ref()
            .and_then(|x| x.moving())
            .filter(|move_| move_.output == *output)
            .map(|move_| move_.tile.window())
            .into_iter();

        let mon = monitors.iter().find(|mon| &mon.output == output).unwrap();
        let mon_windows = mon.workspaces.iter().flat_map(|ws| ws.windows());

        moving_window.chain(mon_windows)
    }

    pub fn with_windows(&self, mut f: impl FnMut(&W, Option<&Output>, Option<WorkspaceId>)) {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            f(move_.tile.window(), Some(&move_.output), None);
        }

        match &self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mon.workspaces {
                        for win in ws.windows() {
                            f(win, Some(&mon.output), Some(ws.id()));
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces } => {
                for ws in workspaces {
                    for win in ws.windows() {
                        f(win, None, Some(ws.id()));
                    }
                }
            }
        }
    }

    pub fn with_windows_mut(&mut self, mut f: impl FnMut(&mut W, Option<&Output>)) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            f(move_.tile.window_mut(), Some(&move_.output));
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        for win in ws.windows_mut() {
                            f(win, Some(&mon.output));
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces } => {
                for ws in workspaces {
                    for win in ws.windows_mut() {
                        f(win, None);
                    }
                }
            }
        }
    }

    fn active_monitor(&mut self) -> Option<&mut Monitor<W>> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            return None;
        };

        Some(&mut monitors[*active_monitor_idx])
    }

    pub fn active_monitor_ref(&self) -> Option<&Monitor<W>> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &self.monitor_set
        else {
            return None;
        };

        Some(&monitors[*active_monitor_idx])
    }

    pub fn monitor_for_output(&self, output: &Output) -> Option<&Monitor<W>> {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return None;
        };

        monitors.iter().find(|mon| &mon.output == output)
    }

    pub fn monitor_for_output_mut(&mut self, output: &Output) -> Option<&mut Monitor<W>> {
        let MonitorSet::Normal { monitors, .. } = &mut self.monitor_set else {
            return None;
        };

        monitors.iter_mut().find(|mon| &mon.output == output)
    }

    pub fn monitor_for_workspace(&self, workspace_name: &str) -> Option<&Monitor<W>> {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return None;
        };

        monitors.iter().find(|monitor| {
            monitor.workspaces.iter().any(|ws| {
                ws.name
                    .as_ref()
                    .map_or(false, |name| name.eq_ignore_ascii_case(workspace_name))
            })
        })
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> + '_ {
        let monitors = if let MonitorSet::Normal { monitors, .. } = &self.monitor_set {
            &monitors[..]
        } else {
            &[][..]
        };

        monitors.iter().map(|mon| &mon.output)
    }

    pub fn move_left(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_left();
    }

    pub fn move_right(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_right();
    }

    pub fn move_column_to_first(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_column_to_first();
    }

    pub fn move_column_to_last(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_column_to_last();
    }

    pub fn move_column_left_or_to_output(&mut self, output: &Output) -> bool {
        if let Some(monitor) = self.active_monitor() {
            if monitor.move_left() {
                return false;
            }
        }

        self.move_column_to_output(output);
        true
    }

    pub fn move_column_right_or_to_output(&mut self, output: &Output) -> bool {
        if let Some(monitor) = self.active_monitor() {
            if monitor.move_right() {
                return false;
            }
        }

        self.move_column_to_output(output);
        true
    }

    pub fn move_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_down();
    }

    pub fn move_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_up();
    }

    pub fn move_down_or_to_workspace_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_down_or_to_workspace_down();
    }

    pub fn move_up_or_to_workspace_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_up_or_to_workspace_up();
    }

    pub fn consume_or_expel_window_left(&mut self, window: Option<&W::Id>) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.consume_or_expel_window_left(window);
    }

    pub fn consume_or_expel_window_right(&mut self, window: Option<&W::Id>) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.consume_or_expel_window_right(window);
    }

    pub fn focus_left(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_left();
    }

    pub fn focus_right(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_right();
    }

    pub fn focus_column_first(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_column_first();
    }

    pub fn focus_column_last(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_column_last();
    }

    pub fn focus_column_right_or_first(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_column_right_or_first();
    }

    pub fn focus_column_left_or_last(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_column_left_or_last();
    }

    pub fn focus_window_up_or_output(&mut self, output: &Output) -> bool {
        if let Some(monitor) = self.active_monitor() {
            if monitor.focus_up() {
                return false;
            }
        }

        self.focus_output(output);
        true
    }

    pub fn focus_window_down_or_output(&mut self, output: &Output) -> bool {
        if let Some(monitor) = self.active_monitor() {
            if monitor.focus_down() {
                return false;
            }
        }

        self.focus_output(output);
        true
    }

    pub fn focus_column_left_or_output(&mut self, output: &Output) -> bool {
        if let Some(monitor) = self.active_monitor() {
            if monitor.focus_left() {
                return false;
            }
        }

        self.focus_output(output);
        true
    }

    pub fn focus_column_right_or_output(&mut self, output: &Output) -> bool {
        if let Some(monitor) = self.active_monitor() {
            if monitor.focus_right() {
                return false;
            }
        }

        self.focus_output(output);
        true
    }

    pub fn focus_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_down();
    }

    pub fn focus_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_up();
    }

    pub fn focus_down_or_left(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_down_or_left();
    }

    pub fn focus_down_or_right(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_down_or_right();
    }

    pub fn focus_up_or_left(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_up_or_left();
    }

    pub fn focus_up_or_right(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_up_or_right();
    }

    pub fn focus_window_or_workspace_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_window_or_workspace_down();
    }

    pub fn focus_window_or_workspace_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.focus_window_or_workspace_up();
    }

    pub fn move_to_workspace_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_to_workspace_up();
    }

    pub fn move_to_workspace_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_to_workspace_down();
    }

    pub fn move_to_workspace(&mut self, window: Option<&W::Id>, idx: usize) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let monitor = if let Some(window) = window {
            match &mut self.monitor_set {
                MonitorSet::Normal { monitors, .. } => monitors
                    .iter_mut()
                    .find(|mon| mon.has_window(window))
                    .unwrap(),
                MonitorSet::NoOutputs { .. } => {
                    return;
                }
            }
        } else {
            let Some(monitor) = self.active_monitor() else {
                return;
            };
            monitor
        };
        monitor.move_to_workspace(window, idx);
    }

    pub fn move_column_to_workspace_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_column_to_workspace_up();
    }

    pub fn move_column_to_workspace_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_column_to_workspace_down();
    }

    pub fn move_column_to_workspace(&mut self, idx: usize) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_column_to_workspace(idx);
    }

    pub fn move_column_to_workspace_on_output(&mut self, output: &Output, idx: usize) {
        self.move_column_to_output(output);
        self.focus_output(output);
        self.move_column_to_workspace(idx);
    }

    pub fn switch_workspace_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.switch_workspace_up();
    }

    pub fn switch_workspace_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.switch_workspace_down();
    }

    pub fn switch_workspace(&mut self, idx: usize) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.switch_workspace(idx);
    }

    pub fn switch_workspace_auto_back_and_forth(&mut self, idx: usize) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.switch_workspace_auto_back_and_forth(idx);
    }

    pub fn switch_workspace_previous(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.switch_workspace_previous();
    }

    pub fn consume_into_column(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.consume_into_column();
    }

    pub fn expel_from_column(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.expel_from_column();
    }

    pub fn center_column(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.center_column();
    }

    pub fn center_window(&mut self, id: Option<&W::Id>) {
        let workspace = if let Some(id) = id {
            Some(self.workspaces_mut().find(|ws| ws.has_window(id)).unwrap())
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.center_window(id);
    }

    pub fn focus(&self) -> Option<&W> {
        self.focus_with_output().map(|(win, _out)| win)
    }

    pub fn focus_with_output(&self) -> Option<(&W, &Output)> {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            return Some((move_.tile.window(), &move_.output));
        }

        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &self.monitor_set
        else {
            return None;
        };

        let mon = &monitors[*active_monitor_idx];
        mon.active_window().map(|win| (win, &mon.output))
    }

    /// Returns the window under the cursor and the position of its toplevel surface within the
    /// output.
    ///
    /// `Some((w, Some(p)))` means that the cursor is within the window's input region and can be
    /// used for delivering events to the window. `Some((w, None))` means that the cursor is within
    /// the window's activation region, but not within the window's input region. For example, the
    /// cursor may be on the window's server-side border.
    pub fn window_under(
        &self,
        output: &Output,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<(&W, Option<Point<f64, Logical>>)> {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return None;
        };

        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            let tile_pos = move_.tile_render_location();
            let pos_within_tile = pos_within_output - tile_pos;

            if move_.tile.is_in_input_region(pos_within_tile) {
                let pos_within_surface = tile_pos + move_.tile.buf_loc();
                return Some((move_.tile.window(), Some(pos_within_surface)));
            } else if move_.tile.is_in_activation_region(pos_within_tile) {
                return Some((move_.tile.window(), None));
            }

            return None;
        };

        let mon = monitors.iter().find(|mon| &mon.output == output)?;
        mon.window_under(pos_within_output)
    }

    pub fn resize_edges_under(
        &self,
        output: &Output,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<ResizeEdge> {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return None;
        };

        let mon = monitors.iter().find(|mon| &mon.output == output)?;
        mon.resize_edges_under(pos_within_output)
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        use std::collections::HashSet;

        use approx::assert_abs_diff_eq;

        use crate::layout::monitor::WorkspaceSwitch;

        let mut move_win_id = None;
        if let Some(state) = &self.interactive_move {
            match state {
                InteractiveMoveState::Starting {
                    window_id,
                    pointer_delta: _,
                    pointer_ratio_within_window: _,
                } => {
                    assert!(
                        self.has_window(window_id),
                        "interactive move must be on an existing window"
                    );
                    move_win_id = Some(window_id.clone());
                }
                InteractiveMoveState::Moving(move_) => {
                    assert_eq!(self.clock, move_.tile.clock);

                    let scale = move_.output.current_scale().fractional_scale();
                    let options = Options::clone(&self.options).adjusted_for_scale(scale);
                    assert_eq!(
                        &*move_.tile.options, &options,
                        "interactive moved tile options must be \
                         base options adjusted for output scale"
                    );

                    let tile_pos = move_.tile_render_location();
                    let rounded_pos = tile_pos.to_physical_precise_round(scale).to_logical(scale);

                    // Tile position must be rounded to physical pixels.
                    assert_abs_diff_eq!(tile_pos.x, rounded_pos.x, epsilon = 1e-5);
                    assert_abs_diff_eq!(tile_pos.y, rounded_pos.y, epsilon = 1e-5);
                }
            }
        }

        let mut seen_workspace_id = HashSet::new();
        let mut seen_workspace_name = Vec::<String>::new();

        let (monitors, &primary_idx, &active_monitor_idx) = match &self.monitor_set {
            MonitorSet::Normal {
                monitors,
                primary_idx,
                active_monitor_idx,
            } => (monitors, primary_idx, active_monitor_idx),
            MonitorSet::NoOutputs { workspaces } => {
                for workspace in workspaces {
                    assert!(
                        workspace.has_windows_or_name(),
                        "with no outputs there cannot be empty unnamed workspaces"
                    );

                    assert_eq!(self.clock, workspace.clock);

                    assert_eq!(
                        workspace.base_options, self.options,
                        "workspace base options must be synchronized with layout"
                    );

                    let options = Options::clone(&workspace.base_options)
                        .adjusted_for_scale(workspace.scale().fractional_scale());
                    assert_eq!(
                        &*workspace.options, &options,
                        "workspace options must be base options adjusted for workspace scale"
                    );

                    assert!(
                        seen_workspace_id.insert(workspace.id()),
                        "workspace id must be unique"
                    );

                    if let Some(name) = &workspace.name {
                        assert!(
                            !seen_workspace_name
                                .iter()
                                .any(|n| n.eq_ignore_ascii_case(name)),
                            "workspace name must be unique"
                        );
                        seen_workspace_name.push(name.clone());
                    }

                    workspace.verify_invariants(move_win_id.as_ref());
                }

                return;
            }
        };

        assert!(primary_idx < monitors.len());
        assert!(active_monitor_idx < monitors.len());

        for (idx, monitor) in monitors.iter().enumerate() {
            assert!(
                !monitor.workspaces.is_empty(),
                "monitor must have at least one workspace"
            );
            assert!(monitor.active_workspace_idx < monitor.workspaces.len());

            assert_eq!(self.clock, monitor.clock);
            assert_eq!(
                monitor.options, self.options,
                "monitor options must be synchronized with layout"
            );

            if let Some(WorkspaceSwitch::Animation(anim)) = &monitor.workspace_switch {
                let before_idx = anim.from() as usize;
                let after_idx = anim.to() as usize;

                assert!(before_idx < monitor.workspaces.len());
                assert!(after_idx < monitor.workspaces.len());
            }

            if idx == primary_idx {
                for ws in &monitor.workspaces {
                    if ws.original_output.matches(&monitor.output) {
                        // This is the primary monitor's own workspace.
                        continue;
                    }

                    let own_monitor_exists = monitors
                        .iter()
                        .any(|m| ws.original_output.matches(&m.output));
                    assert!(
                        !own_monitor_exists,
                        "primary monitor cannot have workspaces for which their own monitor exists"
                    );
                }
            } else {
                assert!(
                    monitor
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.original_output.matches(&monitor.output)),
                    "secondary monitor must not have any non-own workspaces"
                );
            }

            assert!(
                !monitor.workspaces.last().unwrap().has_windows(),
                "monitor must have an empty workspace in the end"
            );
            if monitor.options.empty_workspace_above_first {
                assert!(
                    !monitor.workspaces.first().unwrap().has_windows(),
                    "first workspace must be empty when empty_workspace_above_first is set"
                )
            }

            assert!(
                monitor.workspaces.last().unwrap().name.is_none(),
                "monitor must have an unnamed workspace in the end"
            );
            if monitor.options.empty_workspace_above_first {
                assert!(
                    monitor.workspaces.first().unwrap().name.is_none(),
                    "first workspace must be unnamed when empty_workspace_above_first is set"
                )
            }

            if monitor.options.empty_workspace_above_first {
                assert!(
                    monitor.workspaces.len() != 2,
                    "if empty_workspace_above_first is set there must be just 1 or 3+ workspaces"
                )
            }

            // If there's no workspace switch in progress, there can't be any non-last non-active
            // empty workspaces. If empty_workspace_above_first is set then the first workspace
            // will be empty too.
            let pre_skip = if monitor.options.empty_workspace_above_first {
                1
            } else {
                0
            };
            if monitor.workspace_switch.is_none() {
                for (idx, ws) in monitor
                    .workspaces
                    .iter()
                    .enumerate()
                    .skip(pre_skip)
                    .rev()
                    // skip last
                    .skip(1)
                {
                    if idx != monitor.active_workspace_idx {
                        assert!(
                            ws.has_windows_or_name(),
                            "non-active workspace can't be empty and unnamed except the last one"
                        );
                    }
                }
            }

            // FIXME: verify that primary doesn't have any workspaces for which their own monitor
            // exists.

            for workspace in &monitor.workspaces {
                assert_eq!(self.clock, workspace.clock);

                assert_eq!(
                    workspace.base_options, self.options,
                    "workspace options must be synchronized with layout"
                );

                let options = Options::clone(&workspace.base_options)
                    .adjusted_for_scale(workspace.scale().fractional_scale());
                assert_eq!(
                    &*workspace.options, &options,
                    "workspace options must be base options adjusted for workspace scale"
                );

                assert!(
                    seen_workspace_id.insert(workspace.id()),
                    "workspace id must be unique"
                );

                if let Some(name) = &workspace.name {
                    assert!(
                        !seen_workspace_name
                            .iter()
                            .any(|n| n.eq_ignore_ascii_case(name)),
                        "workspace name must be unique"
                    );
                    seen_workspace_name.push(name.clone());
                }

                workspace.verify_invariants(move_win_id.as_ref());
            }
        }
    }

    pub fn advance_animations(&mut self) {
        let _span = tracy_client::span!("Layout::advance_animations");

        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            move_.tile.advance_animations();
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    mon.advance_animations();
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    ws.advance_animations();
                }
            }
        }
    }

    pub fn are_animations_ongoing(&self, output: Option<&Output>) -> bool {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.are_animations_ongoing() {
                return true;
            }
        }

        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return false;
        };

        for mon in monitors {
            if output.map_or(false, |output| mon.output != *output) {
                continue;
            }

            if mon.are_animations_ongoing() {
                return true;
            }
        }

        false
    }

    pub fn update_render_elements(&mut self, output: Option<&Output>) {
        let _span = tracy_client::span!("Layout::update_render_elements");

        self.update_render_elements_time = self.clock.now();

        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if output.map_or(true, |output| move_.output == *output) {
                let pos_within_output = move_.tile_render_location();
                let view_rect = Rectangle::from_loc_and_size(
                    pos_within_output.upscale(-1.),
                    output_size(&move_.output),
                );
                move_.tile.update(true, view_rect);
            }
        }

        self.update_insert_hint(output);

        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            error!("update_render_elements called with no monitors");
            return;
        };

        for (idx, mon) in monitors.iter_mut().enumerate() {
            if output.map_or(true, |output| mon.output == *output) {
                let is_active = self.is_active
                    && idx == *active_monitor_idx
                    && !matches!(self.interactive_move, Some(InteractiveMoveState::Moving(_)));
                mon.update_render_elements(is_active);
            }
        }
    }

    pub fn update_shaders(&mut self) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            move_.tile.update_shaders();
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        ws.update_shaders();
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    ws.update_shaders();
                }
            }
        }
    }

    fn update_insert_hint(&mut self, output: Option<&Output>) {
        let _span = tracy_client::span!("Layout::update_insert_hint");

        let _span = tracy_client::span!("Layout::update_insert_hint::clear");
        for ws in self.workspaces_mut() {
            ws.clear_insert_hint();
        }

        if !matches!(self.interactive_move, Some(InteractiveMoveState::Moving(_))) {
            return;
        }
        let Some(InteractiveMoveState::Moving(move_)) = self.interactive_move.take() else {
            unreachable!()
        };
        if output.map_or(false, |out| &move_.output != out) {
            self.interactive_move = Some(InteractiveMoveState::Moving(move_));
            return;
        }

        // No insert hint when targeting floating.
        if move_.is_floating {
            self.interactive_move = Some(InteractiveMoveState::Moving(move_));
            return;
        }

        let _span = tracy_client::span!("Layout::update_insert_hint::update");

        if let Some(mon) = self.monitor_for_output_mut(&move_.output) {
            if let Some((ws, offset)) = mon.workspace_under(move_.pointer_pos_within_output) {
                let ws_id = ws.id();
                let ws = mon
                    .workspaces
                    .iter_mut()
                    .find(|ws| ws.id() == ws_id)
                    .unwrap();

                let position = ws.get_insert_position(move_.pointer_pos_within_output - offset);

                let rules = move_.tile.window().rules();
                let border_width = move_.tile.effective_border_width().unwrap_or(0.);
                let corner_radius = rules
                    .geometry_corner_radius
                    .map_or(CornerRadius::default(), |radius| {
                        radius.expanded_by(border_width as f32)
                    });

                ws.set_insert_hint(InsertHint {
                    position,
                    width: move_.width,
                    is_full_width: move_.is_full_width,
                    corner_radius,
                });
            }
        }

        self.interactive_move = Some(InteractiveMoveState::Moving(move_));
    }

    pub fn ensure_named_workspace(&mut self, ws_config: &WorkspaceConfig) {
        if self.find_workspace_by_name(&ws_config.name.0).is_some() {
            return;
        }

        let clock = self.clock.clone();
        let options = self.options.clone();

        match &mut self.monitor_set {
            MonitorSet::Normal {
                monitors,
                primary_idx,
                active_monitor_idx,
            } => {
                let mon_idx = ws_config
                    .open_on_output
                    .as_deref()
                    .map(|name| {
                        monitors
                            .iter_mut()
                            .position(|monitor| output_matches_name(&monitor.output, name))
                            .unwrap_or(*primary_idx)
                    })
                    .unwrap_or(*active_monitor_idx);
                let mon = &mut monitors[mon_idx];

                let mut insert_idx = 0;
                if mon.options.empty_workspace_above_first {
                    // need to insert new empty workspace on top
                    mon.add_workspace_top();
                    insert_idx += 1;
                }

                let ws = Workspace::new_with_config(
                    mon.output.clone(),
                    Some(ws_config.clone()),
                    clock,
                    options,
                );
                mon.workspaces.insert(insert_idx, ws);
                mon.active_workspace_idx += 1;

                mon.workspace_switch = None;
                mon.clean_up_workspaces();
            }
            MonitorSet::NoOutputs { workspaces } => {
                let ws =
                    Workspace::new_with_config_no_outputs(Some(ws_config.clone()), clock, options);
                workspaces.insert(0, ws);
            }
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        self.update_options(Options::from_config(config));
    }

    fn update_options(&mut self, options: Options) {
        let options = Rc::new(options);

        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            let view_size = output_size(&move_.output);
            let scale = move_.output.current_scale().fractional_scale();
            move_.tile.update_config(
                view_size,
                scale,
                Rc::new(Options::clone(&options).adjusted_for_scale(scale)),
            );
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    mon.update_config(options.clone());
                }
            }
            MonitorSet::NoOutputs { workspaces } => {
                for ws in workspaces {
                    ws.update_config(options.clone());
                }
            }
        }

        self.options = options;
    }

    pub fn toggle_width(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.toggle_width();
    }

    pub fn toggle_window_width(&mut self, window: Option<&W::Id>) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.toggle_window_width(window);
    }

    pub fn toggle_window_height(&mut self, window: Option<&W::Id>) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.toggle_window_height(window);
    }

    pub fn toggle_full_width(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.toggle_full_width();
    }

    pub fn set_column_width(&mut self, change: SizeChange) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.set_column_width(change);
    }

    pub fn set_window_width(&mut self, window: Option<&W::Id>, change: SizeChange) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.set_window_width(window, change);
    }

    pub fn set_window_height(&mut self, window: Option<&W::Id>, change: SizeChange) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.set_window_height(window, change);
    }

    pub fn reset_window_height(&mut self, window: Option<&W::Id>) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.reset_window_height(window);
    }

    pub fn toggle_window_floating(&mut self, window: Option<&W::Id>) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                move_.is_floating = !move_.is_floating;

                // When going to floating, restore the floating window size.
                if move_.is_floating {
                    let floating_size = move_.tile.floating_window_size;
                    let win = move_.tile.window_mut();
                    let mut size =
                        floating_size.unwrap_or_else(|| win.expected_size().unwrap_or_default());

                    // Apply min/max size window rules. If requesting a concrete size, apply
                    // completely; if requesting (0, 0), apply only when min/max results in a fixed
                    // size.
                    let min_size = win.min_size();
                    let max_size = win.max_size();
                    size.w = ensure_min_max_size_maybe_zero(size.w, min_size.w, max_size.w);
                    size.h = ensure_min_max_size_maybe_zero(size.h, min_size.h, max_size.h);

                    win.request_size_once(size, true);
                }
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.toggle_window_floating(window);
    }

    pub fn set_window_floating(&mut self, window: Option<&W::Id>, floating: bool) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                if move_.is_floating != floating {
                    self.toggle_window_floating(window);
                }
                return;
            }
        }

        let workspace = if let Some(window) = window {
            Some(
                self.workspaces_mut()
                    .find(|ws| ws.has_window(window))
                    .unwrap(),
            )
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.set_window_floating(window, floating);
    }

    pub fn focus_floating(&mut self) {
        let Some(workspace) = self.active_workspace_mut() else {
            return;
        };
        workspace.focus_floating();
    }

    pub fn focus_tiling(&mut self) {
        let Some(workspace) = self.active_workspace_mut() else {
            return;
        };
        workspace.focus_tiling();
    }

    pub fn switch_focus_floating_tiling(&mut self) {
        let Some(workspace) = self.active_workspace_mut() else {
            return;
        };
        workspace.switch_focus_floating_tiling();
    }

    pub fn move_floating_window(
        &mut self,
        id: Option<&W::Id>,
        x: PositionChange,
        y: PositionChange,
        animate: bool,
    ) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if id.is_none() || id == Some(move_.tile.window().id()) {
                return;
            }
        }

        let workspace = if let Some(id) = id {
            Some(self.workspaces_mut().find(|ws| ws.has_window(id)).unwrap())
        } else {
            self.active_workspace_mut()
        };

        let Some(workspace) = workspace else {
            return;
        };
        workspace.move_floating_window(id, x, y, animate);
    }

    pub fn focus_output(&mut self, output: &Output) {
        if let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        {
            for (idx, mon) in monitors.iter().enumerate() {
                if &mon.output == output {
                    *active_monitor_idx = idx;
                    return;
                }
            }
        }
    }

    pub fn move_to_output(
        &mut self,
        window: Option<&W::Id>,
        output: &Output,
        target_ws_idx: Option<usize>,
    ) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if window.is_none() || window == Some(move_.tile.window().id()) {
                return;
            }
        }

        if let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        {
            let new_idx = monitors
                .iter()
                .position(|mon| &mon.output == output)
                .unwrap();

            let (mon_idx, ws_idx) = if let Some(window) = window {
                monitors
                    .iter()
                    .enumerate()
                    .find_map(|(mon_idx, mon)| {
                        mon.workspaces
                            .iter()
                            .position(|ws| ws.has_window(window))
                            .map(|ws_idx| (mon_idx, ws_idx))
                    })
                    .unwrap()
            } else {
                let mon_idx = *active_monitor_idx;
                let mon = &monitors[mon_idx];
                (mon_idx, mon.active_workspace_idx)
            };

            let workspace_idx = target_ws_idx.unwrap_or(monitors[new_idx].active_workspace_idx);
            if mon_idx == new_idx && ws_idx == workspace_idx {
                return;
            }
            let ws_id = monitors[new_idx].workspaces[workspace_idx].id();

            let mon = &mut monitors[mon_idx];
            let activate = window.map_or(true, |win| {
                mon_idx == *active_monitor_idx
                    && mon.active_window().map(|win| win.id()) == Some(win)
            });
            let activate = if activate {
                ActivateWindow::Yes
            } else {
                ActivateWindow::No
            };

            let ws = &mut mon.workspaces[ws_idx];
            let transaction = Transaction::new();
            let mut removed = if let Some(window) = window {
                ws.remove_tile(window, transaction)
            } else if let Some(removed) = ws.remove_active_tile(transaction) {
                removed
            } else {
                return;
            };

            removed.tile.stop_move_animations();

            let mon = &mut monitors[new_idx];
            mon.add_tile(
                removed.tile,
                MonitorAddWindowTarget::Workspace {
                    id: ws_id,
                    column_idx: None,
                },
                activate,
                removed.width,
                removed.is_full_width,
                removed.is_floating,
            );
            if activate.map_smart(|| false) {
                *active_monitor_idx = new_idx;
            }

            let mon = &mut monitors[mon_idx];
            if mon.workspace_switch.is_none() {
                monitors[mon_idx].clean_up_workspaces();
            }
        }
    }

    pub fn move_column_to_output(&mut self, output: &Output) {
        if let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        {
            let new_idx = monitors
                .iter()
                .position(|mon| &mon.output == output)
                .unwrap();

            let current = &mut monitors[*active_monitor_idx];
            let ws = current.active_workspace();

            if ws.floating_is_active() {
                self.move_to_output(None, output, None);
                return;
            }

            let Some(column) = ws.remove_active_column() else {
                return;
            };

            let workspace_idx = monitors[new_idx].active_workspace_idx;
            self.add_column_by_idx(new_idx, workspace_idx, column, true);
        }
    }

    pub fn move_workspace_to_output(&mut self, output: &Output) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            return;
        };

        let current = &mut monitors[*active_monitor_idx];
        if current.active_workspace_idx == current.workspaces.len() - 1 {
            // Insert a new empty workspace.
            current.add_workspace_bottom();
        }
        if current.options.empty_workspace_above_first && current.active_workspace_idx == 0 {
            current.add_workspace_top();
        }

        let mut ws = current.workspaces.remove(current.active_workspace_idx);
        current.active_workspace_idx = current.active_workspace_idx.saturating_sub(1);
        current.workspace_switch = None;
        current.clean_up_workspaces();

        ws.set_output(Some(output.clone()));
        ws.original_output = OutputId::new(output);

        let target_idx = monitors
            .iter()
            .position(|mon| &mon.output == output)
            .unwrap();
        let target = &mut monitors[target_idx];

        target.previous_workspace_id = Some(target.workspaces[target.active_workspace_idx].id());

        if target.options.empty_workspace_above_first && target.workspaces.len() == 1 {
            // Insert a new empty workspace on top to prepare for insertion of new workspce.
            target.add_workspace_top();
        }
        // Insert the workspace after the currently active one. Unless the currently active one is
        // the last empty workspace, then insert before.
        let target_ws_idx = min(target.active_workspace_idx + 1, target.workspaces.len() - 1);
        target.workspaces.insert(target_ws_idx, ws);
        target.active_workspace_idx = target_ws_idx;
        target.workspace_switch = None;
        target.clean_up_workspaces();

        *active_monitor_idx = target_idx;
    }

    pub fn set_fullscreen(&mut self, window: &W::Id, is_fullscreen: bool) {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.set_fullscreen(window, is_fullscreen);
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.set_fullscreen(window, is_fullscreen);
                        return;
                    }
                }
            }
        }
    }

    pub fn toggle_fullscreen(&mut self, window: &W::Id) {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.toggle_fullscreen(window);
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.toggle_fullscreen(window);
                        return;
                    }
                }
            }
        }
    }

    pub fn workspace_switch_gesture_begin(&mut self, output: &Output, is_touchpad: bool) {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => unreachable!(),
        };

        for monitor in monitors {
            // Cancel the gesture on other outputs.
            if &monitor.output != output {
                monitor.workspace_switch_gesture_end(true, None);
                continue;
            }

            monitor.workspace_switch_gesture_begin(is_touchpad);
        }
    }

    pub fn workspace_switch_gesture_update(
        &mut self,
        delta_y: f64,
        timestamp: Duration,
        is_touchpad: bool,
    ) -> Option<Option<Output>> {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => return None,
        };

        for monitor in monitors {
            if let Some(refresh) =
                monitor.workspace_switch_gesture_update(delta_y, timestamp, is_touchpad)
            {
                if refresh {
                    return Some(Some(monitor.output.clone()));
                } else {
                    return Some(None);
                }
            }
        }

        None
    }

    pub fn workspace_switch_gesture_end(
        &mut self,
        cancelled: bool,
        is_touchpad: Option<bool>,
    ) -> Option<Output> {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => return None,
        };

        for monitor in monitors {
            if monitor.workspace_switch_gesture_end(cancelled, is_touchpad) {
                return Some(monitor.output.clone());
            }
        }

        None
    }

    pub fn view_offset_gesture_begin(&mut self, output: &Output, is_touchpad: bool) {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => unreachable!(),
        };

        for monitor in monitors {
            for (idx, ws) in monitor.workspaces.iter_mut().enumerate() {
                // Cancel the gesture on other workspaces.
                if &monitor.output != output || idx != monitor.active_workspace_idx {
                    ws.view_offset_gesture_end(true, None);
                    continue;
                }

                ws.view_offset_gesture_begin(is_touchpad);
            }
        }
    }

    pub fn view_offset_gesture_update(
        &mut self,
        delta_x: f64,
        timestamp: Duration,
        is_touchpad: bool,
    ) -> Option<Option<Output>> {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => return None,
        };

        for monitor in monitors {
            for ws in &mut monitor.workspaces {
                if let Some(refresh) =
                    ws.view_offset_gesture_update(delta_x, timestamp, is_touchpad)
                {
                    if refresh {
                        return Some(Some(monitor.output.clone()));
                    } else {
                        return Some(None);
                    }
                }
            }
        }

        None
    }

    pub fn view_offset_gesture_end(
        &mut self,
        cancelled: bool,
        is_touchpad: Option<bool>,
    ) -> Option<Output> {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => return None,
        };

        for monitor in monitors {
            for ws in &mut monitor.workspaces {
                if ws.view_offset_gesture_end(cancelled, is_touchpad) {
                    return Some(monitor.output.clone());
                }
            }
        }

        None
    }

    pub fn interactive_move_begin(
        &mut self,
        window_id: W::Id,
        output: &Output,
        start_pos_within_output: Point<f64, Logical>,
    ) -> bool {
        if self.interactive_move.is_some() {
            return false;
        }

        let MonitorSet::Normal { monitors, .. } = &mut self.monitor_set else {
            return false;
        };

        let Some((mon, (ws, ws_offset))) = monitors.iter().find_map(|mon| {
            mon.workspaces_with_render_positions()
                .find(|(ws, _)| ws.has_window(&window_id))
                .map(|rv| (mon, rv))
        }) else {
            return false;
        };

        if mon.output() != output {
            return false;
        }

        let (tile, tile_offset, _visible) = ws
            .tiles_with_render_positions()
            .find(|(tile, _, _)| tile.window().id() == &window_id)
            .unwrap();
        let window_offset = tile.window_loc();

        let tile_pos = ws_offset + tile_offset;

        let pointer_offset_within_window = start_pos_within_output - tile_pos - window_offset;
        let window_size = tile.window_size();
        let pointer_ratio_within_window = (
            f64::clamp(pointer_offset_within_window.x / window_size.w, 0., 1.),
            f64::clamp(pointer_offset_within_window.y / window_size.h, 0., 1.),
        );

        self.interactive_move = Some(InteractiveMoveState::Starting {
            window_id,
            pointer_delta: Point::from((0., 0.)),
            pointer_ratio_within_window,
        });

        true
    }

    pub fn interactive_move_update(
        &mut self,
        window: &W::Id,
        delta: Point<f64, Logical>,
        output: Output,
        pointer_pos_within_output: Point<f64, Logical>,
    ) -> bool {
        let Some(state) = self.interactive_move.take() else {
            return false;
        };

        match state {
            InteractiveMoveState::Starting {
                window_id,
                mut pointer_delta,
                pointer_ratio_within_window,
            } => {
                if window_id != *window {
                    self.interactive_move = Some(InteractiveMoveState::Starting {
                        window_id,
                        pointer_delta,
                        pointer_ratio_within_window,
                    });
                    return false;
                }

                pointer_delta += delta;

                let (cx, cy) = (pointer_delta.x, pointer_delta.y);
                let sq_dist = cx * cx + cy * cy;

                let factor = RubberBand {
                    stiffness: 1.0,
                    limit: 0.5,
                }
                .band(sq_dist / INTERACTIVE_MOVE_START_THRESHOLD);

                let (is_floating, tile) = self
                    .workspaces_mut()
                    .find(|ws| ws.has_window(&window_id))
                    .map(|ws| {
                        (
                            ws.is_floating(&window_id),
                            ws.tiles_mut()
                                .find(|tile| *tile.window().id() == window_id)
                                .unwrap(),
                        )
                    })
                    .unwrap();
                tile.interactive_move_offset = pointer_delta.upscale(factor);

                // Put it back to be able to easily return.
                self.interactive_move = Some(InteractiveMoveState::Starting {
                    window_id: window_id.clone(),
                    pointer_delta,
                    pointer_ratio_within_window,
                });

                if !is_floating && sq_dist < INTERACTIVE_MOVE_START_THRESHOLD {
                    return true;
                }

                // If the pointer is currently on the window's own output, then we can animate the
                // window movement from its current (rubberbanded and possibly moved away) position
                // to the pointer. Otherwise, we just teleport it as the layout code is not aware
                // of monitor positions.
                //
                // FIXME: when and if the layout code knows about monitor positions, this will be
                // potentially animatable.
                let mut tile_pos = None;
                if let MonitorSet::Normal { monitors, .. } = &self.monitor_set {
                    if let Some((mon, (ws, ws_offset))) = monitors.iter().find_map(|mon| {
                        mon.workspaces_with_render_positions()
                            .find(|(ws, _)| ws.has_window(window))
                            .map(|rv| (mon, rv))
                    }) {
                        if mon.output() == &output {
                            let (_, tile_offset, _) = ws
                                .tiles_with_render_positions()
                                .find(|(tile, _, _)| tile.window().id() == window)
                                .unwrap();

                            tile_pos = Some(ws_offset + tile_offset);
                        }
                    }
                }

                let RemovedTile {
                    mut tile,
                    width,
                    is_full_width,
                    mut is_floating,
                } = self.remove_window(window, Transaction::new()).unwrap();

                tile.stop_move_animations();
                tile.interactive_move_offset = Point::from((0., 0.));
                tile.window().output_enter(&output);
                tile.window().set_preferred_scale_transform(
                    output.current_scale(),
                    output.current_transform(),
                );

                let view_size = output_size(&output);
                let scale = output.current_scale().fractional_scale();
                tile.update_config(
                    view_size,
                    scale,
                    Rc::new(Options::clone(&self.options).adjusted_for_scale(scale)),
                );

                // Unfullscreen.
                let floating_size = tile.floating_window_size;
                let unfullscreen_to_floating = tile.unfullscreen_to_floating;
                let win = tile.window_mut();
                if win.is_pending_fullscreen() {
                    // If we're unfullscreening to floating, use the stored floating size,
                    // otherwise use (0, 0).
                    let mut size = if unfullscreen_to_floating {
                        floating_size.unwrap_or_default()
                    } else {
                        Size::from((0, 0))
                    };

                    // Apply min/max size window rules. If requesting a concrete size, apply
                    // completely; if requesting (0, 0), apply only when min/max results in a fixed
                    // size.
                    let min_size = win.min_size();
                    let max_size = win.max_size();
                    size.w = ensure_min_max_size_maybe_zero(size.w, min_size.w, max_size.w);
                    size.h = ensure_min_max_size_maybe_zero(size.h, min_size.h, max_size.h);

                    win.request_size_once(size, true);

                    // If we're unfullscreening to floating, default to the floating layout.
                    is_floating = unfullscreen_to_floating;
                }

                let mut data = InteractiveMoveData {
                    tile,
                    output,
                    pointer_pos_within_output,
                    width,
                    is_full_width,
                    is_floating,
                    pointer_ratio_within_window,
                };

                if let Some(tile_pos) = tile_pos {
                    let new_tile_pos = data.tile_render_location();
                    data.tile.animate_move_from(tile_pos - new_tile_pos);
                }

                self.interactive_move = Some(InteractiveMoveState::Moving(data));
            }
            InteractiveMoveState::Moving(mut move_) => {
                if window != move_.tile.window().id() {
                    self.interactive_move = Some(InteractiveMoveState::Moving(move_));
                    return false;
                }

                if output != move_.output {
                    move_.tile.window().output_leave(&move_.output);
                    move_.tile.window().output_enter(&output);
                    move_.tile.window().set_preferred_scale_transform(
                        output.current_scale(),
                        output.current_transform(),
                    );
                    let view_size = output_size(&output);
                    let scale = output.current_scale().fractional_scale();
                    move_.tile.update_config(
                        view_size,
                        scale,
                        Rc::new(Options::clone(&self.options).adjusted_for_scale(scale)),
                    );
                    move_.output = output.clone();
                    self.focus_output(&output);
                }

                move_.pointer_pos_within_output = pointer_pos_within_output;

                self.interactive_move = Some(InteractiveMoveState::Moving(move_));
            }
        }

        true
    }

    pub fn interactive_move_end(&mut self, window: &W::Id) {
        let Some(move_) = &self.interactive_move else {
            return;
        };

        let move_ = match move_ {
            InteractiveMoveState::Starting { window_id, .. } => {
                if window_id != window {
                    return;
                }

                let Some(InteractiveMoveState::Starting { window_id, .. }) =
                    self.interactive_move.take()
                else {
                    unreachable!()
                };

                let tile = self
                    .workspaces_mut()
                    .flat_map(|ws| ws.tiles_mut())
                    .find(|tile| *tile.window().id() == window_id)
                    .unwrap();
                let offset = tile.interactive_move_offset;
                tile.interactive_move_offset = Point::from((0., 0.));
                tile.animate_move_from(offset);

                return;
            }
            InteractiveMoveState::Moving(move_) => move_,
        };

        if window != move_.tile.window().id() {
            return;
        }

        let Some(InteractiveMoveState::Moving(move_)) = self.interactive_move.take() else {
            unreachable!()
        };

        match &mut self.monitor_set {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                let (mon, ws_idx, position, offset) = if let Some(mon) =
                    monitors.iter_mut().find(|mon| mon.output == move_.output)
                {
                    let (ws, offset) = mon
                        .workspace_under(move_.pointer_pos_within_output)
                        // If the pointer is somehow outside the move output and a workspace switch
                        // is in progress, this won't necessarily do the expected thing, but also
                        // that is not really supposed to happen so eh?
                        .unwrap_or_else(|| mon.workspaces_with_render_positions().next().unwrap());

                    let ws_id = ws.id();
                    let ws_idx = mon
                        .workspaces
                        .iter_mut()
                        .position(|ws| ws.id() == ws_id)
                        .unwrap();

                    let position = if move_.is_floating {
                        InsertPosition::Floating
                    } else {
                        let ws = &mut mon.workspaces[ws_idx];
                        ws.get_insert_position(move_.pointer_pos_within_output - offset)
                    };

                    (mon, ws_idx, position, offset)
                } else {
                    let mon = &mut monitors[*active_monitor_idx];
                    // No point in trying to use the pointer position on the wrong output.
                    let (ws, offset) = mon.workspaces_with_render_positions().next().unwrap();

                    let position = if move_.is_floating {
                        InsertPosition::Floating
                    } else {
                        ws.get_insert_position(Point::from((0., 0.)))
                    };

                    let ws_id = ws.id();
                    let ws_idx = mon
                        .workspaces
                        .iter_mut()
                        .position(|ws| ws.id() == ws_id)
                        .unwrap();
                    (mon, ws_idx, position, offset)
                };

                let win_id = move_.tile.window().id().clone();
                let window_render_loc = move_.tile_render_location() + move_.tile.window_loc();

                match position {
                    InsertPosition::NewColumn(column_idx) => {
                        let ws_id = mon.workspaces[ws_idx].id();
                        mon.add_tile(
                            move_.tile,
                            MonitorAddWindowTarget::Workspace {
                                id: ws_id,
                                column_idx: Some(column_idx),
                            },
                            ActivateWindow::Yes,
                            move_.width,
                            move_.is_full_width,
                            false,
                        );
                    }
                    InsertPosition::InColumn(column_idx, tile_idx) => {
                        mon.add_tile_to_column(
                            ws_idx,
                            column_idx,
                            Some(tile_idx),
                            move_.tile,
                            true,
                        );
                    }
                    InsertPosition::Floating => {
                        let pos = move_.tile_render_location() - offset;

                        let mut tile = move_.tile;
                        let pos = mon.workspaces[ws_idx].floating_logical_to_size_frac(pos);
                        tile.floating_pos = Some(pos);

                        // Set the floating size so it takes into account any window resizing that
                        // took place during the move.
                        if let Some(size) = tile.window().expected_size() {
                            tile.floating_window_size = Some(size);
                        }

                        let ws_id = mon.workspaces[ws_idx].id();
                        mon.add_tile(
                            tile,
                            MonitorAddWindowTarget::Workspace {
                                id: ws_id,
                                column_idx: None,
                            },
                            ActivateWindow::Yes,
                            move_.width,
                            move_.is_full_width,
                            true,
                        );
                    }
                }

                // needed because empty_workspace_above_first could have modified the idx
                let ws_idx = mon.active_workspace_idx();
                let ws = &mut mon.workspaces[ws_idx];
                let (tile, tile_render_loc) = ws
                    .tiles_with_render_positions_mut(false)
                    .find(|(tile, _)| tile.window().id() == &win_id)
                    .unwrap();
                let new_window_render_loc = offset + tile_render_loc + tile.window_loc();

                tile.animate_move_from(window_render_loc - new_window_render_loc);
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                if workspaces.is_empty() {
                    workspaces.push(Workspace::new_no_outputs(
                        self.clock.clone(),
                        self.options.clone(),
                    ));
                }
                let ws = &mut workspaces[0];

                // No point in trying to use the pointer position without outputs.
                ws.add_tile(
                    move_.tile,
                    WorkspaceAddWindowTarget::Auto,
                    ActivateWindow::Yes,
                    move_.width,
                    move_.is_full_width,
                    move_.is_floating,
                );
            }
        }
    }

    pub fn interactive_move_is_moving_above_output(&self, output: &Output) -> bool {
        let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move else {
            return false;
        };

        move_.output == *output
    }

    pub fn interactive_resize_begin(&mut self, window: W::Id, edges: ResizeEdge) -> bool {
        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(&window) {
                            return ws.interactive_resize_begin(window, edges);
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(&window) {
                        return ws.interactive_resize_begin(window, edges);
                    }
                }
            }
        }

        false
    }

    pub fn interactive_resize_update(
        &mut self,
        window: &W::Id,
        delta: Point<f64, Logical>,
    ) -> bool {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return false;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            return ws.interactive_resize_update(window, delta);
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        return ws.interactive_resize_update(window, delta);
                    }
                }
            }
        }

        false
    }

    pub fn interactive_resize_end(&mut self, window: &W::Id) {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.interactive_resize_end(Some(window));
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.interactive_resize_end(Some(window));
                        return;
                    }
                }
            }
        }
    }

    pub fn move_workspace_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_workspace_down();
    }

    pub fn move_workspace_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_workspace_up();
    }

    pub fn start_open_animation_for_window(&mut self, window: &W::Id) {
        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if move_.tile.window().id() == window {
                return;
            }
        }

        for ws in self.workspaces_mut() {
            for tile in ws.tiles_mut() {
                if tile.window().id() == window {
                    tile.start_open_animation();
                    return;
                }
            }
        }
    }

    pub fn store_unmap_snapshot(&mut self, renderer: &mut GlesRenderer, window: &W::Id) {
        let _span = tracy_client::span!("Layout::store_unmap_snapshot");

        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if move_.tile.window().id() == window {
                let scale = Scale::from(move_.output.current_scale().fractional_scale());
                move_.tile.store_unmap_snapshot_if_empty(renderer, scale);
                return;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.store_unmap_snapshot_if_empty(renderer, window);
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.store_unmap_snapshot_if_empty(renderer, window);
                        return;
                    }
                }
            }
        }
    }

    pub fn clear_unmap_snapshot(&mut self, window: &W::Id) {
        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if move_.tile.window().id() == window {
                let _ = move_.tile.take_unmap_snapshot();
                return;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.clear_unmap_snapshot(window);
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.clear_unmap_snapshot(window);
                        return;
                    }
                }
            }
        }
    }

    pub fn start_close_animation_for_window(
        &mut self,
        renderer: &mut GlesRenderer,
        window: &W::Id,
        blocker: TransactionBlocker,
    ) {
        let _span = tracy_client::span!("Layout::start_close_animation_for_window");

        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            if move_.tile.window().id() == window {
                let Some(snapshot) = move_.tile.take_unmap_snapshot() else {
                    return;
                };
                let tile_pos = move_.tile_render_location();
                let tile_size = move_.tile.tile_size();

                let output = move_.output.clone();
                let pointer_pos_within_output = move_.pointer_pos_within_output;
                let Some(mon) = self.monitor_for_output_mut(&output) else {
                    return;
                };
                let Some((ws, offset)) = mon.workspace_under(pointer_pos_within_output) else {
                    return;
                };
                let ws_id = ws.id();
                let ws = mon
                    .workspaces
                    .iter_mut()
                    .find(|ws| ws.id() == ws_id)
                    .unwrap();

                let tile_pos = tile_pos - offset;
                ws.start_close_animation_for_tile(renderer, snapshot, tile_size, tile_pos, blocker);
                return;
            }
        }

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.start_close_animation_for_window(renderer, window, blocker);
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.start_close_animation_for_window(renderer, window, blocker);
                        return;
                    }
                }
            }
        }
    }

    pub fn render_floating_for_output<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
        target: RenderTarget,
    ) -> impl Iterator<Item = TileRenderElement<R>> {
        if self.update_render_elements_time != self.clock.now() {
            error!("clock moved between updating render elements and rendering");
        }

        let mut rv = None;

        if let Some(InteractiveMoveState::Moving(move_)) = &self.interactive_move {
            if &move_.output == output {
                let scale = Scale::from(move_.output.current_scale().fractional_scale());
                let location = move_.tile_render_location();
                rv = Some(move_.tile.render(renderer, location, scale, true, target));
            }
        }

        rv.into_iter().flatten()
    }

    pub fn refresh(&mut self, is_active: bool) {
        let _span = tracy_client::span!("Layout::refresh");

        self.is_active = is_active;

        if let Some(InteractiveMoveState::Moving(move_)) = &mut self.interactive_move {
            let win = move_.tile.window_mut();

            win.set_active_in_column(true);
            win.set_floating(move_.is_floating);
            win.set_activated(true);

            win.set_interactive_resize(None);

            win.set_bounds(output_size(&move_.output).to_i32_round());

            win.send_pending_configure();
            win.refresh();
        }

        match &mut self.monitor_set {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                for (idx, mon) in monitors.iter_mut().enumerate() {
                    let is_active = self.is_active
                        && idx == *active_monitor_idx
                        && !matches!(self.interactive_move, Some(InteractiveMoveState::Moving(_)));
                    for (ws_idx, ws) in mon.workspaces.iter_mut().enumerate() {
                        ws.refresh(is_active);

                        // Cancel the view offset gesture after workspace switches, moves, etc.
                        if ws_idx != mon.active_workspace_idx {
                            ws.view_offset_gesture_end(false, None);
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    ws.refresh(false);
                    ws.view_offset_gesture_end(false, None);
                }
            }
        }
    }

    pub fn workspaces(
        &self,
    ) -> impl Iterator<Item = (Option<&Monitor<W>>, usize, &Workspace<W>)> + '_ {
        let iter_normal;
        let iter_no_outputs;

        match &self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                let it = monitors.iter().flat_map(|mon| {
                    mon.workspaces
                        .iter()
                        .enumerate()
                        .map(move |(idx, ws)| (Some(mon), idx, ws))
                });

                iter_normal = Some(it);
                iter_no_outputs = None;
            }
            MonitorSet::NoOutputs { workspaces } => {
                let it = workspaces
                    .iter()
                    .enumerate()
                    .map(|(idx, ws)| (None, idx, ws));

                iter_normal = None;
                iter_no_outputs = Some(it);
            }
        }

        let iter_normal = iter_normal.into_iter().flatten();
        let iter_no_outputs = iter_no_outputs.into_iter().flatten();
        iter_normal.chain(iter_no_outputs)
    }

    pub fn workspaces_mut(&mut self) -> impl Iterator<Item = &mut Workspace<W>> + '_ {
        let iter_normal;
        let iter_no_outputs;

        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                let it = monitors
                    .iter_mut()
                    .flat_map(|mon| mon.workspaces.iter_mut());

                iter_normal = Some(it);
                iter_no_outputs = None;
            }
            MonitorSet::NoOutputs { workspaces } => {
                let it = workspaces.iter_mut();

                iter_normal = None;
                iter_no_outputs = Some(it);
            }
        }

        let iter_normal = iter_normal.into_iter().flatten();
        let iter_no_outputs = iter_no_outputs.into_iter().flatten();
        iter_normal.chain(iter_no_outputs)
    }

    pub fn windows(&self) -> impl Iterator<Item = (Option<&Monitor<W>>, &W)> {
        let moving_window = self
            .interactive_move
            .as_ref()
            .and_then(|x| x.moving())
            .map(|move_| (self.monitor_for_output(&move_.output), move_.tile.window()))
            .into_iter();

        let rest = self
            .workspaces()
            .flat_map(|(mon, _, ws)| ws.windows().map(move |win| (mon, win)));

        moving_window.chain(rest)
    }

    pub fn has_window(&self, window: &W::Id) -> bool {
        self.windows().any(|(_, win)| win.id() == window)
    }

    fn resolve_default_width(
        &self,
        window: &W,
        width: Option<ColumnWidth>,
        is_floating: bool,
    ) -> ColumnWidth {
        let mut width = width.unwrap_or_else(|| ColumnWidth::Fixed(f64::from(window.size().w)));
        if is_floating {
            return width;
        }

        // Add border width to account for the issue that the scrolling layout currently doesn't
        // take borders into account for fixed sizes.
        if let ColumnWidth::Fixed(w) = &mut width {
            let rules = window.rules();
            let border_config = rules.border.resolve_against(self.options.border);
            if !border_config.off {
                *w += border_config.width.0 * 2.;
            }
        }

        width
    }
}

impl<W: LayoutElement> Default for MonitorSet<W> {
    fn default() -> Self {
        Self::NoOutputs { workspaces: vec![] }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use niri_config::{FloatOrInt, OutputName, WorkspaceName};
    use proptest::prelude::*;
    use proptest_derive::Arbitrary;
    use smithay::output::{Mode, PhysicalProperties, Subpixel};
    use smithay::utils::Rectangle;

    use super::*;

    impl<W: LayoutElement> Default for Layout<W> {
        fn default() -> Self {
            Self::with_options(Clock::with_time(Duration::ZERO), Default::default())
        }
    }

    #[derive(Debug)]
    struct TestWindowInner {
        id: usize,
        parent_id: Cell<Option<usize>>,
        bbox: Cell<Rectangle<i32, Logical>>,
        initial_bbox: Rectangle<i32, Logical>,
        requested_size: Cell<Option<Size<i32, Logical>>>,
        min_size: Size<i32, Logical>,
        max_size: Size<i32, Logical>,
        pending_fullscreen: Cell<bool>,
        pending_activated: Cell<bool>,
    }

    #[derive(Debug, Clone)]
    struct TestWindow(Rc<TestWindowInner>);

    #[derive(Debug, Clone, Copy, Arbitrary)]
    struct TestWindowParams {
        #[proptest(strategy = "1..=5usize")]
        id: usize,
        #[proptest(strategy = "arbitrary_parent_id()")]
        parent_id: Option<usize>,
        is_floating: bool,
        #[proptest(strategy = "arbitrary_bbox()")]
        bbox: Rectangle<i32, Logical>,
        #[proptest(strategy = "arbitrary_min_max_size()")]
        min_max_size: (Size<i32, Logical>, Size<i32, Logical>),
    }

    impl TestWindowParams {
        pub fn new(id: usize) -> Self {
            Self {
                id,
                parent_id: None,
                is_floating: false,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                min_max_size: Default::default(),
            }
        }
    }

    impl TestWindow {
        fn new(params: TestWindowParams) -> Self {
            Self(Rc::new(TestWindowInner {
                id: params.id,
                parent_id: Cell::new(params.parent_id),
                bbox: Cell::new(params.bbox),
                initial_bbox: params.bbox,
                requested_size: Cell::new(None),
                min_size: params.min_max_size.0,
                max_size: params.min_max_size.1,
                pending_fullscreen: Cell::new(false),
                pending_activated: Cell::new(false),
            }))
        }

        fn communicate(&self) -> bool {
            if let Some(size) = self.0.requested_size.get() {
                assert!(size.w >= 0);
                assert!(size.h >= 0);

                let mut new_bbox = self.0.initial_bbox;
                if size.w != 0 {
                    new_bbox.size.w = size.w;
                }
                if size.h != 0 {
                    new_bbox.size.h = size.h;
                }

                if self.0.bbox.get() != new_bbox {
                    self.0.bbox.set(new_bbox);
                    return true;
                }
            }

            false
        }
    }

    impl LayoutElement for TestWindow {
        type Id = usize;

        fn id(&self) -> &Self::Id {
            &self.0.id
        }

        fn size(&self) -> Size<i32, Logical> {
            self.0.bbox.get().size
        }

        fn buf_loc(&self) -> Point<i32, Logical> {
            (0, 0).into()
        }

        fn is_in_input_region(&self, _point: Point<f64, Logical>) -> bool {
            false
        }

        fn render<R: NiriRenderer>(
            &self,
            _renderer: &mut R,
            _location: Point<f64, Logical>,
            _scale: Scale<f64>,
            _alpha: f32,
            _target: RenderTarget,
        ) -> SplitElements<LayoutElementRenderElement<R>> {
            SplitElements::default()
        }

        fn request_size(
            &mut self,
            size: Size<i32, Logical>,
            _animate: bool,
            _transaction: Option<Transaction>,
        ) {
            self.0.requested_size.set(Some(size));
            self.0.pending_fullscreen.set(false);
        }

        fn request_fullscreen(&mut self, _size: Size<i32, Logical>) {
            self.0.pending_fullscreen.set(true);
        }

        fn min_size(&self) -> Size<i32, Logical> {
            self.0.min_size
        }

        fn max_size(&self) -> Size<i32, Logical> {
            self.0.max_size
        }

        fn is_wl_surface(&self, _wl_surface: &WlSurface) -> bool {
            false
        }

        fn set_preferred_scale_transform(&self, _scale: output::Scale, _transform: Transform) {}

        fn has_ssd(&self) -> bool {
            false
        }

        fn output_enter(&self, _output: &Output) {}

        fn output_leave(&self, _output: &Output) {}

        fn set_offscreen_element_id(&self, _id: Option<Id>) {}

        fn set_activated(&mut self, active: bool) {
            self.0.pending_activated.set(active);
        }

        fn set_bounds(&self, _bounds: Size<i32, Logical>) {}

        fn configure_intent(&self) -> ConfigureIntent {
            ConfigureIntent::CanSend
        }

        fn send_pending_configure(&mut self) {}

        fn set_active_in_column(&mut self, _active: bool) {}

        fn set_floating(&mut self, _floating: bool) {}

        fn is_fullscreen(&self) -> bool {
            false
        }

        fn is_pending_fullscreen(&self) -> bool {
            self.0.pending_fullscreen.get()
        }

        fn requested_size(&self) -> Option<Size<i32, Logical>> {
            self.0.requested_size.get()
        }

        fn is_child_of(&self, parent: &Self) -> bool {
            self.0.parent_id.get() == Some(parent.0.id)
        }

        fn refresh(&self) {}

        fn rules(&self) -> &ResolvedWindowRules {
            static EMPTY: ResolvedWindowRules = ResolvedWindowRules::empty();
            &EMPTY
        }

        fn animation_snapshot(&self) -> Option<&LayoutElementRenderSnapshot> {
            None
        }

        fn take_animation_snapshot(&mut self) -> Option<LayoutElementRenderSnapshot> {
            None
        }

        fn set_interactive_resize(&mut self, _data: Option<InteractiveResizeData>) {}

        fn cancel_interactive_resize(&mut self) {}

        fn on_commit(&mut self, _serial: Serial) {}

        fn interactive_resize_data(&self) -> Option<InteractiveResizeData> {
            None
        }
    }

    fn arbitrary_bbox() -> impl Strategy<Value = Rectangle<i32, Logical>> {
        any::<(i16, i16, u16, u16)>().prop_map(|(x, y, w, h)| {
            let loc: Point<i32, _> = Point::from((x.into(), y.into()));
            let size: Size<i32, _> = Size::from((w.max(1).into(), h.max(1).into()));
            Rectangle::from_loc_and_size(loc, size)
        })
    }

    fn arbitrary_size_change() -> impl Strategy<Value = SizeChange> {
        prop_oneof![
            (0..).prop_map(SizeChange::SetFixed),
            (0f64..).prop_map(SizeChange::SetProportion),
            any::<i32>().prop_map(SizeChange::AdjustFixed),
            any::<f64>().prop_map(SizeChange::AdjustProportion),
            // Interactive resize can have negative values here.
            Just(SizeChange::SetFixed(-100)),
        ]
    }

    fn arbitrary_position_change() -> impl Strategy<Value = PositionChange> {
        prop_oneof![
            (-1000f64..1000f64).prop_map(PositionChange::SetFixed),
            (-1000f64..1000f64).prop_map(PositionChange::AdjustFixed),
            any::<f64>().prop_map(PositionChange::SetFixed),
            any::<f64>().prop_map(PositionChange::AdjustFixed),
        ]
    }

    fn arbitrary_min_max() -> impl Strategy<Value = (i32, i32)> {
        prop_oneof![
            Just((0, 0)),
            (1..65536).prop_map(|n| (n, n)),
            (1..65536).prop_map(|min| (min, 0)),
            (1..).prop_map(|max| (0, max)),
            (1..65536, 1..).prop_map(|(min, max): (i32, i32)| (min, max.max(min))),
        ]
    }

    fn arbitrary_min_max_size() -> impl Strategy<Value = (Size<i32, Logical>, Size<i32, Logical>)> {
        prop_oneof![
            5 => (arbitrary_min_max(), arbitrary_min_max()).prop_map(
                |((min_w, max_w), (min_h, max_h))| {
                    let min_size = Size::from((min_w, min_h));
                    let max_size = Size::from((max_w, max_h));
                    (min_size, max_size)
                },
            ),
            1 => arbitrary_min_max().prop_map(|(w, h)| {
                let size = Size::from((w, h));
                (size, size)
            }),
        ]
    }

    fn arbitrary_view_offset_gesture_delta() -> impl Strategy<Value = f64> {
        prop_oneof![(-10f64..10f64), (-50000f64..50000f64),]
    }

    fn arbitrary_resize_edge() -> impl Strategy<Value = ResizeEdge> {
        prop_oneof![
            Just(ResizeEdge::RIGHT),
            Just(ResizeEdge::BOTTOM),
            Just(ResizeEdge::LEFT),
            Just(ResizeEdge::TOP),
            Just(ResizeEdge::BOTTOM_RIGHT),
            Just(ResizeEdge::BOTTOM_LEFT),
            Just(ResizeEdge::TOP_RIGHT),
            Just(ResizeEdge::TOP_LEFT),
            Just(ResizeEdge::empty()),
        ]
    }

    fn arbitrary_scale() -> impl Strategy<Value = f64> {
        prop_oneof![Just(1.), Just(1.5), Just(2.),]
    }

    fn arbitrary_msec_delta() -> impl Strategy<Value = i32> {
        prop_oneof![
            1 => Just(-1000),
            2 => Just(-10),
            1 => Just(0),
            2 => Just(10),
            6 => Just(1000),
        ]
    }

    fn arbitrary_parent_id() -> impl Strategy<Value = Option<usize>> {
        prop_oneof![
            5 => Just(None),
            1 => prop::option::of(1..=5usize),
        ]
    }

    #[derive(Debug, Clone, Copy, Arbitrary)]
    enum Op {
        AddOutput(#[proptest(strategy = "1..=5usize")] usize),
        AddScaledOutput {
            #[proptest(strategy = "1..=5usize")]
            id: usize,
            #[proptest(strategy = "arbitrary_scale()")]
            scale: f64,
        },
        RemoveOutput(#[proptest(strategy = "1..=5usize")] usize),
        FocusOutput(#[proptest(strategy = "1..=5usize")] usize),
        AddNamedWorkspace {
            #[proptest(strategy = "1..=5usize")]
            ws_name: usize,
            #[proptest(strategy = "prop::option::of(1..=5usize)")]
            output_name: Option<usize>,
        },
        UnnameWorkspace {
            #[proptest(strategy = "1..=5usize")]
            ws_name: usize,
        },
        AddWindow {
            params: TestWindowParams,
        },
        AddWindowNextTo {
            params: TestWindowParams,
            #[proptest(strategy = "1..=5usize")]
            next_to_id: usize,
        },
        AddWindowToNamedWorkspace {
            params: TestWindowParams,
            #[proptest(strategy = "1..=5usize")]
            ws_name: usize,
        },
        CloseWindow(#[proptest(strategy = "1..=5usize")] usize),
        FullscreenWindow(#[proptest(strategy = "1..=5usize")] usize),
        SetFullscreenWindow {
            #[proptest(strategy = "1..=5usize")]
            window: usize,
            is_fullscreen: bool,
        },
        FocusColumnLeft,
        FocusColumnRight,
        FocusColumnFirst,
        FocusColumnLast,
        FocusColumnRightOrFirst,
        FocusColumnLeftOrLast,
        FocusWindowOrMonitorUp(#[proptest(strategy = "1..=2u8")] u8),
        FocusWindowOrMonitorDown(#[proptest(strategy = "1..=2u8")] u8),
        FocusColumnOrMonitorLeft(#[proptest(strategy = "1..=2u8")] u8),
        FocusColumnOrMonitorRight(#[proptest(strategy = "1..=2u8")] u8),
        FocusWindowDown,
        FocusWindowUp,
        FocusWindowDownOrColumnLeft,
        FocusWindowDownOrColumnRight,
        FocusWindowUpOrColumnLeft,
        FocusWindowUpOrColumnRight,
        FocusWindowOrWorkspaceDown,
        FocusWindowOrWorkspaceUp,
        FocusWindow(#[proptest(strategy = "1..=5usize")] usize),
        MoveColumnLeft,
        MoveColumnRight,
        MoveColumnToFirst,
        MoveColumnToLast,
        MoveColumnLeftOrToMonitorLeft(#[proptest(strategy = "1..=2u8")] u8),
        MoveColumnRightOrToMonitorRight(#[proptest(strategy = "1..=2u8")] u8),
        MoveWindowDown,
        MoveWindowUp,
        MoveWindowDownOrToWorkspaceDown,
        MoveWindowUpOrToWorkspaceUp,
        ConsumeOrExpelWindowLeft {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
        },
        ConsumeOrExpelWindowRight {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
        },
        ConsumeWindowIntoColumn,
        ExpelWindowFromColumn,
        CenterColumn,
        CenterWindow {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
        },
        FocusWorkspaceDown,
        FocusWorkspaceUp,
        FocusWorkspace(#[proptest(strategy = "0..=4usize")] usize),
        FocusWorkspaceAutoBackAndForth(#[proptest(strategy = "0..=4usize")] usize),
        FocusWorkspacePrevious,
        MoveWindowToWorkspaceDown,
        MoveWindowToWorkspaceUp,
        MoveWindowToWorkspace {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            window_id: Option<usize>,
            #[proptest(strategy = "0..=4usize")]
            workspace_idx: usize,
        },
        MoveColumnToWorkspaceDown,
        MoveColumnToWorkspaceUp,
        MoveColumnToWorkspace(#[proptest(strategy = "0..=4usize")] usize),
        MoveWorkspaceDown,
        MoveWorkspaceUp,
        MoveWindowToOutput {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            window_id: Option<usize>,
            #[proptest(strategy = "1..=5usize")]
            output_id: usize,
            #[proptest(strategy = "proptest::option::of(0..=4usize)")]
            target_ws_idx: Option<usize>,
        },
        MoveColumnToOutput(#[proptest(strategy = "1..=5usize")] usize),
        SwitchPresetColumnWidth,
        SwitchPresetWindowWidth {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
        },
        SwitchPresetWindowHeight {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
        },
        MaximizeColumn,
        SetColumnWidth(#[proptest(strategy = "arbitrary_size_change()")] SizeChange),
        SetWindowWidth {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
            #[proptest(strategy = "arbitrary_size_change()")]
            change: SizeChange,
        },
        SetWindowHeight {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
            #[proptest(strategy = "arbitrary_size_change()")]
            change: SizeChange,
        },
        ResetWindowHeight {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
        },
        ToggleWindowFloating {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
        },
        SetWindowFloating {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
            floating: bool,
        },
        FocusFloating,
        FocusTiling,
        SwitchFocusFloatingTiling,
        MoveFloatingWindow {
            #[proptest(strategy = "proptest::option::of(1..=5usize)")]
            id: Option<usize>,
            #[proptest(strategy = "arbitrary_position_change()")]
            x: PositionChange,
            #[proptest(strategy = "arbitrary_position_change()")]
            y: PositionChange,
            animate: bool,
        },
        SetParent {
            #[proptest(strategy = "1..=5usize")]
            id: usize,
            #[proptest(strategy = "prop::option::of(1..=5usize)")]
            new_parent_id: Option<usize>,
        },
        Communicate(#[proptest(strategy = "1..=5usize")] usize),
        Refresh {
            is_active: bool,
        },
        AdvanceAnimations {
            #[proptest(strategy = "arbitrary_msec_delta()")]
            msec_delta: i32,
        },
        MoveWorkspaceToOutput(#[proptest(strategy = "1..=5usize")] usize),
        ViewOffsetGestureBegin {
            #[proptest(strategy = "1..=5usize")]
            output_idx: usize,
            is_touchpad: bool,
        },
        ViewOffsetGestureUpdate {
            #[proptest(strategy = "arbitrary_view_offset_gesture_delta()")]
            delta: f64,
            timestamp: Duration,
            is_touchpad: bool,
        },
        ViewOffsetGestureEnd {
            is_touchpad: Option<bool>,
        },
        WorkspaceSwitchGestureBegin {
            #[proptest(strategy = "1..=5usize")]
            output_idx: usize,
            is_touchpad: bool,
        },
        WorkspaceSwitchGestureUpdate {
            #[proptest(strategy = "-400f64..400f64")]
            delta: f64,
            timestamp: Duration,
            is_touchpad: bool,
        },
        WorkspaceSwitchGestureEnd {
            cancelled: bool,
            is_touchpad: Option<bool>,
        },
        InteractiveMoveBegin {
            #[proptest(strategy = "1..=5usize")]
            window: usize,
            #[proptest(strategy = "1..=5usize")]
            output_idx: usize,
            #[proptest(strategy = "-20000f64..20000f64")]
            px: f64,
            #[proptest(strategy = "-20000f64..20000f64")]
            py: f64,
        },
        InteractiveMoveUpdate {
            #[proptest(strategy = "1..=5usize")]
            window: usize,
            #[proptest(strategy = "-20000f64..20000f64")]
            dx: f64,
            #[proptest(strategy = "-20000f64..20000f64")]
            dy: f64,
            #[proptest(strategy = "1..=5usize")]
            output_idx: usize,
            #[proptest(strategy = "-20000f64..20000f64")]
            px: f64,
            #[proptest(strategy = "-20000f64..20000f64")]
            py: f64,
        },
        InteractiveMoveEnd {
            #[proptest(strategy = "1..=5usize")]
            window: usize,
        },
        InteractiveResizeBegin {
            #[proptest(strategy = "1..=5usize")]
            window: usize,
            #[proptest(strategy = "arbitrary_resize_edge()")]
            edges: ResizeEdge,
        },
        InteractiveResizeUpdate {
            #[proptest(strategy = "1..=5usize")]
            window: usize,
            #[proptest(strategy = "-20000f64..20000f64")]
            dx: f64,
            #[proptest(strategy = "-20000f64..20000f64")]
            dy: f64,
        },
        InteractiveResizeEnd {
            #[proptest(strategy = "1..=5usize")]
            window: usize,
        },
    }

    impl Op {
        fn apply(self, layout: &mut Layout<TestWindow>) {
            match self {
                Op::AddOutput(id) => {
                    let name = format!("output{id}");
                    if layout.outputs().any(|o| o.name() == name) {
                        return;
                    }

                    let output = Output::new(
                        name.clone(),
                        PhysicalProperties {
                            size: Size::from((1280, 720)),
                            subpixel: Subpixel::Unknown,
                            make: String::new(),
                            model: String::new(),
                        },
                    );
                    output.change_current_state(
                        Some(Mode {
                            size: Size::from((1280, 720)),
                            refresh: 60000,
                        }),
                        None,
                        None,
                        None,
                    );
                    output.user_data().insert_if_missing(|| OutputName {
                        connector: name,
                        make: None,
                        model: None,
                        serial: None,
                    });
                    layout.add_output(output.clone());
                }
                Op::AddScaledOutput { id, scale } => {
                    let name = format!("output{id}");
                    if layout.outputs().any(|o| o.name() == name) {
                        return;
                    }

                    let output = Output::new(
                        name.clone(),
                        PhysicalProperties {
                            size: Size::from((1280, 720)),
                            subpixel: Subpixel::Unknown,
                            make: String::new(),
                            model: String::new(),
                        },
                    );
                    output.change_current_state(
                        Some(Mode {
                            size: Size::from((1280, 720)),
                            refresh: 60000,
                        }),
                        None,
                        Some(smithay::output::Scale::Fractional(scale)),
                        None,
                    );
                    output.user_data().insert_if_missing(|| OutputName {
                        connector: name,
                        make: None,
                        model: None,
                        serial: None,
                    });
                    layout.add_output(output.clone());
                }
                Op::RemoveOutput(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.remove_output(&output);
                }
                Op::FocusOutput(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.focus_output(&output);
                }
                Op::AddNamedWorkspace {
                    ws_name,
                    output_name,
                } => {
                    layout.ensure_named_workspace(&WorkspaceConfig {
                        name: WorkspaceName(format!("ws{ws_name}")),
                        open_on_output: output_name.map(|name| format!("output{name}")),
                    });
                }
                Op::UnnameWorkspace { ws_name } => {
                    layout.unname_workspace(&format!("ws{ws_name}"));
                }
                Op::AddWindow { mut params } => {
                    if layout.has_window(&params.id) {
                        return;
                    }
                    if let Some(parent_id) = params.parent_id {
                        if parent_id_causes_loop(layout, params.id, parent_id) {
                            params.parent_id = None;
                        }
                    }

                    let win = TestWindow::new(params);
                    layout.add_window(
                        win,
                        AddWindowTarget::Auto,
                        None,
                        None,
                        false,
                        params.is_floating,
                        ActivateWindow::default(),
                    );
                }
                Op::AddWindowNextTo {
                    mut params,
                    next_to_id,
                } => {
                    let mut found_next_to = false;

                    if let Some(InteractiveMoveState::Moving(move_)) = &layout.interactive_move {
                        let win_id = move_.tile.window().0.id;
                        if win_id == params.id {
                            return;
                        }
                        if win_id == next_to_id {
                            found_next_to = true;
                        }
                    }

                    match &mut layout.monitor_set {
                        MonitorSet::Normal { monitors, .. } => {
                            for mon in monitors {
                                for ws in &mut mon.workspaces {
                                    for win in ws.windows() {
                                        if win.0.id == params.id {
                                            return;
                                        }

                                        if win.0.id == next_to_id {
                                            found_next_to = true;
                                        }
                                    }
                                }
                            }
                        }
                        MonitorSet::NoOutputs { workspaces, .. } => {
                            for ws in workspaces {
                                for win in ws.windows() {
                                    if win.0.id == params.id {
                                        return;
                                    }

                                    if win.0.id == next_to_id {
                                        found_next_to = true;
                                    }
                                }
                            }
                        }
                    }

                    if !found_next_to {
                        return;
                    }

                    if let Some(parent_id) = params.parent_id {
                        if parent_id_causes_loop(layout, params.id, parent_id) {
                            params.parent_id = None;
                        }
                    }

                    let win = TestWindow::new(params);
                    layout.add_window(
                        win,
                        AddWindowTarget::NextTo(&next_to_id),
                        None,
                        None,
                        false,
                        params.is_floating,
                        ActivateWindow::default(),
                    );
                }
                Op::AddWindowToNamedWorkspace {
                    mut params,
                    ws_name,
                } => {
                    let ws_name = format!("ws{ws_name}");
                    let mut ws_id = None;

                    if let Some(InteractiveMoveState::Moving(move_)) = &layout.interactive_move {
                        if move_.tile.window().0.id == params.id {
                            return;
                        }
                    }

                    match &mut layout.monitor_set {
                        MonitorSet::Normal { monitors, .. } => {
                            for mon in monitors {
                                for ws in &mut mon.workspaces {
                                    for win in ws.windows() {
                                        if win.0.id == params.id {
                                            return;
                                        }
                                    }

                                    if ws
                                        .name
                                        .as_ref()
                                        .map_or(false, |name| name.eq_ignore_ascii_case(&ws_name))
                                    {
                                        ws_id = Some(ws.id());
                                    }
                                }
                            }
                        }
                        MonitorSet::NoOutputs { workspaces, .. } => {
                            for ws in workspaces {
                                for win in ws.windows() {
                                    if win.0.id == params.id {
                                        return;
                                    }
                                }

                                if ws
                                    .name
                                    .as_ref()
                                    .map_or(false, |name| name.eq_ignore_ascii_case(&ws_name))
                                {
                                    ws_id = Some(ws.id());
                                }
                            }
                        }
                    }

                    let Some(ws_id) = ws_id else {
                        return;
                    };

                    if let Some(parent_id) = params.parent_id {
                        if parent_id_causes_loop(layout, params.id, parent_id) {
                            params.parent_id = None;
                        }
                    }

                    let win = TestWindow::new(params);
                    layout.add_window(
                        win,
                        AddWindowTarget::Workspace(ws_id),
                        None,
                        None,
                        false,
                        params.is_floating,
                        ActivateWindow::default(),
                    );
                }
                Op::CloseWindow(id) => {
                    layout.remove_window(&id, Transaction::new());
                }
                Op::FullscreenWindow(id) => {
                    layout.toggle_fullscreen(&id);
                }
                Op::SetFullscreenWindow {
                    window,
                    is_fullscreen,
                } => {
                    layout.set_fullscreen(&window, is_fullscreen);
                }
                Op::FocusColumnLeft => layout.focus_left(),
                Op::FocusColumnRight => layout.focus_right(),
                Op::FocusColumnFirst => layout.focus_column_first(),
                Op::FocusColumnLast => layout.focus_column_last(),
                Op::FocusColumnRightOrFirst => layout.focus_column_right_or_first(),
                Op::FocusColumnLeftOrLast => layout.focus_column_left_or_last(),
                Op::FocusWindowOrMonitorUp(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.focus_window_up_or_output(&output);
                }
                Op::FocusWindowOrMonitorDown(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.focus_window_down_or_output(&output);
                }
                Op::FocusColumnOrMonitorLeft(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.focus_column_left_or_output(&output);
                }
                Op::FocusColumnOrMonitorRight(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.focus_column_right_or_output(&output);
                }
                Op::FocusWindowDown => layout.focus_down(),
                Op::FocusWindowUp => layout.focus_up(),
                Op::FocusWindowDownOrColumnLeft => layout.focus_down_or_left(),
                Op::FocusWindowDownOrColumnRight => layout.focus_down_or_right(),
                Op::FocusWindowUpOrColumnLeft => layout.focus_up_or_left(),
                Op::FocusWindowUpOrColumnRight => layout.focus_up_or_right(),
                Op::FocusWindowOrWorkspaceDown => layout.focus_window_or_workspace_down(),
                Op::FocusWindowOrWorkspaceUp => layout.focus_window_or_workspace_up(),
                Op::FocusWindow(id) => layout.activate_window(&id),
                Op::MoveColumnLeft => layout.move_left(),
                Op::MoveColumnRight => layout.move_right(),
                Op::MoveColumnToFirst => layout.move_column_to_first(),
                Op::MoveColumnToLast => layout.move_column_to_last(),
                Op::MoveColumnLeftOrToMonitorLeft(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.move_column_left_or_to_output(&output);
                }
                Op::MoveColumnRightOrToMonitorRight(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.move_column_right_or_to_output(&output);
                }
                Op::MoveWindowDown => layout.move_down(),
                Op::MoveWindowUp => layout.move_up(),
                Op::MoveWindowDownOrToWorkspaceDown => layout.move_down_or_to_workspace_down(),
                Op::MoveWindowUpOrToWorkspaceUp => layout.move_up_or_to_workspace_up(),
                Op::ConsumeOrExpelWindowLeft { id } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.consume_or_expel_window_left(id.as_ref());
                }
                Op::ConsumeOrExpelWindowRight { id } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.consume_or_expel_window_right(id.as_ref());
                }
                Op::ConsumeWindowIntoColumn => layout.consume_into_column(),
                Op::ExpelWindowFromColumn => layout.expel_from_column(),
                Op::CenterColumn => layout.center_column(),
                Op::CenterWindow { id } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.center_window(id.as_ref());
                }
                Op::FocusWorkspaceDown => layout.switch_workspace_down(),
                Op::FocusWorkspaceUp => layout.switch_workspace_up(),
                Op::FocusWorkspace(idx) => layout.switch_workspace(idx),
                Op::FocusWorkspaceAutoBackAndForth(idx) => {
                    layout.switch_workspace_auto_back_and_forth(idx)
                }
                Op::FocusWorkspacePrevious => layout.switch_workspace_previous(),
                Op::MoveWindowToWorkspaceDown => layout.move_to_workspace_down(),
                Op::MoveWindowToWorkspaceUp => layout.move_to_workspace_up(),
                Op::MoveWindowToWorkspace {
                    window_id,
                    workspace_idx,
                } => {
                    let window_id = window_id.filter(|id| layout.has_window(id));
                    layout.move_to_workspace(window_id.as_ref(), workspace_idx);
                }
                Op::MoveColumnToWorkspaceDown => layout.move_column_to_workspace_down(),
                Op::MoveColumnToWorkspaceUp => layout.move_column_to_workspace_up(),
                Op::MoveColumnToWorkspace(idx) => layout.move_column_to_workspace(idx),
                Op::MoveWindowToOutput {
                    window_id,
                    output_id: id,
                    target_ws_idx,
                } => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };
                    let mon = layout.monitor_for_output(&output).unwrap();

                    let window_id = window_id.filter(|id| layout.has_window(id));
                    let target_ws_idx = target_ws_idx.filter(|idx| mon.workspaces.len() > *idx);
                    layout.move_to_output(window_id.as_ref(), &output, target_ws_idx);
                }
                Op::MoveColumnToOutput(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.move_column_to_output(&output);
                }
                Op::MoveWorkspaceDown => layout.move_workspace_down(),
                Op::MoveWorkspaceUp => layout.move_workspace_up(),
                Op::SwitchPresetColumnWidth => layout.toggle_width(),
                Op::SwitchPresetWindowWidth { id } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.toggle_window_width(id.as_ref());
                }
                Op::SwitchPresetWindowHeight { id } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.toggle_window_height(id.as_ref());
                }
                Op::MaximizeColumn => layout.toggle_full_width(),
                Op::SetColumnWidth(change) => layout.set_column_width(change),
                Op::SetWindowWidth { id, change } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.set_window_width(id.as_ref(), change);
                }
                Op::SetWindowHeight { id, change } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.set_window_height(id.as_ref(), change);
                }
                Op::ResetWindowHeight { id } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.reset_window_height(id.as_ref());
                }
                Op::ToggleWindowFloating { id } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.toggle_window_floating(id.as_ref());
                }
                Op::SetWindowFloating { id, floating } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.set_window_floating(id.as_ref(), floating);
                }
                Op::FocusFloating => {
                    layout.focus_floating();
                }
                Op::FocusTiling => {
                    layout.focus_tiling();
                }
                Op::SwitchFocusFloatingTiling => {
                    layout.switch_focus_floating_tiling();
                }
                Op::MoveFloatingWindow { id, x, y, animate } => {
                    let id = id.filter(|id| layout.has_window(id));
                    layout.move_floating_window(id.as_ref(), x, y, animate);
                }
                Op::SetParent {
                    id,
                    mut new_parent_id,
                } => {
                    if !layout.has_window(&id) {
                        return;
                    }

                    if let Some(parent_id) = new_parent_id {
                        if parent_id_causes_loop(layout, id, parent_id) {
                            new_parent_id = None;
                        }
                    }

                    let mut update = false;

                    if let Some(InteractiveMoveState::Moving(move_)) = &layout.interactive_move {
                        if move_.tile.window().0.id == id {
                            move_.tile.window().0.parent_id.set(new_parent_id);
                            update = true;
                        }
                    }

                    match &mut layout.monitor_set {
                        MonitorSet::Normal { monitors, .. } => {
                            'outer: for mon in monitors {
                                for ws in &mut mon.workspaces {
                                    for win in ws.windows() {
                                        if win.0.id == id {
                                            win.0.parent_id.set(new_parent_id);
                                            update = true;
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }
                        MonitorSet::NoOutputs { workspaces, .. } => {
                            'outer: for ws in workspaces {
                                for win in ws.windows() {
                                    if win.0.id == id {
                                        win.0.parent_id.set(new_parent_id);
                                        update = true;
                                        break 'outer;
                                    }
                                }
                            }
                        }
                    }

                    if update {
                        if let Some(new_parent_id) = new_parent_id {
                            layout.descendants_added(&new_parent_id);
                        }
                    }
                }
                Op::Communicate(id) => {
                    let mut update = false;

                    if let Some(InteractiveMoveState::Moving(move_)) = &layout.interactive_move {
                        if move_.tile.window().0.id == id {
                            if move_.tile.window().communicate() {
                                update = true;
                            }

                            if update {
                                // FIXME: serial.
                                layout.update_window(&id, None);
                            }
                            return;
                        }
                    }

                    match &mut layout.monitor_set {
                        MonitorSet::Normal { monitors, .. } => {
                            'outer: for mon in monitors {
                                for ws in &mut mon.workspaces {
                                    for win in ws.windows() {
                                        if win.0.id == id {
                                            if win.communicate() {
                                                update = true;
                                            }
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }
                        MonitorSet::NoOutputs { workspaces, .. } => {
                            'outer: for ws in workspaces {
                                for win in ws.windows() {
                                    if win.0.id == id {
                                        if win.communicate() {
                                            update = true;
                                        }
                                        break 'outer;
                                    }
                                }
                            }
                        }
                    }

                    if update {
                        // FIXME: serial.
                        layout.update_window(&id, None);
                    }
                }
                Op::Refresh { is_active } => {
                    layout.refresh(is_active);
                }
                Op::AdvanceAnimations { msec_delta } => {
                    let mut now = layout.clock.now_unadjusted();
                    if msec_delta >= 0 {
                        now = now.saturating_add(Duration::from_millis(msec_delta as u64));
                    } else {
                        now = now.saturating_sub(Duration::from_millis(-msec_delta as u64));
                    }
                    layout.clock.set_unadjusted(now);
                    layout.advance_animations();
                }
                Op::MoveWorkspaceToOutput(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.move_workspace_to_output(&output);
                }
                Op::ViewOffsetGestureBegin {
                    output_idx: id,
                    is_touchpad: normalize,
                } => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.view_offset_gesture_begin(&output, normalize);
                }
                Op::ViewOffsetGestureUpdate {
                    delta,
                    timestamp,
                    is_touchpad,
                } => {
                    layout.view_offset_gesture_update(delta, timestamp, is_touchpad);
                }
                Op::ViewOffsetGestureEnd { is_touchpad } => {
                    // We don't handle cancels in this gesture.
                    layout.view_offset_gesture_end(false, is_touchpad);
                }
                Op::WorkspaceSwitchGestureBegin {
                    output_idx: id,
                    is_touchpad,
                } => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.workspace_switch_gesture_begin(&output, is_touchpad);
                }
                Op::WorkspaceSwitchGestureUpdate {
                    delta,
                    timestamp,
                    is_touchpad,
                } => {
                    layout.workspace_switch_gesture_update(delta, timestamp, is_touchpad);
                }
                Op::WorkspaceSwitchGestureEnd {
                    cancelled,
                    is_touchpad,
                } => {
                    layout.workspace_switch_gesture_end(cancelled, is_touchpad);
                }
                Op::InteractiveMoveBegin {
                    window,
                    output_idx,
                    px,
                    py,
                } => {
                    let name = format!("output{output_idx}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };
                    layout.interactive_move_begin(window, &output, Point::from((px, py)));
                }
                Op::InteractiveMoveUpdate {
                    window,
                    dx,
                    dy,
                    output_idx,
                    px,
                    py,
                } => {
                    let name = format!("output{output_idx}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };
                    layout.interactive_move_update(
                        &window,
                        Point::from((dx, dy)),
                        output,
                        Point::from((px, py)),
                    );
                }
                Op::InteractiveMoveEnd { window } => {
                    layout.interactive_move_end(&window);
                }
                Op::InteractiveResizeBegin { window, edges } => {
                    layout.interactive_resize_begin(window, edges);
                }
                Op::InteractiveResizeUpdate { window, dx, dy } => {
                    layout.interactive_resize_update(&window, Point::from((dx, dy)));
                }
                Op::InteractiveResizeEnd { window } => {
                    layout.interactive_resize_end(&window);
                }
            }
        }
    }

    #[track_caller]
    fn check_ops(ops: &[Op]) -> Layout<TestWindow> {
        let mut layout = Layout::default();
        for op in ops {
            op.apply(&mut layout);
            layout.verify_invariants();
        }
        layout
    }

    #[track_caller]
    fn check_ops_with_options(options: Options, ops: &[Op]) -> Layout<TestWindow> {
        let mut layout = Layout::with_options(Clock::with_time(Duration::ZERO), options);

        for op in ops {
            op.apply(&mut layout);
            layout.verify_invariants();
        }

        layout
    }

    #[test]
    fn operations_dont_panic() {
        let every_op = [
            Op::AddOutput(0),
            Op::AddOutput(1),
            Op::AddOutput(2),
            Op::RemoveOutput(0),
            Op::RemoveOutput(1),
            Op::RemoveOutput(2),
            Op::FocusOutput(0),
            Op::FocusOutput(1),
            Op::FocusOutput(2),
            Op::AddNamedWorkspace {
                ws_name: 1,
                output_name: Some(1),
            },
            Op::UnnameWorkspace { ws_name: 1 },
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddWindowNextTo {
                params: TestWindowParams::new(2),
                next_to_id: 1,
            },
            Op::AddWindowToNamedWorkspace {
                params: TestWindowParams::new(3),
                ws_name: 1,
            },
            Op::CloseWindow(0),
            Op::CloseWindow(1),
            Op::CloseWindow(2),
            Op::FullscreenWindow(1),
            Op::FullscreenWindow(2),
            Op::FullscreenWindow(3),
            Op::FocusColumnLeft,
            Op::FocusColumnRight,
            Op::FocusColumnRightOrFirst,
            Op::FocusColumnLeftOrLast,
            Op::FocusWindowOrMonitorUp(0),
            Op::FocusWindowOrMonitorDown(1),
            Op::FocusColumnOrMonitorLeft(0),
            Op::FocusColumnOrMonitorRight(1),
            Op::FocusWindowUp,
            Op::FocusWindowUpOrColumnLeft,
            Op::FocusWindowUpOrColumnRight,
            Op::FocusWindowOrWorkspaceUp,
            Op::FocusWindowDown,
            Op::FocusWindowDownOrColumnLeft,
            Op::FocusWindowDownOrColumnRight,
            Op::FocusWindowOrWorkspaceDown,
            Op::MoveColumnLeft,
            Op::MoveColumnRight,
            Op::MoveColumnLeftOrToMonitorLeft(0),
            Op::MoveColumnRightOrToMonitorRight(1),
            Op::ConsumeWindowIntoColumn,
            Op::ExpelWindowFromColumn,
            Op::CenterColumn,
            Op::FocusWorkspaceDown,
            Op::FocusWorkspaceUp,
            Op::FocusWorkspace(1),
            Op::FocusWorkspace(2),
            Op::MoveWindowToWorkspaceDown,
            Op::MoveWindowToWorkspaceUp,
            Op::MoveWindowToWorkspace {
                window_id: None,
                workspace_idx: 1,
            },
            Op::MoveWindowToWorkspace {
                window_id: None,
                workspace_idx: 2,
            },
            Op::MoveColumnToWorkspaceDown,
            Op::MoveColumnToWorkspaceUp,
            Op::MoveColumnToWorkspace(1),
            Op::MoveColumnToWorkspace(2),
            Op::MoveWindowDown,
            Op::MoveWindowDownOrToWorkspaceDown,
            Op::MoveWindowUp,
            Op::MoveWindowUpOrToWorkspaceUp,
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::ConsumeOrExpelWindowRight { id: None },
            Op::MoveWorkspaceToOutput(1),
        ];

        for third in every_op {
            for second in every_op {
                for first in every_op {
                    // eprintln!("{first:?}, {second:?}, {third:?}");

                    let mut layout = Layout::default();
                    first.apply(&mut layout);
                    layout.verify_invariants();
                    second.apply(&mut layout);
                    layout.verify_invariants();
                    third.apply(&mut layout);
                    layout.verify_invariants();
                }
            }
        }
    }

    #[test]
    fn operations_from_starting_state_dont_panic() {
        if std::env::var_os("RUN_SLOW_TESTS").is_none() {
            eprintln!("ignoring slow test");
            return;
        }

        // Running every op from an empty state doesn't get us to all the interesting states. So,
        // also run it from a manually-created starting state with more things going on to exercise
        // more code paths.
        let setup_ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::MoveWindowToWorkspaceDown,
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::AddWindow {
                params: TestWindowParams::new(3),
            },
            Op::FocusColumnLeft,
            Op::ConsumeWindowIntoColumn,
            Op::AddWindow {
                params: TestWindowParams::new(4),
            },
            Op::AddOutput(2),
            Op::AddWindow {
                params: TestWindowParams::new(5),
            },
            Op::MoveWindowToOutput {
                window_id: None,
                output_id: 2,
                target_ws_idx: None,
            },
            Op::FocusOutput(1),
            Op::Communicate(1),
            Op::Communicate(2),
            Op::Communicate(3),
            Op::Communicate(4),
            Op::Communicate(5),
        ];

        let every_op = [
            Op::AddOutput(0),
            Op::AddOutput(1),
            Op::AddOutput(2),
            Op::RemoveOutput(0),
            Op::RemoveOutput(1),
            Op::RemoveOutput(2),
            Op::FocusOutput(0),
            Op::FocusOutput(1),
            Op::FocusOutput(2),
            Op::AddNamedWorkspace {
                ws_name: 1,
                output_name: Some(1),
            },
            Op::UnnameWorkspace { ws_name: 1 },
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::AddWindowNextTo {
                params: TestWindowParams::new(6),
                next_to_id: 0,
            },
            Op::AddWindowNextTo {
                params: TestWindowParams::new(7),
                next_to_id: 1,
            },
            Op::AddWindowToNamedWorkspace {
                params: TestWindowParams::new(5),
                ws_name: 1,
            },
            Op::CloseWindow(0),
            Op::CloseWindow(1),
            Op::CloseWindow(2),
            Op::FullscreenWindow(1),
            Op::FullscreenWindow(2),
            Op::FullscreenWindow(3),
            Op::SetFullscreenWindow {
                window: 1,
                is_fullscreen: false,
            },
            Op::SetFullscreenWindow {
                window: 1,
                is_fullscreen: true,
            },
            Op::SetFullscreenWindow {
                window: 2,
                is_fullscreen: false,
            },
            Op::SetFullscreenWindow {
                window: 2,
                is_fullscreen: true,
            },
            Op::FocusColumnLeft,
            Op::FocusColumnRight,
            Op::FocusColumnRightOrFirst,
            Op::FocusColumnLeftOrLast,
            Op::FocusWindowOrMonitorUp(0),
            Op::FocusWindowOrMonitorDown(1),
            Op::FocusColumnOrMonitorLeft(0),
            Op::FocusColumnOrMonitorRight(1),
            Op::FocusWindowUp,
            Op::FocusWindowUpOrColumnLeft,
            Op::FocusWindowUpOrColumnRight,
            Op::FocusWindowOrWorkspaceUp,
            Op::FocusWindowDown,
            Op::FocusWindowDownOrColumnLeft,
            Op::FocusWindowDownOrColumnRight,
            Op::FocusWindowOrWorkspaceDown,
            Op::MoveColumnLeft,
            Op::MoveColumnRight,
            Op::MoveColumnLeftOrToMonitorLeft(0),
            Op::MoveColumnRightOrToMonitorRight(1),
            Op::ConsumeWindowIntoColumn,
            Op::ExpelWindowFromColumn,
            Op::CenterColumn,
            Op::FocusWorkspaceDown,
            Op::FocusWorkspaceUp,
            Op::FocusWorkspace(1),
            Op::FocusWorkspace(2),
            Op::FocusWorkspace(3),
            Op::MoveWindowToWorkspaceDown,
            Op::MoveWindowToWorkspaceUp,
            Op::MoveWindowToWorkspace {
                window_id: None,
                workspace_idx: 1,
            },
            Op::MoveWindowToWorkspace {
                window_id: None,
                workspace_idx: 2,
            },
            Op::MoveWindowToWorkspace {
                window_id: None,
                workspace_idx: 3,
            },
            Op::MoveColumnToWorkspaceDown,
            Op::MoveColumnToWorkspaceUp,
            Op::MoveColumnToWorkspace(1),
            Op::MoveColumnToWorkspace(2),
            Op::MoveColumnToWorkspace(3),
            Op::MoveWindowDown,
            Op::MoveWindowDownOrToWorkspaceDown,
            Op::MoveWindowUp,
            Op::MoveWindowUpOrToWorkspaceUp,
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::ConsumeOrExpelWindowRight { id: None },
        ];

        for third in every_op {
            for second in every_op {
                for first in every_op {
                    // eprintln!("{first:?}, {second:?}, {third:?}");

                    let mut layout = Layout::default();
                    for op in setup_ops {
                        op.apply(&mut layout);
                    }

                    first.apply(&mut layout);
                    layout.verify_invariants();
                    second.apply(&mut layout);
                    layout.verify_invariants();
                    third.apply(&mut layout);
                    layout.verify_invariants();
                }
            }
        }
    }

    #[test]
    fn primary_active_workspace_idx_not_updated_on_output_add() {
        let ops = [
            Op::AddOutput(1),
            Op::AddOutput(2),
            Op::FocusOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::FocusOutput(2),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::RemoveOutput(2),
            Op::FocusWorkspace(3),
            Op::AddOutput(2),
        ];

        check_ops(&ops);
    }

    #[test]
    fn window_closed_on_previous_workspace() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::FocusWorkspaceDown,
            Op::CloseWindow(0),
        ];

        check_ops(&ops);
    }

    #[test]
    fn removing_output_must_keep_empty_focus_on_primary() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddOutput(2),
            Op::RemoveOutput(1),
        ];

        let layout = check_ops(&ops);

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        // The workspace from the removed output was inserted at position 0, so the active workspace
        // must change to 1 to keep the focus on the empty workspace.
        assert_eq!(monitors[0].active_workspace_idx, 1);
    }

    #[test]
    fn move_to_workspace_by_idx_does_not_leave_empty_workspaces() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddOutput(2),
            Op::FocusOutput(2),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::RemoveOutput(1),
            Op::MoveWindowToWorkspace {
                window_id: Some(0),
                workspace_idx: 2,
            },
        ];

        let layout = check_ops(&ops);

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        assert!(monitors[0].workspaces[1].has_windows());
    }

    #[test]
    fn empty_workspaces_dont_move_back_to_original_output() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::AddOutput(2),
            Op::RemoveOutput(1),
            Op::FocusWorkspace(1),
            Op::CloseWindow(1),
            Op::AddOutput(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn large_negative_height_change() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::AdjustProportion(-1e129),
            },
        ];

        let mut options = Options::default();
        options.border.off = false;
        options.border.width = FloatOrInt(1.);

        check_ops_with_options(options, &ops);
    }

    #[test]
    fn large_max_size() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams {
                    min_max_size: (Size::from((0, 0)), Size::from((i32::MAX, i32::MAX))),
                    ..TestWindowParams::new(1)
                },
            },
        ];

        let mut options = Options::default();
        options.border.off = false;
        options.border.width = FloatOrInt(1.);

        check_ops_with_options(options, &ops);
    }

    #[test]
    fn workspace_cleanup_during_switch() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FocusWorkspaceDown,
            Op::CloseWindow(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn workspace_transfer_during_switch() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddOutput(2),
            Op::FocusOutput(2),
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::RemoveOutput(1),
            Op::FocusWorkspaceDown,
            Op::FocusWorkspaceDown,
            Op::AddOutput(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn workspace_transfer_during_switch_from_last() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddOutput(2),
            Op::RemoveOutput(1),
            Op::FocusWorkspaceUp,
            Op::AddOutput(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn workspace_transfer_during_switch_gets_cleaned_up() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::RemoveOutput(1),
            Op::AddOutput(2),
            Op::MoveColumnToWorkspaceDown,
            Op::MoveColumnToWorkspaceDown,
            Op::AddOutput(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn move_workspace_to_output() {
        let ops = [
            Op::AddOutput(1),
            Op::AddOutput(2),
            Op::FocusOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::MoveWorkspaceToOutput(2),
        ];

        let layout = check_ops(&ops);

        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = layout.monitor_set
        else {
            unreachable!()
        };

        assert_eq!(active_monitor_idx, 1);
        assert_eq!(monitors[0].workspaces.len(), 1);
        assert!(!monitors[0].workspaces[0].has_windows());
        assert_eq!(monitors[1].active_workspace_idx, 0);
        assert_eq!(monitors[1].workspaces.len(), 2);
        assert!(monitors[1].workspaces[0].has_windows());
    }

    #[test]
    fn fullscreen() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FullscreenWindow(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn unfullscreen_window_in_column() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::SetFullscreenWindow {
                window: 2,
                is_fullscreen: false,
            },
        ];

        check_ops(&ops);
    }

    #[test]
    fn open_right_of_on_different_workspace() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::AddWindowNextTo {
                params: TestWindowParams::new(3),
                next_to_id: 1,
            },
        ];

        let layout = check_ops(&ops);

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        let mon = monitors.into_iter().next().unwrap();
        assert_eq!(
            mon.active_workspace_idx, 1,
            "the second workspace must remain active"
        );
        assert_eq!(
            mon.workspaces[0].scrolling().active_column_idx(),
            1,
            "the new window must become active"
        );
    }

    #[test]
    // empty_workspace_above_first = true
    fn open_right_of_on_different_workspace_ewaf() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::AddWindowNextTo {
                params: TestWindowParams::new(3),
                next_to_id: 1,
            },
        ];

        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        let layout = check_ops_with_options(options, &ops);

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        let mon = monitors.into_iter().next().unwrap();
        assert_eq!(
            mon.active_workspace_idx, 2,
            "the second workspace must remain active"
        );
        assert_eq!(
            mon.workspaces[1].scrolling().active_column_idx(),
            1,
            "the new window must become active"
        );
    }

    #[test]
    fn unfullscreen_view_offset_not_reset_on_removal() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::FullscreenWindow(0),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::ConsumeOrExpelWindowRight { id: None },
        ];

        check_ops(&ops);
    }

    #[test]
    fn unfullscreen_view_offset_not_reset_on_consume() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::FullscreenWindow(0),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::ConsumeWindowIntoColumn,
        ];

        check_ops(&ops);
    }

    #[test]
    fn unfullscreen_view_offset_not_reset_on_quick_double_toggle() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::FullscreenWindow(0),
            Op::FullscreenWindow(0),
        ];

        check_ops(&ops);
    }

    #[test]
    fn unfullscreen_view_offset_set_on_fullscreening_inactive_tile_in_column() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::FullscreenWindow(0),
        ];

        check_ops(&ops);
    }

    #[test]
    fn unfullscreen_view_offset_not_reset_on_gesture() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FullscreenWindow(1),
            Op::ViewOffsetGestureBegin {
                output_idx: 1,
                is_touchpad: true,
            },
            Op::ViewOffsetGestureEnd {
                is_touchpad: Some(true),
            },
        ];

        check_ops(&ops);
    }

    #[test]
    fn removing_all_outputs_preserves_empty_named_workspaces() {
        let ops = [
            Op::AddOutput(1),
            Op::AddNamedWorkspace {
                ws_name: 1,
                output_name: None,
            },
            Op::AddNamedWorkspace {
                ws_name: 2,
                output_name: None,
            },
            Op::RemoveOutput(1),
        ];

        let layout = check_ops(&ops);

        let MonitorSet::NoOutputs { workspaces } = layout.monitor_set else {
            unreachable!()
        };

        assert_eq!(workspaces.len(), 2);
    }

    #[test]
    fn config_change_updates_cached_sizes() {
        let mut config = Config::default();
        config.layout.border.off = false;
        config.layout.border.width = FloatOrInt(2.);

        let mut layout = Layout::new(Clock::default(), &config);

        Op::AddWindow {
            params: TestWindowParams {
                bbox: Rectangle::from_loc_and_size((0, 0), (1280, 200)),
                ..TestWindowParams::new(1)
            },
        }
        .apply(&mut layout);

        config.layout.border.width = FloatOrInt(4.);
        layout.update_config(&config);

        layout.verify_invariants();
    }

    #[test]
    fn preset_height_change_removes_preset() {
        let mut config = Config::default();
        config.layout.preset_window_heights = vec![PresetSize::Fixed(1), PresetSize::Fixed(2)];

        let mut layout = Layout::new(Clock::default(), &config);

        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::SwitchPresetWindowHeight { id: None },
            Op::SwitchPresetWindowHeight { id: None },
        ];
        for op in ops {
            op.apply(&mut layout);
        }

        // Leave only one.
        config.layout.preset_window_heights = vec![PresetSize::Fixed(1)];

        layout.update_config(&config);

        layout.verify_invariants();
    }

    #[test]
    fn set_window_height_recomputes_to_auto() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::SetFixed(100),
            },
            Op::FocusWindowUp,
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::SetFixed(200),
            },
        ];

        check_ops(&ops);
    }

    #[test]
    fn one_window_in_column_becomes_weight_1() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::SetFixed(100),
            },
            Op::Communicate(2),
            Op::FocusWindowUp,
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::SetFixed(200),
            },
            Op::Communicate(1),
            Op::CloseWindow(0),
            Op::CloseWindow(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn one_window_in_column_becomes_weight_1_after_fullscreen() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::SetFixed(100),
            },
            Op::Communicate(2),
            Op::FocusWindowUp,
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::SetFixed(200),
            },
            Op::Communicate(1),
            Op::CloseWindow(0),
            Op::FullscreenWindow(1),
        ];

        check_ops(&ops);
    }

    #[test]
    fn fixed_height_takes_max_non_auto_into_account() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::SetWindowHeight {
                id: Some(0),
                change: SizeChange::SetFixed(704),
            },
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::ConsumeOrExpelWindowLeft { id: None },
        ];

        let options = Options {
            border: niri_config::Border {
                off: false,
                width: niri_config::FloatOrInt(4.),
                ..Default::default()
            },
            gaps: 0.,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn start_interactive_move_then_remove_window() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::InteractiveMoveBegin {
                window: 0,
                output_idx: 1,
                px: 0.,
                py: 0.,
            },
            Op::CloseWindow(0),
        ];

        check_ops(&ops);
    }

    #[test]
    fn interactive_move_onto_empty_output() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::InteractiveMoveBegin {
                window: 0,
                output_idx: 1,
                px: 0.,
                py: 0.,
            },
            Op::AddOutput(2),
            Op::InteractiveMoveUpdate {
                window: 0,
                dx: 1000.,
                dy: 0.,
                output_idx: 2,
                px: 0.,
                py: 0.,
            },
            Op::InteractiveMoveEnd { window: 0 },
        ];

        check_ops(&ops);
    }

    #[test]
    fn interactive_move_onto_empty_output_ewaf() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::InteractiveMoveBegin {
                window: 0,
                output_idx: 1,
                px: 0.,
                py: 0.,
            },
            Op::AddOutput(2),
            Op::InteractiveMoveUpdate {
                window: 0,
                dx: 1000.,
                dy: 0.,
                output_idx: 2,
                px: 0.,
                py: 0.,
            },
            Op::InteractiveMoveEnd { window: 0 },
        ];

        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn interactive_move_onto_last_workspace() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::InteractiveMoveBegin {
                window: 0,
                output_idx: 1,
                px: 0.,
                py: 0.,
            },
            Op::InteractiveMoveUpdate {
                window: 0,
                dx: 1000.,
                dy: 0.,
                output_idx: 1,
                px: 0.,
                py: 0.,
            },
            Op::FocusWorkspaceDown,
            Op::AdvanceAnimations { msec_delta: 1000 },
            Op::InteractiveMoveEnd { window: 0 },
        ];

        check_ops(&ops);
    }

    #[test]
    fn interactive_move_onto_first_empty_workspace() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::InteractiveMoveBegin {
                window: 1,
                output_idx: 1,
                px: 0.,
                py: 0.,
            },
            Op::InteractiveMoveUpdate {
                window: 1,
                dx: 1000.,
                dy: 0.,
                output_idx: 1,
                px: 0.,
                py: 0.,
            },
            Op::FocusWorkspaceUp,
            Op::AdvanceAnimations { msec_delta: 1000 },
            Op::InteractiveMoveEnd { window: 1 },
        ];
        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn output_active_workspace_is_preserved() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::RemoveOutput(1),
            Op::AddOutput(1),
        ];

        let layout = check_ops(&ops);

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        assert_eq!(monitors[0].active_workspace_idx, 1);
    }

    #[test]
    fn output_active_workspace_is_preserved_with_other_outputs() {
        let ops = [
            Op::AddOutput(1),
            Op::AddOutput(2),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                params: TestWindowParams::new(2),
            },
            Op::RemoveOutput(1),
            Op::AddOutput(1),
        ];

        let layout = check_ops(&ops);

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        assert_eq!(monitors[1].active_workspace_idx, 1);
    }

    #[test]
    fn named_workspace_to_output() {
        let ops = [
            Op::AddNamedWorkspace {
                ws_name: 1,
                output_name: None,
            },
            Op::AddOutput(1),
            Op::MoveWorkspaceToOutput(1),
            Op::FocusWorkspaceUp,
        ];
        check_ops(&ops);
    }

    #[test]
    // empty_workspace_above_first = true
    fn named_workspace_to_output_ewaf() {
        let ops = [
            Op::AddNamedWorkspace {
                ws_name: 1,
                output_name: Some(2),
            },
            Op::AddOutput(1),
            Op::AddOutput(2),
        ];
        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn move_window_to_empty_workspace_above_first() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::MoveWorkspaceUp,
            Op::MoveWorkspaceDown,
            Op::FocusWorkspaceUp,
            Op::MoveWorkspaceDown,
        ];
        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn move_window_to_different_output() {
        let ops = [
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddOutput(1),
            Op::AddOutput(2),
            Op::MoveWorkspaceToOutput(2),
        ];
        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn close_window_empty_ws_above_first() {
        let ops = [
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::AddOutput(1),
            Op::CloseWindow(1),
        ];
        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn add_and_remove_output() {
        let ops = [
            Op::AddOutput(2),
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
            Op::RemoveOutput(2),
        ];
        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        check_ops_with_options(options, &ops);
    }

    #[test]
    fn switch_ewaf_on() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
        ];

        let mut layout = check_ops(&ops);
        layout.update_options(Options {
            empty_workspace_above_first: true,
            ..Default::default()
        });
        layout.verify_invariants();
    }

    #[test]
    fn switch_ewaf_off() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(1),
            },
        ];

        let options = Options {
            empty_workspace_above_first: true,
            ..Default::default()
        };
        let mut layout = check_ops_with_options(options, &ops);
        layout.update_options(Options::default());
        layout.verify_invariants();
    }

    #[test]
    fn interactive_move_drop_on_other_output_during_animation() {
        let ops = [
            Op::AddOutput(3),
            Op::AddWindow {
                params: TestWindowParams::new(3),
            },
            Op::InteractiveMoveBegin {
                window: 3,
                output_idx: 3,
                px: 0.0,
                py: 0.0,
            },
            Op::FocusWorkspaceDown,
            Op::AddOutput(4),
            Op::InteractiveMoveUpdate {
                window: 3,
                dx: 0.0,
                dy: 8300.68619826683,
                output_idx: 4,
                px: 0.0,
                py: 0.0,
            },
            Op::RemoveOutput(4),
            Op::InteractiveMoveEnd { window: 3 },
        ];
        check_ops(&ops);
    }

    #[test]
    fn set_width_fixed_negative() {
        let ops = [
            Op::AddOutput(3),
            Op::AddWindow {
                params: TestWindowParams::new(3),
            },
            Op::ToggleWindowFloating { id: Some(3) },
            Op::SetColumnWidth(SizeChange::SetFixed(-100)),
        ];
        check_ops(&ops);
    }

    #[test]
    fn set_height_fixed_negative() {
        let ops = [
            Op::AddOutput(3),
            Op::AddWindow {
                params: TestWindowParams::new(3),
            },
            Op::ToggleWindowFloating { id: Some(3) },
            Op::SetWindowHeight {
                id: None,
                change: SizeChange::SetFixed(-100),
            },
        ];
        check_ops(&ops);
    }

    #[test]
    fn interactive_resize_to_negative() {
        let ops = [
            Op::AddOutput(3),
            Op::AddWindow {
                params: TestWindowParams::new(3),
            },
            Op::ToggleWindowFloating { id: Some(3) },
            Op::InteractiveResizeBegin {
                window: 3,
                edges: ResizeEdge::BOTTOM_RIGHT,
            },
            Op::InteractiveResizeUpdate {
                window: 3,
                dx: -10000.,
                dy: -10000.,
            },
        ];
        check_ops(&ops);
    }

    #[test]
    fn windows_on_other_workspaces_remain_activated() {
        let ops = [
            Op::AddOutput(3),
            Op::AddWindow {
                params: TestWindowParams::new(3),
            },
            Op::FocusWorkspaceDown,
            Op::Refresh { is_active: true },
        ];

        let layout = check_ops(&ops);
        let (_, win) = layout.windows().next().unwrap();
        assert!(win.0.pending_activated.get());
    }

    #[test]
    fn stacking_add_parent_brings_up_child() {
        let ops = [
            Op::AddOutput(0),
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    parent_id: Some(1),
                    ..TestWindowParams::new(0)
                },
            },
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    ..TestWindowParams::new(1)
                },
            },
        ];

        check_ops(&ops);
    }

    #[test]
    fn stacking_add_parent_brings_up_descendants() {
        let ops = [
            Op::AddOutput(0),
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    parent_id: Some(2),
                    ..TestWindowParams::new(0)
                },
            },
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    parent_id: Some(0),
                    ..TestWindowParams::new(1)
                },
            },
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    ..TestWindowParams::new(2)
                },
            },
        ];

        check_ops(&ops);
    }

    #[test]
    fn stacking_activate_brings_up_descendants() {
        let ops = [
            Op::AddOutput(0),
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    ..TestWindowParams::new(0)
                },
            },
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    parent_id: Some(0),
                    ..TestWindowParams::new(1)
                },
            },
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    parent_id: Some(1),
                    ..TestWindowParams::new(2)
                },
            },
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    ..TestWindowParams::new(3)
                },
            },
            Op::FocusWindow(0),
        ];

        check_ops(&ops);
    }

    #[test]
    fn stacking_set_parent_brings_up_child() {
        let ops = [
            Op::AddOutput(0),
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    ..TestWindowParams::new(0)
                },
            },
            Op::AddWindow {
                params: TestWindowParams {
                    is_floating: true,
                    ..TestWindowParams::new(1)
                },
            },
            Op::SetParent {
                id: 0,
                new_parent_id: Some(1),
            },
        ];

        check_ops(&ops);
    }

    #[test]
    fn move_window_to_workspace_with_different_active_output() {
        let ops = [
            Op::AddOutput(0),
            Op::AddOutput(1),
            Op::AddWindow {
                params: TestWindowParams::new(0),
            },
            Op::FocusOutput(1),
            Op::MoveWindowToWorkspace {
                window_id: Some(0),
                workspace_idx: 2,
            },
        ];

        check_ops(&ops);
    }

    fn parent_id_causes_loop(layout: &Layout<TestWindow>, id: usize, mut parent_id: usize) -> bool {
        if parent_id == id {
            return true;
        }

        'outer: loop {
            for (_, win) in layout.windows() {
                if win.0.id == parent_id {
                    match win.0.parent_id.get() {
                        Some(new_parent_id) => {
                            if new_parent_id == id {
                                // Found a loop.
                                return true;
                            }

                            parent_id = new_parent_id;
                            continue 'outer;
                        }
                        // Reached window with no parent.
                        None => return false,
                    }
                }
            }

            // Parent is not in the layout.
            return false;
        }
    }

    fn arbitrary_spacing() -> impl Strategy<Value = f64> {
        // Give equal weight to:
        // - 0: the element is disabled
        // - 4: some reasonable value
        // - random value, likely unreasonably big
        prop_oneof![Just(0.), Just(4.), ((1.)..=65535.)]
    }

    fn arbitrary_spacing_neg() -> impl Strategy<Value = f64> {
        // Give equal weight to:
        // - 0: the element is disabled
        // - 4: some reasonable value
        // - -4: some reasonable negative value
        // - random value, likely unreasonably big
        prop_oneof![Just(0.), Just(4.), Just(-4.), ((1.)..=65535.)]
    }

    fn arbitrary_struts() -> impl Strategy<Value = Struts> {
        (
            arbitrary_spacing_neg(),
            arbitrary_spacing_neg(),
            arbitrary_spacing_neg(),
            arbitrary_spacing_neg(),
        )
            .prop_map(|(left, right, top, bottom)| Struts {
                left: FloatOrInt(left),
                right: FloatOrInt(right),
                top: FloatOrInt(top),
                bottom: FloatOrInt(bottom),
            })
    }

    fn arbitrary_center_focused_column() -> impl Strategy<Value = CenterFocusedColumn> {
        prop_oneof![
            Just(CenterFocusedColumn::Never),
            Just(CenterFocusedColumn::OnOverflow),
            Just(CenterFocusedColumn::Always),
        ]
    }

    prop_compose! {
        fn arbitrary_focus_ring()(
            off in any::<bool>(),
            width in arbitrary_spacing(),
        ) -> niri_config::FocusRing {
            niri_config::FocusRing {
                off,
                width: FloatOrInt(width),
                ..Default::default()
            }
        }
    }

    prop_compose! {
        fn arbitrary_border()(
            off in any::<bool>(),
            width in arbitrary_spacing(),
        ) -> niri_config::Border {
            niri_config::Border {
                off,
                width: FloatOrInt(width),
                ..Default::default()
            }
        }
    }

    prop_compose! {
        fn arbitrary_options()(
            gaps in arbitrary_spacing(),
            struts in arbitrary_struts(),
            focus_ring in arbitrary_focus_ring(),
            border in arbitrary_border(),
            center_focused_column in arbitrary_center_focused_column(),
            always_center_single_column in any::<bool>(),
            empty_workspace_above_first in any::<bool>(),
        ) -> Options {
            Options {
                gaps,
                struts,
                center_focused_column,
                always_center_single_column,
                empty_workspace_above_first,
                focus_ring,
                border,
                ..Default::default()
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: if std::env::var_os("RUN_SLOW_TESTS").is_none() {
                eprintln!("ignoring slow test");
                0
            } else {
                ProptestConfig::default().cases
            },
            ..ProptestConfig::default()
        })]

        #[test]
        fn random_operations_dont_panic(
            ops: Vec<Op>,
            options in arbitrary_options(),
            post_options in prop::option::of(arbitrary_options()),
        ) {
            // eprintln!("{ops:?}");
            let mut layout = check_ops_with_options(options, &ops);

            if let Some(post_options) = post_options {
                layout.update_options(post_options);
                layout.verify_invariants();
            }
        }
    }
}
