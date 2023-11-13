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

use std::cmp::{max, min};
use std::iter::zip;
use std::mem;
use std::rc::Rc;
use std::time::Duration;

use arrayvec::ArrayVec;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::utils::{
    CropRenderElement, Relocate, RelocateRenderElement,
};
use smithay::backend::renderer::element::{AsRenderElements, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::ImportAll;
use smithay::desktop::space::SpaceElement;
use smithay::desktop::{layer_map_for_output, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::render_elements;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::compositor::{send_surface_state, with_states};
use smithay::wayland::shell::xdg::SurfaceCachedState;

use crate::animation::Animation;
use crate::config::{self, Color, Config, PresetWidth, SizeChange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputId(String);

render_elements! {
    #[derive(Debug)]
    pub WorkspaceRenderElement<R> where R: ImportAll;
    Wayland = WaylandSurfaceRenderElement<R>,
    FocusRing = SolidColorRenderElement,
}
pub type MonitorRenderElement<R> =
    RelocateRenderElement<CropRenderElement<WorkspaceRenderElement<R>>>;

pub trait LayoutElement: SpaceElement + PartialEq + Clone {
    fn request_size(&self, size: Size<i32, Logical>);
    fn request_fullscreen(&self, size: Size<i32, Logical>);
    fn min_size(&self) -> Size<i32, Logical>;
    fn max_size(&self) -> Size<i32, Logical>;
    fn is_wl_surface(&self, wl_surface: &WlSurface) -> bool;
    fn has_ssd(&self) -> bool;
    fn set_preferred_scale_transform(&self, scale: i32, transform: Transform);
}

#[derive(Debug)]
pub struct Layout<W: LayoutElement> {
    /// Monitors and workspaes in the layout.
    monitor_set: MonitorSet<W>,
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

#[derive(Debug)]
pub struct Monitor<W: LayoutElement> {
    /// Output for this monitor.
    output: Output,
    // Must always contain at least one.
    workspaces: Vec<Workspace<W>>,
    /// Index of the currently active workspace.
    active_workspace_idx: usize,
    /// In-progress switch between workspaces.
    workspace_switch: Option<WorkspaceSwitch>,
    /// Configurable properties of the layout.
    options: Rc<Options>,
}

#[derive(Debug)]
enum WorkspaceSwitch {
    Animation(Animation),
    Gesture(WorkspaceSwitchGesture),
}

#[derive(Debug)]
struct WorkspaceSwitchGesture {
    /// Index of the workspace where the gesture was started.
    center_idx: usize,
    /// Current, fractional workspace index.
    current_idx: f64,
}

#[derive(Debug)]
pub struct Workspace<W: LayoutElement> {
    /// The original output of this workspace.
    ///
    /// Most of the time this will be the workspace's current output, however, after an output
    /// disconnection, it may remain pointing to the disconnected output.
    original_output: OutputId,

    /// Current output of this workspace.
    output: Option<Output>,

    /// Latest known view size for this workspace.
    ///
    /// This should be computed from the current workspace output size, or, if all outputs have
    /// been disconnected, preserved until a new output is connected.
    view_size: Size<i32, Logical>,

    /// Latest known working area for this workspace.
    ///
    /// This is similar to view size, but takes into account things like layer shell exclusive
    /// zones.
    working_area: Rectangle<i32, Logical>,

    /// Columns of windows on this workspace.
    columns: Vec<Column<W>>,

    /// Index of the currently active column, if any.
    active_column_idx: usize,

    /// Focus ring buffer and parameters.
    focus_ring: FocusRing,

    /// Offset of the view computed from the active column.
    ///
    /// Any gaps, including left padding from work area left exclusive zone, is handled
    /// with this view offset (rather than added as a constant elsewhere in the code). This allows
    /// for natural handling of fullscreen windows, which must ignore work area padding.
    view_offset: i32,

    /// Animation of the view offset, if one is currently ongoing.
    view_offset_anim: Option<Animation>,

    /// Whether to activate the previous, rather than the next, column upon column removal.
    ///
    /// When a new column is created and removed with no focus changes in-between, it is more
    /// natural to activate the previously-focused column. This variable tracks that.
    ///
    /// Since we only create-and-activate columns immediately to the right of the active column (in
    /// contrast to tabs in Firefox, for example), we can track this as a bool, rather than an
    /// index of the previous column to activate.
    activate_prev_column_on_removal: bool,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

#[derive(Debug)]
struct FocusRing {
    buffers: [SolidColorBuffer; 4],
    locations: [Point<i32, Logical>; 4],
    is_off: bool,
    is_border: bool,
    width: i32,
    active_color: Color,
    inactive_color: Color,
}

#[derive(Debug, PartialEq)]
struct Options {
    /// Padding around windows in logical pixels.
    gaps: i32,
    focus_ring: config::FocusRing,
    /// Column widths that `toggle_width()` switches between.
    preset_widths: Vec<ColumnWidth>,
    /// Initial width for new windows.
    default_width: Option<ColumnWidth>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            gaps: 16,
            focus_ring: Default::default(),
            preset_widths: vec![
                ColumnWidth::Proportion(1. / 3.),
                ColumnWidth::Proportion(0.5),
                ColumnWidth::Proportion(2. / 3.),
            ],
            default_width: None,
        }
    }
}

impl Options {
    fn from_config(config: &Config) -> Self {
        let preset_column_widths = &config.preset_column_widths;

        let preset_widths = if preset_column_widths.is_empty() {
            Options::default().preset_widths
        } else {
            preset_column_widths
                .iter()
                .copied()
                .map(ColumnWidth::from)
                .collect()
        };

        // Missing default_column_width maps to Some(ColumnWidth::Proportion(0.5)),
        // while present, but empty, maps to None.
        let default_width = config
            .default_column_width
            .as_ref()
            .map(|w| w.0.first().copied().map(ColumnWidth::from))
            .unwrap_or(Some(ColumnWidth::Proportion(0.5)));

        Self {
            gaps: config.gaps.into(),
            focus_ring: config.focus_ring,
            preset_widths,
            default_width,
        }
    }
}

/// Width of a column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    /// Proportion of the current view width.
    Proportion(f64),
    /// One of the proportion presets.
    ///
    /// This is separate from Proportion in order to be able to reliably cycle between preset
    /// proportions.
    Preset(usize),
    /// Fixed width in logical pixels.
    Fixed(i32),
}

impl From<PresetWidth> for ColumnWidth {
    fn from(value: PresetWidth) -> Self {
        match value {
            PresetWidth::Proportion(p) => Self::Proportion(p.clamp(0., 10000.)),
            PresetWidth::Fixed(f) => Self::Fixed(f.clamp(1, 100000)),
        }
    }
}

/// Height of a window in a column.
///
/// Proportional height is intentionally omitted. With column widths you frequently want e.g. two
/// columns side-by-side with 50% width each, and you want them to remain this way when moving to a
/// differently sized monitor. Windows in a column, however, already auto-size to fill the available
/// height, giving you this behavior. The only reason to set a different window height, then, is
/// when you want something in the window to fit exactly, e.g. to fit 30 lines in a terminal, which
/// corresponds to the `Fixed` variant.
///
/// This does not preclude the usual set of binds to set or resize a window proportionally. Just,
/// they are converted to, and stored as fixed height right away, so that once you resize a window
/// to fit the desired content, it can never become smaller than that when moving between monitors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowHeight {
    /// Automatically computed height, evenly distributed across the column.
    Auto,
    /// Fixed height in logical pixels.
    Fixed(i32),
}

#[derive(Debug)]
struct Column<W: LayoutElement> {
    /// Windows in this column.
    ///
    /// Must be non-empty.
    windows: Vec<W>,

    /// Heights of the windows.
    ///
    /// Must have the same number of elements as `windows`.
    heights: Vec<WindowHeight>,

    /// Index of the currently active window.
    active_window_idx: usize,

    /// Desired width of this column.
    ///
    /// If the column is full-width or full-screened, this is the width that should be restored
    /// upon unfullscreening and untoggling full-width.
    width: ColumnWidth,

    /// Whether this column is full-width.
    is_full_width: bool,

    /// Whether this column contains a single full-screened window.
    is_fullscreen: bool,

    /// Latest known view size for this column's workspace.
    view_size: Size<i32, Logical>,

    /// Latest known working area for this column's workspace.
    working_area: Rectangle<i32, Logical>,

    /// Configurable properties of the layout.
    options: Rc<Options>,
}

impl OutputId {
    pub fn new(output: &Output) -> Self {
        Self(output.name())
    }
}

impl LayoutElement for Window {
    fn request_size(&self, size: Size<i32, Logical>) {
        self.toplevel().with_pending_state(|state| {
            state.size = Some(size);
            state.states.unset(xdg_toplevel::State::Fullscreen);
        });
    }

    fn request_fullscreen(&self, size: Size<i32, Logical>) {
        self.toplevel().with_pending_state(|state| {
            state.size = Some(size);
            state.states.set(xdg_toplevel::State::Fullscreen);
        });
    }

    fn min_size(&self) -> Size<i32, Logical> {
        with_states(self.toplevel().wl_surface(), |state| {
            let curr = state.cached_state.current::<SurfaceCachedState>();
            curr.min_size
        })
    }

    fn max_size(&self) -> Size<i32, Logical> {
        with_states(self.toplevel().wl_surface(), |state| {
            let curr = state.cached_state.current::<SurfaceCachedState>();
            curr.max_size
        })
    }

    fn is_wl_surface(&self, wl_surface: &WlSurface) -> bool {
        self.toplevel().wl_surface() == wl_surface
    }

    fn set_preferred_scale_transform(&self, scale: i32, transform: Transform) {
        self.with_surfaces(|surface, data| {
            send_surface_state(surface, data, scale, transform);
        });
    }

    fn has_ssd(&self) -> bool {
        self.toplevel().current_state().decoration_mode
            == Some(zxdg_toplevel_decoration_v1::Mode::ServerSide)
    }
}

impl FocusRing {
    fn update(
        &mut self,
        win_pos: Point<i32, Logical>,
        win_size: Size<i32, Logical>,
        is_border: bool,
    ) {
        if is_border {
            self.buffers[0].resize((win_size.w + self.width * 2, self.width));
            self.buffers[1].resize((win_size.w + self.width * 2, self.width));
            self.buffers[2].resize((self.width, win_size.h));
            self.buffers[3].resize((self.width, win_size.h));

            self.locations[0] = win_pos + Point::from((-self.width, -self.width));
            self.locations[1] = win_pos + Point::from((-self.width, win_size.h));
            self.locations[2] = win_pos + Point::from((-self.width, 0));
            self.locations[3] = win_pos + Point::from((win_size.w, 0));
        } else {
            let size = win_size + Size::from((self.width * 2, self.width * 2));
            self.buffers[0].resize(size);
            self.locations[0] = win_pos - Point::from((self.width, self.width));
        }

        self.is_border = is_border;
    }

    fn set_active(&mut self, is_active: bool) {
        let color = if is_active {
            self.active_color.into()
        } else {
            self.inactive_color.into()
        };

        for buf in &mut self.buffers {
            buf.set_color(color);
        }
    }

    fn render(&self, scale: Scale<f64>) -> impl Iterator<Item = SolidColorRenderElement> {
        let mut rv = ArrayVec::<_, 4>::new();

        if self.is_off {
            return rv.into_iter();
        }

        let mut push = |buffer, location: Point<i32, Logical>| {
            let elem = SolidColorRenderElement::from_buffer(
                buffer,
                location.to_physical_precise_round(scale),
                scale,
                1.,
                Kind::Unspecified,
            );
            rv.push(elem);
        };

        if self.is_border {
            for (buf, loc) in zip(&self.buffers, self.locations) {
                push(buf, loc);
            }
        } else {
            push(&self.buffers[0], self.locations[0]);
        }

        rv.into_iter()
    }
}

impl FocusRing {
    fn new(config: config::FocusRing) -> Self {
        Self {
            buffers: Default::default(),
            locations: Default::default(),
            is_off: config.off,
            is_border: false,
            width: config.width.into(),
            active_color: config.active_color,
            inactive_color: config.inactive_color,
        }
    }
}

impl WorkspaceSwitch {
    fn current_idx(&self) -> f64 {
        match self {
            WorkspaceSwitch::Animation(anim) => anim.value(),
            WorkspaceSwitch::Gesture(gesture) => gesture.current_idx,
        }
    }

    /// Returns `true` if the workspace switch is [`Animation`].
    ///
    /// [`Animation`]: WorkspaceSwitch::Animation
    #[must_use]
    fn is_animation(&self) -> bool {
        matches!(self, Self::Animation(..))
    }
}

impl ColumnWidth {
    fn resolve(self, options: &Options, view_width: i32) -> i32 {
        match self {
            ColumnWidth::Proportion(proportion) => {
                ((view_width - options.gaps) as f64 * proportion).floor() as i32 - options.gaps
            }
            ColumnWidth::Preset(idx) => options.preset_widths[idx].resolve(options, view_width),
            ColumnWidth::Fixed(width) => width,
        }
    }
}

impl<W: LayoutElement> Layout<W> {
    pub fn new(config: &Config) -> Self {
        Self {
            monitor_set: MonitorSet::NoOutputs { workspaces: vec![] },
            options: Rc::new(Options::from_config(config)),
        }
    }

    pub fn add_output(&mut self, output: Output) {
        let id = OutputId::new(&output);

        self.monitor_set = match mem::take(&mut self.monitor_set) {
            MonitorSet::Normal {
                mut monitors,
                primary_idx,
                active_monitor_idx,
            } => {
                let primary = &mut monitors[primary_idx];

                let mut workspaces = vec![];
                for i in (0..primary.workspaces.len()).rev() {
                    if primary.workspaces[i].original_output == id {
                        let ws = primary.workspaces.remove(i);

                        // The user could've closed a window while remaining on this workspace, on
                        // another monitor. However, we will add an empty workspace in the end
                        // instead.
                        if ws.has_windows() {
                            workspaces.push(ws);
                        }

                        if i <= primary.active_workspace_idx {
                            primary.active_workspace_idx =
                                primary.active_workspace_idx.saturating_sub(1);
                        }
                    }
                }
                workspaces.reverse();

                // Make sure there's always an empty workspace.
                workspaces.push(Workspace::new(output.clone(), self.options.clone()));

                for ws in &mut workspaces {
                    ws.set_output(Some(output.clone()));
                }

                monitors.push(Monitor::new(output, workspaces, self.options.clone()));
                MonitorSet::Normal {
                    monitors,
                    primary_idx,
                    active_monitor_idx,
                }
            }
            MonitorSet::NoOutputs { mut workspaces } => {
                // We know there are no empty workspaces there, so add one.
                workspaces.push(Workspace::new(output.clone(), self.options.clone()));

                for workspace in &mut workspaces {
                    workspace.set_output(Some(output.clone()));
                }

                let monitor = Monitor::new(output, workspaces, self.options.clone());

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
                let mut workspaces = monitor.workspaces;

                for ws in &mut workspaces {
                    ws.set_output(None);
                }

                // Get rid of empty workspaces.
                workspaces.retain(|ws| ws.has_windows());

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

                    let empty_was_focused =
                        primary.active_workspace_idx == primary.workspaces.len() - 1;

                    // Push the workspaces from the removed monitor in the end, right before the
                    // last, empty, workspace.
                    let empty = primary.workspaces.remove(primary.workspaces.len() - 1);
                    primary.workspaces.extend(workspaces);
                    primary.workspaces.push(empty);

                    // If the empty workspace was focused on the primary monitor, keep it focused.
                    if empty_was_focused {
                        primary.active_workspace_idx = primary.workspaces.len() - 1;
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

    pub fn add_window_by_idx(
        &mut self,
        monitor_idx: usize,
        workspace_idx: usize,
        window: W,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            panic!()
        };

        monitors[monitor_idx].add_window(workspace_idx, window, activate, width, is_full_width);

        if activate {
            *active_monitor_idx = monitor_idx;
        }
    }

    /// Adds a new window to the layout.
    ///
    /// Returns an output that the window was added to, if there were any outputs.
    pub fn add_window(
        &mut self,
        window: W,
        activate: bool,
        width: Option<ColumnWidth>,
        is_full_width: bool,
    ) -> Option<&Output> {
        let width = width
            .or(self.options.default_width)
            .unwrap_or_else(|| ColumnWidth::Fixed(window.geometry().size.w));

        match &mut self.monitor_set {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                let mon = &mut monitors[*active_monitor_idx];
                mon.add_window(
                    mon.active_workspace_idx,
                    window,
                    activate,
                    width,
                    is_full_width,
                );
                Some(&mon.output)
            }
            MonitorSet::NoOutputs { workspaces } => {
                let ws = if let Some(ws) = workspaces.get_mut(0) {
                    ws
                } else {
                    workspaces.push(Workspace::new_no_outputs(self.options.clone()));
                    &mut workspaces[0]
                };
                ws.add_window(window, activate, width, is_full_width);
                None
            }
        }
    }

    pub fn remove_window(&mut self, window: &W) {
        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for (idx, ws) in mon.workspaces.iter_mut().enumerate() {
                        if ws.has_window(window) {
                            ws.remove_window(window);

                            // Clean up empty workspaces that are not active and not last.
                            if !ws.has_windows()
                                && idx != mon.active_workspace_idx
                                && idx != mon.workspaces.len() - 1
                            {
                                mon.workspaces.remove(idx);

                                if idx < mon.active_workspace_idx {
                                    mon.active_workspace_idx -= 1;
                                }
                            }

                            break;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for (idx, ws) in workspaces.iter_mut().enumerate() {
                    if ws.has_window(window) {
                        ws.remove_window(window);

                        // Clean up empty workspaces.
                        if !ws.has_windows() {
                            workspaces.remove(idx);
                        }

                        break;
                    }
                }
            }
        }
    }

    pub fn update_window(&mut self, window: &W) {
        match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mut mon.workspaces {
                        if ws.has_window(window) {
                            ws.update_window(window);
                            return;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.update_window(window);
                        return;
                    }
                }
            }
        }
    }

    pub fn find_window_and_output(&self, wl_surface: &WlSurface) -> Option<(W, Output)> {
        if let MonitorSet::Normal { monitors, .. } = &self.monitor_set {
            for mon in monitors {
                for ws in &mon.workspaces {
                    if let Some(window) = ws.find_wl_surface(wl_surface) {
                        return Some((window.clone(), mon.output.clone()));
                    }
                }
            }
        }

        None
    }

    pub fn update_output_size(&mut self, output: &Output) {
        let MonitorSet::Normal { monitors, .. } = &mut self.monitor_set else {
            panic!()
        };

        for mon in monitors {
            if &mon.output == output {
                let view_size = output_size(output);
                let working_area = layer_map_for_output(output).non_exclusive_zone();

                for ws in &mut mon.workspaces {
                    ws.set_view_size(view_size, working_area);
                }

                break;
            }
        }
    }

    pub fn activate_window(&mut self, window: &W) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &mut self.monitor_set
        else {
            todo!()
        };

        for (monitor_idx, mon) in monitors.iter_mut().enumerate() {
            for (_workspace_idx, ws) in mon.workspaces.iter_mut().enumerate() {
                if ws.has_window(window) {
                    *active_monitor_idx = monitor_idx;
                    // FIXME: switch to this workspace if not already switching.
                    ws.activate_window(window);
                    break;
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

    pub fn active_window(&self) -> Option<(W, Output)> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &self.monitor_set
        else {
            return None;
        };

        let mon = &monitors[*active_monitor_idx];
        let ws = &mon.workspaces[mon.active_workspace_idx];

        if ws.columns.is_empty() {
            return None;
        }

        let col = &ws.columns[ws.active_column_idx];
        Some((
            col.windows[col.active_window_idx].clone(),
            mon.output.clone(),
        ))
    }

    pub fn windows_for_output(&self, output: &Output) -> impl Iterator<Item = &W> + '_ {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            panic!()
        };

        let mon = monitors.iter().find(|mon| &mon.output == output).unwrap();
        mon.workspaces.iter().flat_map(|ws| ws.windows())
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

    pub fn monitor_for_output(&self, output: &Output) -> Option<&Monitor<W>> {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return None;
        };

        monitors.iter().find(|monitor| &monitor.output == output)
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

    pub fn move_to_workspace(&mut self, idx: u8) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.move_to_workspace(idx);
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

    pub fn switch_workspace(&mut self, idx: u8) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.switch_workspace(idx);
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

    pub fn focus(&self) -> Option<&W> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = &self.monitor_set
        else {
            return None;
        };

        monitors[*active_monitor_idx].focus()
    }

    pub fn window_under(
        &self,
        output: &Output,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<(&W, Point<i32, Logical>)> {
        let MonitorSet::Normal { monitors, .. } = &self.monitor_set else {
            return None;
        };

        let mon = monitors.iter().find(|mon| &mon.output == output)?;
        mon.window_under(pos_within_output)
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        let (monitors, &primary_idx, &active_monitor_idx) = match &self.monitor_set {
            MonitorSet::Normal {
                monitors,
                primary_idx,
                active_monitor_idx,
            } => (monitors, primary_idx, active_monitor_idx),
            MonitorSet::NoOutputs { workspaces } => {
                for workspace in workspaces {
                    assert!(
                        workspace.has_windows(),
                        "with no outputs there cannot be empty workspaces"
                    );

                    assert_eq!(
                        workspace.options, self.options,
                        "workspace options must be synchronized with layout"
                    );

                    workspace.verify_invariants();
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

            assert_eq!(
                monitor.options, self.options,
                "monitor options must be synchronized with layout"
            );

            let monitor_id = OutputId::new(&monitor.output);

            if idx == primary_idx {
                for ws in &monitor.workspaces {
                    if ws.original_output == monitor_id {
                        // This is the primary monitor's own workspace.
                        continue;
                    }

                    let own_monitor_exists = monitors
                        .iter()
                        .any(|m| OutputId::new(&m.output) == ws.original_output);
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
                        .any(|workspace| workspace.original_output == monitor_id),
                    "secondary monitor must not have any non-own workspaces"
                );
            }

            assert!(
                monitor.workspaces.last().unwrap().columns.is_empty(),
                "monitor must have an empty workspace in the end"
            );

            // If there's no workspace switch in progress, there can't be any non-last non-active
            // empty workspaces.
            if monitor.workspace_switch.is_none() {
                for (idx, ws) in monitor.workspaces.iter().enumerate().rev().skip(1) {
                    if idx != monitor.active_workspace_idx {
                        assert!(
                            !ws.columns.is_empty(),
                            "non-active workspace can't be empty except the last one"
                        );
                    }
                }
            }

            // FIXME: verify that primary doesn't have any workspaces for which their own monitor
            // exists.

            for workspace in &monitor.workspaces {
                assert_eq!(
                    workspace.options, self.options,
                    "workspace options must be synchronized with layout"
                );

                workspace.verify_invariants();
            }
        }
    }

    pub fn advance_animations(&mut self, current_time: Duration) {
        let _span = tracy_client::span!("Layout::advance_animations");

        match &mut self.monitor_set {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                for (idx, mon) in monitors.iter_mut().enumerate() {
                    mon.advance_animations(current_time, idx == *active_monitor_idx);
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    ws.advance_animations(current_time, false);
                }
            }
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        let options = Rc::new(Options::from_config(config));

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

    pub fn set_window_height(&mut self, change: SizeChange) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.set_window_height(change);
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

    pub fn move_to_output(&mut self, output: &Output) {
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
            if !ws.has_windows() {
                return;
            }
            let column = &ws.columns[ws.active_column_idx];
            let window = column.windows[column.active_window_idx].clone();
            let width = column.width;
            let is_full_width = column.is_full_width;
            ws.remove_window(&window);

            let workspace_idx = monitors[new_idx].active_workspace_idx;
            self.add_window_by_idx(new_idx, workspace_idx, window, true, width, is_full_width);
        }
    }

    pub fn move_window_to_output(&mut self, window: W, output: &Output) {
        if !matches!(&self.monitor_set, MonitorSet::Normal { .. }) {
            return;
        }

        self.remove_window(&window);

        if let MonitorSet::Normal { monitors, .. } = &mut self.monitor_set {
            let mut width = None;
            let mut is_full_width = false;
            for mon in &*monitors {
                for ws in &mon.workspaces {
                    for col in &ws.columns {
                        if col.windows.contains(&window) {
                            width = Some(col.width);
                            is_full_width = col.is_full_width;
                            break;
                        }
                    }
                }
            }
            let Some(width) = width else { return };

            let new_idx = monitors
                .iter()
                .position(|mon| &mon.output == output)
                .unwrap();

            let workspace_idx = monitors[new_idx].active_workspace_idx;
            // FIXME: activate only if it was already active and focused.
            self.add_window_by_idx(new_idx, workspace_idx, window, true, width, is_full_width);
        }
    }

    pub fn set_fullscreen(&mut self, window: &W, is_fullscreen: bool) {
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

    pub fn toggle_fullscreen(&mut self, window: &W) {
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

    pub fn workspace_switch_gesture_begin(&mut self, output: &Output) {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => unreachable!(),
        };

        for monitor in monitors {
            // Cancel the gesture on other outputs.
            if &monitor.output != output {
                if let Some(WorkspaceSwitch::Gesture(_)) = monitor.workspace_switch {
                    monitor.workspace_switch = None;
                }
                continue;
            }

            let center_idx = monitor.active_workspace_idx;
            let current_idx = monitor
                .workspace_switch
                .as_ref()
                .map(|s| s.current_idx())
                .unwrap_or(center_idx as f64);

            let gesture = WorkspaceSwitchGesture {
                center_idx,
                current_idx,
            };
            monitor.workspace_switch = Some(WorkspaceSwitch::Gesture(gesture));
        }
    }

    pub fn workspace_switch_gesture_update(&mut self, delta_y: f64) -> Option<Option<Output>> {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => return None,
        };

        for monitor in monitors {
            if let Some(WorkspaceSwitch::Gesture(gesture)) = &mut monitor.workspace_switch {
                // Normalize like GNOME Shell's workspace switching.
                let delta_y = -delta_y / 400.;

                let min = gesture.center_idx.saturating_sub(1) as f64;
                let max = (gesture.center_idx + 1).min(monitor.workspaces.len() - 1) as f64;
                let new_idx = (gesture.current_idx + delta_y).clamp(min, max);

                if gesture.current_idx == new_idx {
                    return Some(None);
                }

                gesture.current_idx = new_idx;
                return Some(Some(monitor.output.clone()));
            }
        }

        None
    }

    pub fn workspace_switch_gesture_end(&mut self, cancelled: bool) -> Option<Output> {
        let monitors = match &mut self.monitor_set {
            MonitorSet::Normal { monitors, .. } => monitors,
            MonitorSet::NoOutputs { .. } => return None,
        };

        for monitor in monitors {
            if let Some(WorkspaceSwitch::Gesture(gesture)) = &mut monitor.workspace_switch {
                if cancelled {
                    monitor.workspace_switch = None;
                    return Some(monitor.output.clone());
                }

                // FIXME: keep track of gesture velocity and use it to compute the final point and
                // to animate to it.
                let current_idx = gesture.current_idx;
                let idx = current_idx.round() as usize;

                monitor.active_workspace_idx = idx;
                monitor.workspace_switch = Some(WorkspaceSwitch::Animation(Animation::new(
                    current_idx,
                    idx as f64,
                    Duration::from_millis(250),
                )));

                return Some(monitor.output.clone());
            }
        }

        None
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
}

impl Layout<Window> {
    pub fn refresh(&self) {
        let _span = tracy_client::span!("MonitorSet::refresh");

        match &self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mon.workspaces {
                        ws.refresh();
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    ws.refresh();
                }
            }
        }
    }
}

impl<W: LayoutElement> Default for MonitorSet<W> {
    fn default() -> Self {
        Self::NoOutputs { workspaces: vec![] }
    }
}

impl<W: LayoutElement> Monitor<W> {
    fn new(output: Output, workspaces: Vec<Workspace<W>>, options: Rc<Options>) -> Self {
        Self {
            output,
            workspaces,
            active_workspace_idx: 0,
            workspace_switch: None,
            options,
        }
    }

    fn active_workspace(&mut self) -> &mut Workspace<W> {
        &mut self.workspaces[self.active_workspace_idx]
    }

    fn activate_workspace(&mut self, idx: usize) {
        if self.active_workspace_idx == idx {
            return;
        }

        let current_idx = self
            .workspace_switch
            .as_ref()
            .map(|s| s.current_idx())
            .unwrap_or(self.active_workspace_idx as f64);

        self.active_workspace_idx = idx;

        self.workspace_switch = Some(WorkspaceSwitch::Animation(Animation::new(
            current_idx,
            idx as f64,
            Duration::from_millis(250),
        )));
    }

    pub fn add_window(
        &mut self,
        workspace_idx: usize,
        window: W,
        activate: bool,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        let workspace = &mut self.workspaces[workspace_idx];

        workspace.add_window(window.clone(), activate, width, is_full_width);

        // After adding a new window, workspace becomes this output's own.
        workspace.original_output = OutputId::new(&self.output);

        if workspace_idx == self.workspaces.len() - 1 {
            // Insert a new empty workspace.
            let ws = Workspace::new(self.output.clone(), self.options.clone());
            self.workspaces.push(ws);
        }

        if activate {
            self.activate_workspace(workspace_idx);
        }
    }

    fn clean_up_workspaces(&mut self) {
        assert!(self.workspace_switch.is_none());

        for idx in (0..self.workspaces.len() - 1).rev() {
            if self.active_workspace_idx == idx {
                continue;
            }

            if !self.workspaces[idx].has_windows() {
                self.workspaces.remove(idx);
                if self.active_workspace_idx > idx {
                    self.active_workspace_idx -= 1;
                }
            }
        }
    }

    pub fn move_left(&mut self) {
        self.active_workspace().move_left();
    }

    pub fn move_right(&mut self) {
        self.active_workspace().move_right();
    }

    pub fn move_down(&mut self) {
        self.active_workspace().move_down();
    }

    pub fn move_up(&mut self) {
        self.active_workspace().move_up();
    }

    pub fn focus_left(&mut self) {
        self.active_workspace().focus_left();
    }

    pub fn focus_right(&mut self) {
        self.active_workspace().focus_right();
    }

    pub fn focus_down(&mut self) {
        self.active_workspace().focus_down();
    }

    pub fn focus_up(&mut self) {
        self.active_workspace().focus_up();
    }

    pub fn move_to_workspace_up(&mut self) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = source_workspace_idx.saturating_sub(1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.columns.is_empty() {
            return;
        }

        let column = &mut workspace.columns[workspace.active_column_idx];
        let width = column.width;
        let is_full_width = column.is_full_width;
        let window = column.windows[column.active_window_idx].clone();
        workspace.remove_window(&window);

        self.add_window(new_idx, window, true, width, is_full_width);
    }

    pub fn move_to_workspace_down(&mut self) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(source_workspace_idx + 1, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.columns.is_empty() {
            return;
        }

        let column = &mut workspace.columns[workspace.active_column_idx];
        let width = column.width;
        let is_full_width = column.is_full_width;
        let window = column.windows[column.active_window_idx].clone();
        workspace.remove_window(&window);

        self.add_window(new_idx, window, true, width, is_full_width);
    }

    pub fn move_to_workspace(&mut self, idx: u8) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(idx.saturating_sub(1) as usize, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.columns.is_empty() {
            return;
        }

        let column = &mut workspace.columns[workspace.active_column_idx];
        let width = column.width;
        let is_full_width = column.is_full_width;
        let window = column.windows[column.active_window_idx].clone();
        workspace.remove_window(&window);

        self.add_window(new_idx, window, true, width, is_full_width);

        // Don't animate this action.
        self.workspace_switch = None;

        self.clean_up_workspaces();
    }

    pub fn switch_workspace_up(&mut self) {
        self.activate_workspace(self.active_workspace_idx.saturating_sub(1));
    }

    pub fn switch_workspace_down(&mut self) {
        self.activate_workspace(min(
            self.active_workspace_idx + 1,
            self.workspaces.len() - 1,
        ));
    }

    pub fn switch_workspace(&mut self, idx: u8) {
        self.activate_workspace(min(
            idx.saturating_sub(1) as usize,
            self.workspaces.len() - 1,
        ));
        // Don't animate this action.
        self.workspace_switch = None;

        self.clean_up_workspaces();
    }

    pub fn consume_into_column(&mut self) {
        self.active_workspace().consume_into_column();
    }

    pub fn expel_from_column(&mut self) {
        self.active_workspace().expel_from_column();
    }

    pub fn center_column(&mut self) {
        self.active_workspace().center_column();
    }

    pub fn focus(&self) -> Option<&W> {
        let workspace = &self.workspaces[self.active_workspace_idx];
        if !workspace.has_windows() {
            return None;
        }

        let column = &workspace.columns[workspace.active_column_idx];
        Some(&column.windows[column.active_window_idx])
    }

    pub fn advance_animations(&mut self, current_time: Duration, is_active: bool) {
        if let Some(WorkspaceSwitch::Animation(anim)) = &mut self.workspace_switch {
            anim.set_current_time(current_time);
            if anim.is_done() {
                self.workspace_switch = None;
                self.clean_up_workspaces();
            }
        }

        for ws in &mut self.workspaces {
            ws.advance_animations(current_time, is_active);
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.workspace_switch
            .as_ref()
            .is_some_and(|s| s.is_animation())
            || self.workspaces.iter().any(|ws| ws.are_animations_ongoing())
    }

    pub fn are_transitions_ongoing(&self) -> bool {
        self.workspace_switch.is_some()
            || self.workspaces.iter().any(|ws| ws.are_animations_ongoing())
    }

    fn update_config(&mut self, options: Rc<Options>) {
        for ws in &mut self.workspaces {
            ws.update_config(options.clone());
        }

        self.options = options;
    }

    fn toggle_width(&mut self) {
        self.active_workspace().toggle_width();
    }

    fn toggle_full_width(&mut self) {
        self.active_workspace().toggle_full_width();
    }

    fn set_column_width(&mut self, change: SizeChange) {
        self.active_workspace().set_column_width(change);
    }

    fn set_window_height(&mut self, change: SizeChange) {
        self.active_workspace().set_window_height(change);
    }

    fn move_workspace_down(&mut self) {
        let new_idx = min(self.active_workspace_idx + 1, self.workspaces.len() - 1);
        if new_idx == self.active_workspace_idx {
            return;
        }

        self.workspaces.swap(self.active_workspace_idx, new_idx);

        if new_idx == self.workspaces.len() - 1 {
            // Insert a new empty workspace.
            let ws = Workspace::new(self.output.clone(), self.options.clone());
            self.workspaces.push(ws);
        }

        self.activate_workspace(new_idx);
        self.workspace_switch = None;

        self.clean_up_workspaces();
    }

    fn move_workspace_up(&mut self) {
        let new_idx = self.active_workspace_idx.saturating_sub(1);
        if new_idx == self.active_workspace_idx {
            return;
        }

        self.workspaces.swap(self.active_workspace_idx, new_idx);

        if self.active_workspace_idx == self.workspaces.len() - 1 {
            // Insert a new empty workspace.
            let ws = Workspace::new(self.output.clone(), self.options.clone());
            self.workspaces.push(ws);
        }

        self.activate_workspace(new_idx);
        self.workspace_switch = None;

        self.clean_up_workspaces();
    }

    pub fn window_under(
        &self,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<(&W, Point<i32, Logical>)> {
        match &self.workspace_switch {
            Some(switch) => {
                let size = output_size(&self.output);

                let render_idx = switch.current_idx();
                let before_idx = render_idx.floor() as usize;
                let after_idx = render_idx.ceil() as usize;

                let offset = ((render_idx - before_idx as f64) * size.h as f64).round() as i32;

                let (idx, ws_offset) = if pos_within_output.y < (size.h - offset) as f64 {
                    (before_idx, Point::from((0, offset)))
                } else {
                    (after_idx, Point::from((0, -size.h + offset)))
                };

                let ws = &self.workspaces[idx];
                let (win, win_pos) = ws.window_under(pos_within_output + ws_offset.to_f64())?;
                Some((win, win_pos - ws_offset))
            }
            None => {
                let ws = &self.workspaces[self.active_workspace_idx];
                ws.window_under(pos_within_output)
            }
        }
    }

    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer only if the view is stationary.
        if self.workspace_switch.is_some() {
            return false;
        }

        let ws = &self.workspaces[self.active_workspace_idx];
        ws.render_above_top_layer()
    }
}

impl Monitor<Window> {
    pub fn render_elements(
        &self,
        renderer: &mut GlesRenderer,
    ) -> Vec<MonitorRenderElement<GlesRenderer>> {
        let _span = tracy_client::span!("Monitor::render_elements");

        let output_scale = Scale::from(self.output.current_scale().fractional_scale());
        let output_transform = self.output.current_transform();
        let output_mode = self.output.current_mode().unwrap();
        let size = output_transform.transform_size(output_mode.size);

        match &self.workspace_switch {
            Some(switch) => {
                let render_idx = switch.current_idx();
                let before_idx = render_idx.floor() as usize;
                let after_idx = render_idx.ceil() as usize;

                let offset = ((render_idx - before_idx as f64) * size.h as f64).round() as i32;

                let before = self.workspaces[before_idx].render_elements(renderer);
                let after = self.workspaces[after_idx].render_elements(renderer);

                let before = before.into_iter().filter_map(|elem| {
                    Some(RelocateRenderElement::from_element(
                        CropRenderElement::from_element(
                            elem,
                            output_scale,
                            Rectangle::from_extemities((0, offset), (size.w, size.h)),
                        )?,
                        (0, -offset),
                        Relocate::Relative,
                    ))
                });
                let after = after.into_iter().filter_map(|elem| {
                    Some(RelocateRenderElement::from_element(
                        CropRenderElement::from_element(
                            elem,
                            output_scale,
                            Rectangle::from_extemities((0, 0), (size.w, offset)),
                        )?,
                        (0, -offset + size.h),
                        Relocate::Relative,
                    ))
                });
                before.chain(after).collect()
            }
            None => {
                let elements = self.workspaces[self.active_workspace_idx].render_elements(renderer);
                elements
                    .into_iter()
                    .filter_map(|elem| {
                        Some(RelocateRenderElement::from_element(
                            CropRenderElement::from_element(
                                elem,
                                output_scale,
                                Rectangle::from_loc_and_size((0, 0), size),
                            )?,
                            (0, 0),
                            Relocate::Relative,
                        ))
                    })
                    .collect()
            }
        }
    }
}

impl<W: LayoutElement> Workspace<W> {
    fn new(output: Output, options: Rc<Options>) -> Self {
        let working_area = layer_map_for_output(&output).non_exclusive_zone();
        Self {
            original_output: OutputId::new(&output),
            view_size: output_size(&output),
            working_area,
            output: Some(output),
            columns: vec![],
            active_column_idx: 0,
            focus_ring: FocusRing::new(options.focus_ring),
            view_offset: 0,
            view_offset_anim: None,
            activate_prev_column_on_removal: false,
            options,
        }
    }

    fn new_no_outputs(options: Rc<Options>) -> Self {
        Self {
            output: None,
            original_output: OutputId(String::new()),
            view_size: Size::from((1280, 720)),
            working_area: Rectangle::from_loc_and_size((0, 0), (1280, 720)),
            columns: vec![],
            active_column_idx: 0,
            focus_ring: FocusRing::new(options.focus_ring),
            view_offset: 0,
            view_offset_anim: None,
            activate_prev_column_on_removal: false,
            options,
        }
    }

    pub fn advance_animations(&mut self, current_time: Duration, is_active: bool) {
        match &mut self.view_offset_anim {
            Some(anim) => {
                anim.set_current_time(current_time);
                self.view_offset = anim.value().round() as i32;
                if anim.is_done() {
                    self.view_offset_anim = None;
                }
            }
            None => (),
        }

        let view_pos = self.view_pos();

        // This shall one day become a proper animation.
        if !self.columns.is_empty() {
            let col = &self.columns[self.active_column_idx];
            let active_win = &col.windows[col.active_window_idx];
            let geom = active_win.geometry();
            let has_ssd = active_win.has_ssd();

            let win_pos = Point::from((
                self.column_x(self.active_column_idx) - view_pos,
                col.window_y(col.active_window_idx),
            ));

            self.focus_ring.update(win_pos, geom.size, has_ssd);
            self.focus_ring.set_active(is_active);
        }
    }

    pub fn are_animations_ongoing(&self) -> bool {
        self.view_offset_anim.is_some()
    }

    fn update_config(&mut self, options: Rc<Options>) {
        let c = &options.focus_ring;
        self.focus_ring.is_off = c.off;
        self.focus_ring.width = c.width.into();
        self.focus_ring.active_color = c.active_color;
        self.focus_ring.inactive_color = c.inactive_color;
        // The focus ring buffer will be updated in a subsequent update_animations call.

        for column in &mut self.columns {
            column.update_config(options.clone());
        }

        self.options = options;
    }

    fn windows(&self) -> impl Iterator<Item = &W> + '_ {
        self.columns.iter().flat_map(|col| col.windows.iter())
    }

    fn set_output(&mut self, output: Option<Output>) {
        if self.output == output {
            return;
        }

        if let Some(output) = self.output.take() {
            for win in self.windows() {
                win.output_leave(&output);
            }
        }

        self.output = output;

        if let Some(output) = &self.output {
            let working_area = layer_map_for_output(output).non_exclusive_zone();
            self.set_view_size(output_size(output), working_area);

            for win in self.windows() {
                self.enter_output_for_window(win);
            }
        }
    }

    fn enter_output_for_window(&self, window: &W) {
        if let Some(output) = &self.output {
            prepare_for_output(window, output);

            // FIXME: proper overlap.
            window.output_enter(
                output,
                Rectangle::from_loc_and_size((0, 0), (i32::MAX, i32::MAX)),
            );
        }
    }

    fn set_view_size(&mut self, size: Size<i32, Logical>, working_area: Rectangle<i32, Logical>) {
        if self.view_size == size && self.working_area == working_area {
            return;
        }

        self.view_size = size;
        self.working_area = working_area;

        for col in &mut self.columns {
            col.set_view_size(self.view_size, self.working_area);
        }
    }

    fn toplevel_bounds(&self) -> Size<i32, Logical> {
        Size::from((
            max(self.working_area.size.w - self.options.gaps * 2, 1),
            max(self.working_area.size.h - self.options.gaps * 2, 1),
        ))
    }

    pub fn configure_new_window(&self, window: &Window) {
        let width = if let Some(width) = self.options.default_width {
            max(1, width.resolve(&self.options, self.working_area.size.w))
        } else {
            0
        };

        let height = self.working_area.size.h - self.options.gaps * 2;
        let size = Size::from((width, max(height, 1)));

        let bounds = self.toplevel_bounds();

        if let Some(output) = self.output.as_ref() {
            prepare_for_output(window, output);
        }

        window.toplevel().with_pending_state(|state| {
            state.size = Some(size);
            state.bounds = Some(bounds);
        });
    }

    fn compute_new_view_offset_for_column(&self, current_x: i32, idx: usize) -> i32 {
        if self.columns[idx].is_fullscreen {
            return 0;
        }

        let new_col_x = self.column_x(idx);

        let final_x = if let Some(anim) = &self.view_offset_anim {
            current_x - self.view_offset + anim.to().round() as i32
        } else {
            current_x
        };

        let new_offset = compute_new_view_offset(
            final_x + self.working_area.loc.x,
            self.working_area.size.w,
            new_col_x,
            self.columns[idx].width(),
            self.options.gaps,
        );

        // Non-fullscreen windows are always offset at least by the working area position.
        new_offset - self.working_area.loc.x
    }

    fn animate_view_offset_to_column(&mut self, current_x: i32, idx: usize) {
        let new_view_offset = self.compute_new_view_offset_for_column(current_x, idx);

        let new_col_x = self.column_x(idx);
        let from_view_offset = current_x - new_col_x;
        self.view_offset = from_view_offset;

        // If we're already animating towards that, don't restart it.
        if let Some(anim) = &self.view_offset_anim {
            if anim.value().round() as i32 == self.view_offset
                && anim.to().round() as i32 == new_view_offset
            {
                return;
            }
        }

        // If our view offset is already this, we don't need to do anything.
        if self.view_offset == new_view_offset {
            self.view_offset_anim = None;
            return;
        }

        self.view_offset_anim = Some(Animation::new(
            self.view_offset as f64,
            new_view_offset as f64,
            Duration::from_millis(250),
        ));
    }

    fn activate_column(&mut self, idx: usize) {
        if self.active_column_idx == idx {
            return;
        }

        let current_x = self.view_pos();
        self.animate_view_offset_to_column(current_x, idx);

        self.active_column_idx = idx;

        // A different column was activated; reset the flag.
        self.activate_prev_column_on_removal = false;
    }

    fn has_windows(&self) -> bool {
        self.windows().next().is_some()
    }

    fn has_window(&self, window: &W) -> bool {
        self.windows().any(|win| win == window)
    }

    fn find_wl_surface(&self, wl_surface: &WlSurface) -> Option<&W> {
        self.windows().find(|win| win.is_wl_surface(wl_surface))
    }

    /// Computes the X position of the windows in the given column, in logical coordinates.
    fn column_x(&self, column_idx: usize) -> i32 {
        let mut x = 0;

        for column in self.columns.iter().take(column_idx) {
            x += column.width() + self.options.gaps;
        }

        x
    }

    fn add_window(&mut self, window: W, activate: bool, width: ColumnWidth, is_full_width: bool) {
        self.enter_output_for_window(&window);

        let was_empty = self.columns.is_empty();

        let idx = if self.columns.is_empty() {
            0
        } else {
            self.active_column_idx + 1
        };

        let column = Column::new(
            window,
            self.view_size,
            self.working_area,
            self.options.clone(),
            width,
            is_full_width,
        );
        self.columns.insert(idx, column);

        if activate {
            // If this is the first window on an empty workspace, skip the animation from whatever
            // view_offset was left over.
            if was_empty {
                // Try to make the code produce a left-aligned offset, even in presence of left
                // exclusive zones.
                self.view_offset = self.compute_new_view_offset_for_column(self.column_x(0), 0);
                self.view_offset_anim = None;
            }

            self.activate_column(idx);
            self.activate_prev_column_on_removal = true;
        }
    }

    fn remove_window(&mut self, window: &W) {
        if let Some(output) = &self.output {
            window.output_leave(output);
        }

        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();
        let column = &mut self.columns[column_idx];

        let window_idx = column.windows.iter().position(|win| win == window).unwrap();
        column.windows.remove(window_idx);
        column.heights.remove(window_idx);
        if column.windows.is_empty() {
            if column_idx + 1 == self.active_column_idx {
                // The previous column, that we were going to activate upon removal of the active
                // column, has just been itself removed.
                self.activate_prev_column_on_removal = false;
            }

            // FIXME: activate_column below computes current view position to compute the new view
            // position, which can include the column we're removing here. This leads to unwanted
            // view jumps.
            self.columns.remove(column_idx);
            if self.columns.is_empty() {
                return;
            }

            if self.active_column_idx > column_idx
                || (self.active_column_idx == column_idx && self.activate_prev_column_on_removal)
            {
                // A column to the left was removed; preserve the current position.
                // FIXME: preserve activate_prev_column_on_removal.
                // Or, the active column was removed, and we needed to activate the previous column.
                self.activate_column(self.active_column_idx.saturating_sub(1));
            } else {
                self.activate_column(min(self.active_column_idx, self.columns.len() - 1));
            }

            return;
        }

        column.active_window_idx = min(column.active_window_idx, column.windows.len() - 1);
        column.update_window_sizes();
    }

    fn update_window(&mut self, window: &W) {
        let (idx, column) = self
            .columns
            .iter_mut()
            .enumerate()
            .find(|(_, col)| col.contains(window))
            .unwrap();
        column.update_window_sizes();

        if idx == self.active_column_idx {
            // We might need to move the view to ensure the resized window is still visible.
            let current_x = self.view_pos();
            self.animate_view_offset_to_column(current_x, idx);
        }
    }

    fn activate_window(&mut self, window: &W) {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.contains(window))
            .unwrap();
        let column = &mut self.columns[column_idx];

        column.activate_window(window);
        self.activate_column(column_idx);
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        assert!(self.view_size.w > 0);
        assert!(self.view_size.h > 0);

        if !self.columns.is_empty() {
            assert!(self.active_column_idx < self.columns.len());

            for column in &self.columns {
                column.verify_invariants();
            }
        }
    }

    fn focus_left(&mut self) {
        self.activate_column(self.active_column_idx.saturating_sub(1));
    }

    fn focus_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.activate_column(min(self.active_column_idx + 1, self.columns.len() - 1));
    }

    fn focus_down(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].focus_down();
    }

    fn focus_up(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].focus_up();
    }

    fn move_left(&mut self) {
        let new_idx = self.active_column_idx.saturating_sub(1);
        if self.active_column_idx == new_idx {
            return;
        }

        let current_x = self.view_pos();

        self.columns.swap(self.active_column_idx, new_idx);

        self.view_offset =
            self.compute_new_view_offset_for_column(current_x, self.active_column_idx);

        self.activate_column(new_idx);
    }

    fn move_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let new_idx = min(self.active_column_idx + 1, self.columns.len() - 1);
        if self.active_column_idx == new_idx {
            return;
        }

        let current_x = self.view_pos();

        self.columns.swap(self.active_column_idx, new_idx);

        self.view_offset =
            self.compute_new_view_offset_for_column(current_x, self.active_column_idx);

        self.activate_column(new_idx);
    }

    fn move_down(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].move_down();
    }

    fn move_up(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].move_up();
    }

    fn consume_into_column(&mut self) {
        if self.columns.len() < 2 {
            return;
        }

        if self.active_column_idx == self.columns.len() - 1 {
            return;
        }

        let source_column_idx = self.active_column_idx + 1;

        let source_column = &mut self.columns[source_column_idx];
        let window = source_column.windows[0].clone();
        self.remove_window(&window);

        let target_column = &mut self.columns[self.active_column_idx];
        target_column.add_window(window);
    }

    fn expel_from_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let source_column = &mut self.columns[self.active_column_idx];
        if source_column.windows.len() == 1 {
            return;
        }

        let width = source_column.width;
        let is_full_width = source_column.is_full_width;
        let window = source_column.windows[source_column.active_window_idx].clone();
        self.remove_window(&window);

        self.add_window(window, true, width, is_full_width);
    }

    fn center_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let col = &self.columns[self.active_column_idx];
        if col.is_fullscreen {
            return;
        }

        let width = col.width();

        // If the column is wider than the working area, then on commit it will be shifted to left
        // edge alignment by the usual positioning code, so there's no use in doing anything here.
        if self.working_area.size.w <= width {
            return;
        }

        let new_view_offset = -(self.working_area.size.w - width) / 2 - self.working_area.loc.x;

        // If we're already animating towards that, don't restart it.
        if let Some(anim) = &self.view_offset_anim {
            if anim.to().round() as i32 == new_view_offset {
                return;
            }
        }

        // If our view offset is already this, we don't need to do anything.
        if self.view_offset == new_view_offset {
            return;
        }

        self.view_offset_anim = Some(Animation::new(
            self.view_offset as f64,
            new_view_offset as f64,
            Duration::from_millis(250),
        ));
    }

    fn view_pos(&self) -> i32 {
        self.column_x(self.active_column_idx) + self.view_offset
    }

    fn window_under(&self, pos: Point<f64, Logical>) -> Option<(&W, Point<i32, Logical>)> {
        if self.columns.is_empty() {
            return None;
        }

        let view_pos = self.view_pos();

        // Prefer the active window since it's drawn on top.
        let col = &self.columns[self.active_column_idx];
        let active_win = &col.windows[col.active_window_idx];
        let geom = active_win.geometry();
        let buf_pos = Point::from((
            self.column_x(self.active_column_idx) - view_pos,
            col.window_y(col.active_window_idx),
        )) - geom.loc;
        if active_win.is_in_input_region(&(pos - buf_pos.to_f64())) {
            return Some((active_win, buf_pos));
        }

        let mut x = -view_pos;
        for col in &self.columns {
            for (win, y) in zip(&col.windows, col.window_ys()) {
                if win == active_win {
                    // Already handled it above.
                    continue;
                }

                let geom = win.geometry();
                let buf_pos = Point::from((x, y)) - geom.loc;
                if win.is_in_input_region(&(pos - buf_pos.to_f64())) {
                    return Some((win, buf_pos));
                }
            }

            x += col.width() + self.options.gaps;
        }

        None
    }

    fn toggle_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].toggle_width();
    }

    fn toggle_full_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].toggle_full_width();
    }

    fn set_column_width(&mut self, change: SizeChange) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].set_column_width(change);
    }

    fn set_window_height(&mut self, change: SizeChange) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].set_window_height(change);
    }

    pub fn set_fullscreen(&mut self, window: &W, is_fullscreen: bool) {
        let (mut col_idx, win_idx) = self
            .columns
            .iter()
            .enumerate()
            .find_map(|(col_idx, col)| {
                col.windows
                    .iter()
                    .position(|w| w == window)
                    .map(|win_idx| (col_idx, win_idx))
            })
            .unwrap();

        let mut col = &mut self.columns[col_idx];

        if is_fullscreen && col.windows.len() > 1 {
            // This wasn't the only window in its column; extract it into a separate column.
            let target_window_was_focused =
                self.active_column_idx == col_idx && col.active_window_idx == win_idx;
            let window = col.windows.remove(win_idx);
            col.heights.remove(win_idx);
            col.active_window_idx = min(col.active_window_idx, col.windows.len() - 1);
            col.update_window_sizes();
            let width = col.width;
            let is_full_width = col.is_full_width;

            col_idx += 1;
            self.columns.insert(
                col_idx,
                Column::new(
                    window,
                    self.view_size,
                    self.working_area,
                    self.options.clone(),
                    width,
                    is_full_width,
                ),
            );
            if self.active_column_idx >= col_idx || target_window_was_focused {
                self.active_column_idx += 1;
            }
            col = &mut self.columns[col_idx];
        }

        col.set_fullscreen(is_fullscreen);
    }

    pub fn toggle_fullscreen(&mut self, window: &W) {
        let col = self
            .columns
            .iter_mut()
            .find(|col| col.windows.contains(window))
            .unwrap();
        let value = !col.is_fullscreen;
        self.set_fullscreen(window, value);
    }

    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer if we're on a fullscreen window and the view is stationary.
        if self.columns.is_empty() {
            return false;
        }

        if self.view_offset_anim.is_some() {
            return false;
        }

        self.columns[self.active_column_idx].is_fullscreen
    }
}

impl Workspace<Window> {
    fn refresh(&self) {
        let bounds = self.toplevel_bounds();

        for (col_idx, col) in self.columns.iter().enumerate() {
            for (win_idx, win) in col.windows.iter().enumerate() {
                let active = self.active_column_idx == col_idx && col.active_window_idx == win_idx;
                win.set_activated(active);

                win.toplevel().with_pending_state(|state| {
                    state.bounds = Some(bounds);
                });

                win.toplevel().send_pending_configure();
                win.refresh();
            }
        }
    }

    pub fn render_elements(
        &self,
        renderer: &mut GlesRenderer,
    ) -> Vec<WorkspaceRenderElement<GlesRenderer>> {
        if self.columns.is_empty() {
            return vec![];
        }

        // FIXME: workspaces should probably cache their last used scale so they can be correctly
        // rendered even with no outputs connected.
        let output_scale = self
            .output
            .as_ref()
            .map(|o| Scale::from(o.current_scale().fractional_scale()))
            .unwrap_or(Scale::from(1.));

        let mut rv = vec![];
        let view_pos = self.view_pos();

        // Draw the active window on top.
        let col = &self.columns[self.active_column_idx];
        let active_win = &col.windows[col.active_window_idx];
        let win_pos = Point::from((
            self.column_x(self.active_column_idx) - view_pos,
            col.window_y(col.active_window_idx),
        ));

        // Draw the window itself.
        let geom = active_win.geometry();
        let buf_pos = win_pos - geom.loc;
        rv.extend(active_win.render_elements(
            renderer,
            buf_pos.to_physical_precise_round(output_scale),
            output_scale,
            1.,
        ));

        // Draw the focus ring.
        rv.extend(self.focus_ring.render(output_scale).map(Into::into));

        let mut x = -view_pos;
        for col in &self.columns {
            for (win, y) in zip(&col.windows, col.window_ys()) {
                if win == active_win {
                    // Already handled it above.
                    continue;
                }

                let geom = win.geometry();
                let buf_pos = Point::from((x, y)) - geom.loc;
                rv.extend(win.render_elements(
                    renderer,
                    buf_pos.to_physical_precise_round(output_scale),
                    output_scale,
                    1.,
                ));
            }

            x += col.width() + self.options.gaps;
        }

        rv
    }
}

impl<W: LayoutElement> Column<W> {
    fn new(
        window: W,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
        options: Rc<Options>,
        width: ColumnWidth,
        is_full_width: bool,
    ) -> Self {
        let mut rv = Self {
            windows: vec![],
            heights: vec![],
            active_window_idx: 0,
            width,
            is_full_width,
            is_fullscreen: false,
            view_size,
            working_area,
            options,
        };

        rv.add_window(window);

        rv
    }

    fn set_view_size(&mut self, size: Size<i32, Logical>, working_area: Rectangle<i32, Logical>) {
        if self.view_size == size && self.working_area == working_area {
            return;
        }

        self.view_size = size;
        self.working_area = working_area;

        self.update_window_sizes();
    }

    fn update_config(&mut self, options: Rc<Options>) {
        let mut update_sizes = false;

        // If preset widths changed, make our width non-preset.
        if self.options.preset_widths != options.preset_widths {
            if let ColumnWidth::Preset(idx) = self.width {
                self.width = self.options.preset_widths[idx];
            }
        }

        if self.options.gaps != options.gaps {
            update_sizes = true;
        }

        self.options = options;

        if update_sizes {
            self.update_window_sizes();
        }
    }

    fn set_width(&mut self, width: ColumnWidth) {
        self.width = width;
        self.is_full_width = false;
        self.update_window_sizes();
    }

    fn contains(&self, window: &W) -> bool {
        self.windows.iter().any(|win| win == window)
    }

    fn activate_window(&mut self, window: &W) {
        let idx = self.windows.iter().position(|win| win == window).unwrap();
        self.active_window_idx = idx;
    }

    fn add_window(&mut self, window: W) {
        self.is_fullscreen = false;
        self.windows.push(window);
        self.heights.push(WindowHeight::Auto);
        self.update_window_sizes();
    }

    fn update_window_sizes(&mut self) {
        if self.is_fullscreen {
            self.windows[0].request_fullscreen(self.view_size);
            return;
        }

        let min_size: Vec<_> = self.windows.iter().map(LayoutElement::min_size).collect();
        let max_size: Vec<_> = self.windows.iter().map(LayoutElement::max_size).collect();

        // Compute the column width.
        let min_width = min_size
            .iter()
            .filter_map(|size| {
                let w = size.w;
                if w == 0 {
                    None
                } else {
                    Some(w)
                }
            })
            .max()
            .unwrap_or(1);
        let max_width = max_size
            .iter()
            .filter_map(|size| {
                let w = size.w;
                if w == 0 {
                    None
                } else {
                    Some(w)
                }
            })
            .min()
            .unwrap_or(i32::MAX);
        let max_width = max(max_width, min_width);

        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let width = width.resolve(&self.options, self.working_area.size.w);
        let width = max(min(width, max_width), min_width);

        // Compute the window heights.
        let mut heights = self.heights.clone();
        let mut height_left = self.working_area.size.h - self.options.gaps;
        let mut auto_windows_left = self.windows.len();

        // Subtract all fixed-height windows.
        for (h, (min_size, max_size)) in zip(&mut heights, zip(&min_size, &max_size)) {
            // Check if the window has an exact height constraint.
            if min_size.h > 0 && min_size.h == max_size.h {
                *h = WindowHeight::Fixed(min_size.h);
            }

            if let WindowHeight::Fixed(h) = h {
                if max_size.h > 0 {
                    *h = min(*h, max_size.h);
                }
                if min_size.h > 0 {
                    *h = max(*h, min_size.h);
                }
                *h = max(*h, 1);

                height_left -= *h + self.options.gaps;
                auto_windows_left -= 1;
            }
        }

        // Iteratively try to distribute the remaining height, checking against window min heights.
        // Pick an auto height according to the current sizes, then check if it satisfies all
        // remaining min heights. If not, allocate fixed height to those windows and repeat the
        // loop. On each iteration the auto height will get smaller.
        //
        // NOTE: we do not respect max height here. Doing so would complicate things: if the current
        // auto height is above some window's max height, then the auto height can become larger.
        // Combining this with the min height loop is where the complexity appears.
        //
        // However, most max height uses are for fixed-size dialogs, where min height == max_height.
        // This case is separately handled above.
        while auto_windows_left > 0 {
            // Compute the current auto height.
            let auto_height = height_left / auto_windows_left as i32 - self.options.gaps;
            let auto_height = max(auto_height, 1);

            // Integer division above can result in imperfect height distribution. We will make some
            // windows 1 px taller to account for this.
            let mut ones_left = height_left
                .saturating_sub((auto_height + self.options.gaps) * auto_windows_left as i32);

            let mut unsatisfied_min = false;
            let mut ones_left_2 = ones_left;
            for (h, min_size) in zip(&mut heights, &min_size) {
                if matches!(h, WindowHeight::Fixed(_)) {
                    continue;
                }

                let mut auto = auto_height;
                if ones_left_2 > 0 {
                    auto += 1;
                    ones_left_2 -= 1;
                }

                // Check if the auto height satisfies the min height.
                if min_size.h > 0 && min_size.h > auto {
                    *h = WindowHeight::Fixed(min_size.h);
                    height_left -= min_size.h + self.options.gaps;
                    auto_windows_left -= 1;
                    unsatisfied_min = true;
                }
            }

            // If some min height was unsatisfied, then we allocated the window more than the auto
            // height, which means that the remaining auto windows now have less height to work
            // with, and the loop must run again.
            if unsatisfied_min {
                continue;
            }

            // All min heights were satisfied, fill them in.
            for h in &mut heights {
                if matches!(h, WindowHeight::Fixed(_)) {
                    continue;
                }

                let mut auto = auto_height;
                if ones_left > 0 {
                    auto += 1;
                    ones_left -= 1;
                }

                *h = WindowHeight::Fixed(auto);
                auto_windows_left -= 1;
            }

            assert_eq!(auto_windows_left, 0);
        }

        for (win, h) in zip(&self.windows, heights) {
            let WindowHeight::Fixed(height) = h else {
                unreachable!()
            };

            let size = Size::from((width, height));
            win.request_size(size);
        }
    }

    fn width(&self) -> i32 {
        self.windows
            .iter()
            .map(|win| win.geometry().size.w)
            .max()
            .unwrap()
    }

    fn focus_up(&mut self) {
        self.active_window_idx = self.active_window_idx.saturating_sub(1);
    }

    fn focus_down(&mut self) {
        self.active_window_idx = min(self.active_window_idx + 1, self.windows.len() - 1);
    }

    fn move_up(&mut self) {
        let new_idx = self.active_window_idx.saturating_sub(1);
        if self.active_window_idx == new_idx {
            return;
        }

        self.windows.swap(self.active_window_idx, new_idx);
        self.heights.swap(self.active_window_idx, new_idx);
        self.active_window_idx = new_idx;
    }

    fn move_down(&mut self) {
        let new_idx = min(self.active_window_idx + 1, self.windows.len() - 1);
        if self.active_window_idx == new_idx {
            return;
        }

        self.windows.swap(self.active_window_idx, new_idx);
        self.heights.swap(self.active_window_idx, new_idx);
        self.active_window_idx = new_idx;
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        assert!(!self.windows.is_empty(), "columns can't be empty");
        assert!(self.active_window_idx < self.windows.len());
        assert_eq!(self.windows.len(), self.heights.len());

        if self.is_fullscreen {
            assert_eq!(self.windows.len(), 1);
        }
    }

    fn toggle_width(&mut self) {
        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let idx = match width {
            ColumnWidth::Preset(idx) => (idx + 1) % self.options.preset_widths.len(),
            _ => {
                let current = self.width();
                self.options
                    .preset_widths
                    .iter()
                    .position(|prop| {
                        prop.resolve(&self.options, self.working_area.size.w) > current
                    })
                    .unwrap_or(0)
            }
        };
        let width = ColumnWidth::Preset(idx);
        self.set_width(width);
    }

    fn toggle_full_width(&mut self) {
        self.is_full_width = !self.is_full_width;
        self.update_window_sizes();
    }

    fn set_column_width(&mut self, change: SizeChange) {
        let width = if self.is_full_width {
            ColumnWidth::Proportion(1.)
        } else {
            self.width
        };

        let current_px = width.resolve(&self.options, self.working_area.size.w);

        let current = match width {
            ColumnWidth::Preset(idx) => self.options.preset_widths[idx],
            current => current,
        };

        // FIXME: fix overflows then remove limits.
        const MAX_PX: i32 = 100000;
        const MAX_F: f64 = 10000.;

        let width = match (current, change) {
            (_, SizeChange::SetFixed(fixed)) => ColumnWidth::Fixed(fixed.clamp(1, MAX_PX)),
            (_, SizeChange::SetProportion(proportion)) => {
                ColumnWidth::Proportion((proportion / 100.).clamp(0., MAX_F))
            }
            (_, SizeChange::AdjustFixed(delta)) => {
                let width = current_px.saturating_add(delta).clamp(1, MAX_PX);
                ColumnWidth::Fixed(width)
            }
            (ColumnWidth::Proportion(current), SizeChange::AdjustProportion(delta)) => {
                let proportion = (current + delta / 100.).clamp(0., MAX_F);
                ColumnWidth::Proportion(proportion)
            }
            (ColumnWidth::Fixed(_), SizeChange::AdjustProportion(delta)) => {
                let current = (current_px + self.options.gaps) as f64
                    / (self.working_area.size.w - self.options.gaps) as f64;
                let proportion = (current + delta / 100.).clamp(0., MAX_F);
                ColumnWidth::Proportion(proportion)
            }
            (ColumnWidth::Preset(_), _) => unreachable!(),
        };

        self.set_width(width);
    }

    fn set_window_height(&mut self, change: SizeChange) {
        let current = self.heights[self.active_window_idx];
        let current_px = match current {
            WindowHeight::Auto => self.windows[self.active_window_idx].geometry().size.h,
            WindowHeight::Fixed(height) => height,
        };
        let current_prop = (current_px + self.options.gaps) as f64
            / (self.working_area.size.h - self.options.gaps) as f64;

        // FIXME: fix overflows then remove limits.
        const MAX_PX: i32 = 100000;

        let mut height = match change {
            SizeChange::SetFixed(fixed) => fixed,
            SizeChange::SetProportion(proportion) => {
                ((self.working_area.size.h - self.options.gaps) as f64 * proportion
                    - self.options.gaps as f64)
                    .round() as i32
            }
            SizeChange::AdjustFixed(delta) => current_px.saturating_add(delta),
            SizeChange::AdjustProportion(delta) => {
                let proportion = current_prop + delta / 100.;
                ((self.working_area.size.h - self.options.gaps) as f64 * proportion
                    - self.options.gaps as f64)
                    .round() as i32
            }
        };

        // Clamp it against the window height constraints.
        let win = &self.windows[self.active_window_idx];
        let min_h = win.min_size().h;
        let max_h = win.max_size().h;

        if max_h > 0 {
            height = height.min(max_h);
        }
        if min_h > 0 {
            height = height.max(min_h);
        }

        self.heights[self.active_window_idx] = WindowHeight::Fixed(height.clamp(1, MAX_PX));
        self.update_window_sizes();
    }

    fn set_fullscreen(&mut self, is_fullscreen: bool) {
        assert_eq!(self.windows.len(), 1);
        self.is_fullscreen = is_fullscreen;
        self.update_window_sizes();
    }

    fn window_y(&self, window_idx: usize) -> i32 {
        self.window_ys().nth(window_idx).unwrap()
    }

    fn window_ys(&self) -> impl Iterator<Item = i32> + '_ {
        let mut y = 0;

        if !self.is_fullscreen {
            y = self.working_area.loc.y + self.options.gaps;
        }

        self.windows.iter().map(move |win| {
            let pos = y;
            y += win.geometry().size.h + self.options.gaps;
            pos
        })
    }
}

pub fn output_size(output: &Output) -> Size<i32, Logical> {
    let output_scale = output.current_scale().integer_scale();
    let output_transform = output.current_transform();
    let output_mode = output.current_mode().unwrap();

    output_transform
        .transform_size(output_mode.size)
        .to_logical(output_scale)
}

fn compute_new_view_offset(
    cur_x: i32,
    view_width: i32,
    new_col_x: i32,
    new_col_width: i32,
    gaps: i32,
) -> i32 {
    // If the column is wider than the view, always left-align it.
    if view_width <= new_col_width {
        return 0;
    }

    // Compute the padding in case it needs to be smaller due to large window width.
    let padding = ((view_width - new_col_width) / 2).clamp(0, gaps);

    // Compute the desired new X with padding.
    let new_x = new_col_x - padding;
    let new_right_x = new_col_x + new_col_width + padding;

    // If the column is already fully visible, leave the view as is.
    if cur_x <= new_x && new_right_x <= cur_x + view_width {
        return -(new_col_x - cur_x);
    }

    // Otherwise, prefer the aligment that results in less motion from the current position.
    let dist_to_left = cur_x.abs_diff(new_x);
    let dist_to_right = (cur_x + view_width).abs_diff(new_right_x);
    if dist_to_left <= dist_to_right {
        -padding
    } else {
        -(view_width - padding - new_col_width)
    }
}

fn prepare_for_output(window: &impl LayoutElement, output: &Output) {
    let scale = output.current_scale().integer_scale();
    let transform = output.current_transform();
    window.set_preferred_scale_transform(scale, transform);
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use proptest::prelude::*;
    use proptest_derive::Arbitrary;
    use smithay::output::{Mode, PhysicalProperties, Subpixel};
    use smithay::utils::IsAlive;

    use super::*;

    impl<W: LayoutElement> Default for Layout<W> {
        fn default() -> Self {
            Self {
                monitor_set: MonitorSet::NoOutputs { workspaces: vec![] },
                options: Rc::new(Options::default()),
            }
        }
    }

    #[derive(Debug)]
    struct TestWindowInner {
        id: usize,
        bbox: Cell<Rectangle<i32, Logical>>,
        initial_bbox: Rectangle<i32, Logical>,
        requested_size: Cell<Option<Size<i32, Logical>>>,
    }

    #[derive(Debug, Clone)]
    struct TestWindow(Rc<TestWindowInner>);

    impl TestWindow {
        fn new(id: usize, bbox: Rectangle<i32, Logical>) -> Self {
            Self(Rc::new(TestWindowInner {
                id,
                bbox: Cell::new(bbox),
                initial_bbox: bbox,
                requested_size: Cell::new(None),
            }))
        }

        fn communicate(&self) -> bool {
            if let Some(size) = self.0.requested_size.take() {
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

    impl PartialEq for TestWindow {
        fn eq(&self, other: &Self) -> bool {
            self.0.id == other.0.id
        }
    }

    impl IsAlive for TestWindow {
        fn alive(&self) -> bool {
            true
        }
    }

    impl SpaceElement for TestWindow {
        fn bbox(&self) -> Rectangle<i32, Logical> {
            self.0.bbox.get()
        }

        fn is_in_input_region(&self, _point: &Point<f64, Logical>) -> bool {
            false
        }

        fn set_activate(&self, _activated: bool) {}
        fn output_enter(&self, _output: &Output, _overlap: Rectangle<i32, Logical>) {}
        fn output_leave(&self, _output: &Output) {}
    }

    impl LayoutElement for TestWindow {
        fn request_size(&self, size: Size<i32, Logical>) {
            self.0.requested_size.set(Some(size));
        }

        fn request_fullscreen(&self, _size: Size<i32, Logical>) {}

        fn min_size(&self) -> Size<i32, Logical> {
            Size::from((0, 0))
        }

        fn max_size(&self) -> Size<i32, Logical> {
            Size::from((0, 0))
        }

        fn is_wl_surface(&self, _wl_surface: &WlSurface) -> bool {
            false
        }

        fn set_preferred_scale_transform(&self, _scale: i32, _transform: Transform) {}

        fn has_ssd(&self) -> bool {
            false
        }
    }

    fn arbitrary_bbox() -> impl Strategy<Value = Rectangle<i32, Logical>> {
        any::<(i16, i16, u16, u16)>().prop_map(|(x, y, w, h)| {
            let loc: Point<i32, _> = Point::from((x.into(), y.into()));
            let size: Size<i32, _> = Size::from((w.into(), h.into()));
            Rectangle::from_loc_and_size(loc, size)
        })
    }

    fn arbitrary_size_change() -> impl Strategy<Value = SizeChange> {
        prop_oneof![
            (0..).prop_map(SizeChange::SetFixed),
            (0f64..).prop_map(SizeChange::SetProportion),
            any::<i32>().prop_map(SizeChange::AdjustFixed),
            any::<f64>().prop_map(SizeChange::AdjustProportion),
        ]
    }

    #[derive(Debug, Clone, Copy, Arbitrary)]
    enum Op {
        AddOutput(#[proptest(strategy = "1..=5usize")] usize),
        RemoveOutput(#[proptest(strategy = "1..=5usize")] usize),
        FocusOutput(#[proptest(strategy = "1..=5usize")] usize),
        AddWindow {
            #[proptest(strategy = "1..=5usize")]
            id: usize,
            #[proptest(strategy = "arbitrary_bbox()")]
            bbox: Rectangle<i32, Logical>,
            activate: bool,
        },
        CloseWindow(#[proptest(strategy = "1..=5usize")] usize),
        FullscreenWindow(#[proptest(strategy = "1..=5usize")] usize),
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
        CenterColumn,
        FocusWorkspaceDown,
        FocusWorkspaceUp,
        FocusWorkspace(#[proptest(strategy = "1..=5u8")] u8),
        MoveWindowToWorkspaceDown,
        MoveWindowToWorkspaceUp,
        MoveWindowToWorkspace(#[proptest(strategy = "1..=5u8")] u8),
        MoveWorkspaceDown,
        MoveWorkspaceUp,
        MoveWindowToOutput(#[proptest(strategy = "1..=5u8")] u8),
        SwitchPresetColumnWidth,
        MaximizeColumn,
        SetColumnWidth(#[proptest(strategy = "arbitrary_size_change()")] SizeChange),
        SetWindowHeight(#[proptest(strategy = "arbitrary_size_change()")] SizeChange),
        Communicate(#[proptest(strategy = "1..=5usize")] usize),
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
                        name,
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
                            refresh: 60,
                        }),
                        None,
                        None,
                        None,
                    );
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
                Op::AddWindow { id, bbox, activate } => {
                    match &mut layout.monitor_set {
                        MonitorSet::Normal { monitors, .. } => {
                            for mon in monitors {
                                for ws in &mut mon.workspaces {
                                    for win in ws.windows() {
                                        if win.0.id == id {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        MonitorSet::NoOutputs { workspaces, .. } => {
                            for ws in workspaces {
                                for win in ws.windows() {
                                    if win.0.id == id {
                                        return;
                                    }
                                }
                            }
                        }
                    }

                    let win = TestWindow::new(id, bbox);
                    layout.add_window(win, activate, None, false);
                }
                Op::CloseWindow(id) => {
                    let dummy = TestWindow::new(id, Rectangle::default());
                    layout.remove_window(&dummy);
                }
                Op::FullscreenWindow(id) => {
                    let dummy = TestWindow::new(id, Rectangle::default());
                    layout.toggle_fullscreen(&dummy);
                }
                Op::FocusColumnLeft => layout.focus_left(),
                Op::FocusColumnRight => layout.focus_right(),
                Op::FocusWindowDown => layout.focus_down(),
                Op::FocusWindowUp => layout.focus_up(),
                Op::MoveColumnLeft => layout.move_left(),
                Op::MoveColumnRight => layout.move_right(),
                Op::MoveWindowDown => layout.move_down(),
                Op::MoveWindowUp => layout.move_up(),
                Op::ConsumeWindowIntoColumn => layout.consume_into_column(),
                Op::ExpelWindowFromColumn => layout.expel_from_column(),
                Op::CenterColumn => layout.center_column(),
                Op::FocusWorkspaceDown => layout.switch_workspace_down(),
                Op::FocusWorkspaceUp => layout.switch_workspace_up(),
                Op::FocusWorkspace(idx) => layout.switch_workspace(idx),
                Op::MoveWindowToWorkspaceDown => layout.move_to_workspace_down(),
                Op::MoveWindowToWorkspaceUp => layout.move_to_workspace_up(),
                Op::MoveWindowToWorkspace(idx) => layout.move_to_workspace(idx),
                Op::MoveWindowToOutput(id) => {
                    let name = format!("output{id}");
                    let Some(output) = layout.outputs().find(|o| o.name() == name).cloned() else {
                        return;
                    };

                    layout.move_to_output(&output);
                }
                Op::MoveWorkspaceDown => layout.move_workspace_down(),
                Op::MoveWorkspaceUp => layout.move_workspace_up(),
                Op::SwitchPresetColumnWidth => layout.toggle_width(),
                Op::MaximizeColumn => layout.toggle_full_width(),
                Op::SetColumnWidth(change) => layout.set_column_width(change),
                Op::SetWindowHeight(change) => layout.set_window_height(change),
                Op::Communicate(id) => {
                    let mut window = None;
                    match &mut layout.monitor_set {
                        MonitorSet::Normal { monitors, .. } => {
                            'outer: for mon in monitors {
                                for ws in &mut mon.workspaces {
                                    for win in ws.windows() {
                                        if win.0.id == id {
                                            if win.communicate() {
                                                window = Some(win.clone());
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
                                            window = Some(win.clone());
                                        }
                                        break 'outer;
                                    }
                                }
                            }
                        }
                    }

                    if let Some(win) = window {
                        layout.update_window(&win);
                    }
                }
            }
        }
    }

    #[track_caller]
    fn check_ops(ops: &[Op]) {
        let mut layout = Layout::default();
        for op in ops {
            op.apply(&mut layout);
            layout.verify_invariants();
        }
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
            Op::AddWindow {
                id: 0,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::CloseWindow(0),
            Op::CloseWindow(1),
            Op::CloseWindow(2),
            Op::FocusColumnLeft,
            Op::FocusColumnRight,
            Op::MoveColumnLeft,
            Op::MoveColumnRight,
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
            Op::MoveWindowToWorkspace(1),
            Op::MoveWindowToWorkspace(2),
            Op::MoveWindowToWorkspace(3),
            Op::MoveWindowDown,
            Op::MoveWindowUp,
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
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::MoveWindowToWorkspaceDown,
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddWindow {
                id: 3,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::FocusColumnLeft,
            Op::ConsumeWindowIntoColumn,
            Op::AddWindow {
                id: 4,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddOutput(2),
            Op::AddWindow {
                id: 5,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::MoveWindowToOutput(2),
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
            Op::AddWindow {
                id: 0,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::CloseWindow(0),
            Op::CloseWindow(1),
            Op::CloseWindow(2),
            Op::FocusColumnLeft,
            Op::FocusColumnRight,
            Op::MoveColumnLeft,
            Op::MoveColumnRight,
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
            Op::MoveWindowToWorkspace(1),
            Op::MoveWindowToWorkspace(2),
            Op::MoveWindowToWorkspace(3),
            Op::MoveWindowDown,
            Op::MoveWindowUp,
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
                id: 0,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::FocusOutput(2),
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
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
                id: 0,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
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
                id: 0,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddOutput(2),
            Op::RemoveOutput(1),
        ];

        let mut layout = Layout::default();
        for op in ops {
            op.apply(&mut layout);
        }

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
                id: 0,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::MoveWindowToWorkspace(2),
        ];

        let mut layout = Layout::default();
        for op in ops {
            op.apply(&mut layout);
        }

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        assert!(monitors[0].workspaces[0].has_windows());
    }

    #[test]
    fn focus_workspace_by_idx_does_not_leave_empty_workspaces() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                id: 0,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::FocusWorkspaceUp,
            Op::CloseWindow(0),
            Op::FocusWorkspace(3),
        ];

        let mut layout = Layout::default();
        for op in ops {
            op.apply(&mut layout);
        }

        let MonitorSet::Normal { monitors, .. } = layout.monitor_set else {
            unreachable!()
        };

        assert!(monitors[0].workspaces[0].has_windows());
    }

    #[test]
    fn empty_workspaces_dont_move_back_to_original_output() {
        let ops = [
            Op::AddOutput(1),
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
                activate: true,
            },
            Op::AddOutput(2),
            Op::RemoveOutput(1),
            Op::FocusWorkspace(1),
            Op::CloseWindow(1),
            Op::AddOutput(1),
        ];

        check_ops(&ops);
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
        fn random_operations_dont_panic(ops: Vec<Op>) {
            // eprintln!("{ops:?}");
            check_ops(&ops);
        }
    }
}
