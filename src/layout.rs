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
use std::mem;
use std::time::Duration;

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
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::render_elements;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};
use smithay::wayland::compositor::{with_states, SurfaceData};
use smithay::wayland::dmabuf::DmabufFeedback;
use smithay::wayland::shell::xdg::SurfaceCachedState;

use crate::animation::Animation;
use crate::config::{Color, Config};

const PADDING: i32 = 16;
const WIDTH_PROPORTIONS: [ColumnWidth; 3] = [
    ColumnWidth::Proportion(1. / 3.),
    ColumnWidth::Proportion(0.5),
    ColumnWidth::Proportion(2. / 3.),
];

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
    fn send_frame<T, F>(
        &self,
        output: &Output,
        time: T,
        throttle: Option<Duration>,
        primary_scan_out_output: F,
    ) where
        T: Into<Duration>,
        F: FnMut(&WlSurface, &SurfaceData) -> Option<Output> + Copy;
    fn send_dmabuf_feedback<'a, P, F>(
        &self,
        output: &Output,
        primary_scan_out_output: P,
        select_dmabuf_feedback: F,
    ) where
        P: FnMut(&WlSurface, &SurfaceData) -> Option<Output> + Copy,
        F: Fn(&WlSurface, &SurfaceData) -> &'a DmabufFeedback + Copy;
}

#[derive(Debug)]
pub enum MonitorSet<W: LayoutElement> {
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
    NoOutputs(Vec<Workspace<W>>),
}

#[derive(Debug)]
pub struct Monitor<W: LayoutElement> {
    /// Output for this monitor.
    output: Output,
    // Must always contain at least one.
    workspaces: Vec<Workspace<W>>,
    /// Index of the currently active workspace.
    active_workspace_idx: usize,
    /// Animation for workspace switching.
    workspace_idx_anim: Option<Animation>,
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
}

#[derive(Debug)]
struct FocusRing {
    buffer: SolidColorBuffer,
    is_off: bool,
    width: i32,
    active_color: Color,
    inactive_color: Color,
}

/// Width of a column.
#[derive(Debug, Clone, Copy)]
enum ColumnWidth {
    /// Proportion of the current view width.
    Proportion(f64),
    /// One of the proportion presets.
    ///
    /// This is separate from Proportion in order to be able to reliably cycle between preset
    /// proportions.
    PresetProportion(usize),
    /// Fixed width in logical pixels.
    Fixed(i32),
}

#[derive(Debug)]
struct Column<W: LayoutElement> {
    /// Windows in this column.
    ///
    /// Must be non-empty.
    windows: Vec<W>,

    /// Index of the currently active window.
    active_window_idx: usize,

    /// Desired width of this column.
    width: ColumnWidth,

    /// Whether this column contains a single full-screened window.
    is_fullscreen: bool,
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

    fn send_frame<T, F>(
        &self,
        output: &Output,
        time: T,
        throttle: Option<Duration>,
        primary_scan_out_output: F,
    ) where
        T: Into<Duration>,
        F: FnMut(&WlSurface, &SurfaceData) -> Option<Output> + Copy,
    {
        self.send_frame(output, time, throttle, primary_scan_out_output);
    }

    fn send_dmabuf_feedback<'a, P, F>(
        &self,
        output: &Output,
        primary_scan_out_output: P,
        select_dmabuf_feedback: F,
    ) where
        P: FnMut(&WlSurface, &SurfaceData) -> Option<Output> + Copy,
        F: Fn(&WlSurface, &SurfaceData) -> &'a DmabufFeedback + Copy,
    {
        self.send_dmabuf_feedback(output, primary_scan_out_output, select_dmabuf_feedback);
    }
}

impl FocusRing {
    fn resize(&mut self, size: Size<i32, Logical>) {
        let size = size + Size::from((self.width * 2, self.width * 2));
        self.buffer.resize(size);
    }

    fn set_active(&mut self, is_active: bool) {
        self.buffer.set_color(if is_active {
            self.active_color.into()
        } else {
            self.inactive_color.into()
        });
    }

    fn render(
        &self,
        loc: Point<i32, Logical>,
        scale: Scale<f64>,
    ) -> Option<SolidColorRenderElement> {
        if self.is_off {
            return None;
        }

        let offset = Point::from((self.width, self.width));
        Some(SolidColorRenderElement::from_buffer(
            &self.buffer,
            (loc - offset).to_physical_precise_round(scale),
            scale,
            1.,
            Kind::Unspecified,
        ))
    }
}

impl Default for FocusRing {
    fn default() -> Self {
        Self {
            buffer: SolidColorBuffer::new((0, 0), [0., 0., 0., 0.]),
            is_off: true,
            width: 0,
            active_color: Color::default(),
            inactive_color: Color::default(),
        }
    }
}

impl ColumnWidth {
    fn resolve(self, view_width: i32) -> i32 {
        match self {
            ColumnWidth::Proportion(proportion) => (view_width as f64 * proportion).floor() as i32,
            ColumnWidth::PresetProportion(idx) => WIDTH_PROPORTIONS[idx].resolve(view_width),
            ColumnWidth::Fixed(width) => width,
        }
    }
}

impl Default for ColumnWidth {
    fn default() -> Self {
        Self::Proportion(0.5)
    }
}

impl<W: LayoutElement> MonitorSet<W> {
    pub fn new() -> Self {
        Self::NoOutputs(vec![])
    }

    pub fn add_output(&mut self, output: Output) {
        let id = OutputId::new(&output);

        *self = match mem::take(self) {
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
                        workspaces.push(ws);
                    }
                }
                workspaces.reverse();
                if workspaces.iter().all(|ws| ws.has_windows()) {
                    // Make sure there's always an empty workspace.
                    workspaces.push(Workspace::new(output.clone()));
                }

                for ws in &mut workspaces {
                    ws.set_output(Some(output.clone()));
                }

                monitors.push(Monitor::new(output, workspaces));
                MonitorSet::Normal {
                    monitors,
                    primary_idx,
                    active_monitor_idx,
                }
            }
            MonitorSet::NoOutputs(mut workspaces) => {
                // We know there are no empty workspaces there, so add one.
                workspaces.push(Workspace::new(output.clone()));

                for workspace in &mut workspaces {
                    workspace.set_output(Some(output.clone()));
                }

                let monitor = Monitor::new(output, workspaces);
                MonitorSet::Normal {
                    monitors: vec![monitor],
                    primary_idx: 0,
                    active_monitor_idx: 0,
                }
            }
        }
    }

    pub fn remove_output(&mut self, output: &Output) {
        *self = match mem::take(self) {
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
                    MonitorSet::NoOutputs(workspaces)
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

                    let empty = primary.workspaces.remove(primary.workspaces.len() - 1);
                    primary.workspaces.extend(workspaces);
                    primary.workspaces.push(empty);

                    MonitorSet::Normal {
                        monitors,
                        primary_idx,
                        active_monitor_idx,
                    }
                }
            }
            MonitorSet::NoOutputs(_) => {
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
    ) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = self
        else {
            panic!()
        };

        monitors[monitor_idx].add_window(workspace_idx, window, activate);

        if activate {
            *active_monitor_idx = monitor_idx;
        }
    }

    /// Adds a new window to the layout.
    ///
    /// Returns an output that the window was added to, if there were any outputs.
    pub fn add_window(&mut self, window: W, activate: bool) -> Option<&Output> {
        match self {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                let mon = &mut monitors[*active_monitor_idx];
                mon.add_window(mon.active_workspace_idx, window, activate);
                Some(&mon.output)
            }
            MonitorSet::NoOutputs(workspaces) => {
                let ws = if let Some(ws) = workspaces.get_mut(0) {
                    ws
                } else {
                    workspaces.push(Workspace::new_no_outputs());
                    &mut workspaces[0]
                };
                ws.add_window(window, activate);
                None
            }
        }
    }

    pub fn remove_window(&mut self, window: &W) {
        match self {
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
                            }

                            break;
                        }
                    }
                }
            }
            MonitorSet::NoOutputs(workspaces) => {
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
        match self {
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
            MonitorSet::NoOutputs(workspaces) => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.update_window(window);
                        return;
                    }
                }
            }
        }
    }

    pub fn send_frame(&self, output: &Output, time: Duration) {
        if let MonitorSet::Normal { monitors, .. } = self {
            for mon in monitors {
                if &mon.output == output {
                    mon.workspaces[mon.active_workspace_idx].send_frame(time);
                }
            }
        }
    }

    pub fn send_dmabuf_feedback(&self, output: &Output, feedback: &DmabufFeedback) {
        if let MonitorSet::Normal { monitors, .. } = self {
            for mon in monitors {
                if &mon.output == output {
                    mon.workspaces[mon.active_workspace_idx].send_dmabuf_feedback(feedback);
                }
            }
        }
    }

    pub fn find_window_and_output(&mut self, wl_surface: &WlSurface) -> Option<(W, Output)> {
        if let MonitorSet::Normal { monitors, .. } = self {
            for mon in monitors {
                for ws in &mut mon.workspaces {
                    if let Some(window) = ws.find_wl_surface(wl_surface) {
                        return Some((window.clone(), mon.output.clone()));
                    }
                }
            }
        }

        None
    }

    pub fn update_output_size(&mut self, output: &Output) {
        let MonitorSet::Normal { monitors, .. } = self else {
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
        } = self
        else {
            todo!()
        };

        for (monitor_idx, mon) in monitors.iter_mut().enumerate() {
            for (workspace_idx, ws) in mon.workspaces.iter_mut().enumerate() {
                if ws.has_window(window) {
                    *active_monitor_idx = monitor_idx;
                    // TODO
                    assert_eq!(mon.active_workspace_idx, workspace_idx);
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
        } = self
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
        } = self
        else {
            return None;
        };

        Some(&monitors[*active_monitor_idx].output)
    }

    pub fn workspace_for_output(&self, output: &Output) -> Option<&Workspace<W>> {
        let MonitorSet::Normal { monitors, .. } = self else {
            return None;
        };

        monitors.iter().find_map(|monitor| {
            if &monitor.output == output {
                Some(&monitor.workspaces[monitor.active_workspace_idx])
            } else {
                None
            }
        })
    }

    pub fn windows_for_output(&self, output: &Output) -> impl Iterator<Item = &W> + '_ {
        let MonitorSet::Normal { monitors, .. } = self else {
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
        } = self
        else {
            return None;
        };

        Some(&mut monitors[*active_monitor_idx])
    }

    pub fn monitor_for_output(&self, output: &Output) -> Option<&Monitor<W>> {
        let MonitorSet::Normal { monitors, .. } = self else {
            return None;
        };

        monitors.iter().find(|monitor| &monitor.output == output)
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> + '_ {
        let monitors = if let MonitorSet::Normal { monitors, .. } = self {
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

    pub fn focus(&self) -> Option<&W> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = self
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
        let ws = self.workspace_for_output(output).unwrap();
        ws.window_under(pos_within_output)
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        let (monitors, &primary_idx, &active_monitor_idx) = match &self {
            MonitorSet::Normal {
                monitors,
                primary_idx,
                active_monitor_idx,
            } => (monitors, primary_idx, active_monitor_idx),
            MonitorSet::NoOutputs(workspaces) => {
                for workspace in workspaces {
                    assert!(
                        workspace.has_windows(),
                        "with no outputs there cannot be empty workspaces"
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
                "monitor monitor must have at least one workspace"
            );
            assert!(monitor.active_workspace_idx < monitor.workspaces.len());

            let monitor_id = OutputId::new(&monitor.output);

            if idx == primary_idx {
            } else {
                assert!(
                    monitor
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.original_output == monitor_id),
                    "secondary monitor must have all own workspaces"
                );
            }

            // FIXME: verify that primary doesn't have any workspaces for which their own monitor
            // exists.

            for workspace in &monitor.workspaces {
                workspace.verify_invariants();
            }
        }
    }

    pub fn advance_animations(&mut self, current_time: Duration) {
        match self {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                for (idx, mon) in monitors.iter_mut().enumerate() {
                    mon.advance_animations(current_time, idx == *active_monitor_idx);
                }
            }
            MonitorSet::NoOutputs(workspaces) => {
                for ws in workspaces {
                    ws.advance_animations(current_time, false);
                }
            }
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        match self {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    mon.update_config(config);
                }
            }
            MonitorSet::NoOutputs(workspaces) => {
                for ws in workspaces {
                    ws.update_config(config);
                }
            }
        }
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

    pub fn focus_output(&mut self, output: &Output) {
        if let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = self
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
        } = self
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
            ws.remove_window(&window);

            let workspace_idx = monitors[new_idx].active_workspace_idx;
            self.add_window_by_idx(new_idx, workspace_idx, window, true);
        }
    }

    pub fn move_window_to_output(&mut self, window: W, output: &Output) {
        self.remove_window(&window);

        if let MonitorSet::Normal { monitors, .. } = self {
            let new_idx = monitors
                .iter()
                .position(|mon| &mon.output == output)
                .unwrap();

            let workspace_idx = monitors[new_idx].active_workspace_idx;
            // FIXME: activate only if it was already active and focused.
            self.add_window_by_idx(new_idx, workspace_idx, window, true);
        }
    }

    pub fn set_fullscreen(&mut self, window: &W, is_fullscreen: bool) {
        match self {
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
            MonitorSet::NoOutputs(workspaces) => {
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
        match self {
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
            MonitorSet::NoOutputs(workspaces) => {
                for ws in workspaces {
                    if ws.has_window(window) {
                        ws.toggle_fullscreen(window);
                        return;
                    }
                }
            }
        }
    }
}

impl MonitorSet<Window> {
    pub fn refresh(&self) {
        let _span = tracy_client::span!("MonitorSet::refresh");

        match self {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mon.workspaces {
                        ws.refresh();
                    }
                }
            }
            MonitorSet::NoOutputs(workspaces) => {
                for ws in workspaces {
                    ws.refresh();
                }
            }
        }
    }
}

impl<W: LayoutElement> Default for MonitorSet<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: LayoutElement> Monitor<W> {
    fn new(output: Output, workspaces: Vec<Workspace<W>>) -> Self {
        Self {
            output,
            workspaces,
            active_workspace_idx: 0,
            workspace_idx_anim: None,
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
            .workspace_idx_anim
            .as_ref()
            .map(|anim| anim.value())
            .unwrap_or(self.active_workspace_idx as f64);

        self.active_workspace_idx = idx;

        self.workspace_idx_anim = Some(Animation::new(
            current_idx,
            idx as f64,
            Duration::from_millis(250),
        ));
    }

    pub fn add_window(&mut self, workspace_idx: usize, window: W, activate: bool) {
        let workspace = &mut self.workspaces[workspace_idx];

        workspace.add_window(window.clone(), activate);

        // After adding a new window, workspace becomes this output's own.
        workspace.original_output = OutputId::new(&self.output);

        if workspace_idx == self.workspaces.len() - 1 {
            // Insert a new empty workspace.
            let ws = Workspace::new(self.output.clone());
            self.workspaces.push(ws);
        }

        if activate {
            self.activate_workspace(workspace_idx);
        }
    }

    fn clean_up_workspaces(&mut self) {
        assert!(self.workspace_idx_anim.is_none());

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
        let window = column.windows[column.active_window_idx].clone();
        workspace.remove_window(&window);

        self.add_window(new_idx, window, true);
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
        let window = column.windows[column.active_window_idx].clone();
        workspace.remove_window(&window);

        self.add_window(new_idx, window, true);
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
        let window = column.windows[column.active_window_idx].clone();
        workspace.remove_window(&window);

        self.add_window(new_idx, window, true);

        // Don't animate this action.
        self.workspace_idx_anim = None;
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
        self.workspace_idx_anim = None;
    }

    pub fn consume_into_column(&mut self) {
        self.active_workspace().consume_into_column();
    }

    pub fn expel_from_column(&mut self) {
        self.active_workspace().expel_from_column();
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
        if let Some(anim) = &mut self.workspace_idx_anim {
            anim.set_current_time(current_time);
            if anim.is_done() {
                self.workspace_idx_anim = None;
                self.clean_up_workspaces();
            }
        }

        for ws in &mut self.workspaces {
            ws.advance_animations(current_time, is_active);
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        for ws in &mut self.workspaces {
            ws.update_config(config);
        }
    }

    fn toggle_width(&mut self) {
        self.active_workspace().toggle_width();
    }

    fn toggle_full_width(&mut self) {
        self.active_workspace().toggle_full_width();
    }
}

impl Monitor<Window> {
    pub fn render_elements(
        &self,
        renderer: &mut GlesRenderer,
    ) -> Vec<MonitorRenderElement<GlesRenderer>> {
        let output_scale = Scale::from(self.output.current_scale().fractional_scale());
        let output_transform = self.output.current_transform();
        let output_mode = self.output.current_mode().unwrap();
        let output_size = output_transform.transform_size(output_mode.size);

        match &self.workspace_idx_anim {
            Some(anim) => {
                let render_idx = anim.value();
                let below_idx = render_idx.floor() as usize;
                let above_idx = render_idx.ceil() as usize;

                let offset =
                    ((render_idx - below_idx as f64) * output_size.h as f64).round() as i32;

                let below = self.workspaces[below_idx].render_elements(renderer);
                let above = self.workspaces[above_idx].render_elements(renderer);

                let below = below.into_iter().filter_map(|elem| {
                    Some(RelocateRenderElement::from_element(
                        CropRenderElement::from_element(
                            elem,
                            output_scale,
                            Rectangle::from_loc_and_size((0, 0), output_size),
                        )?,
                        (0, -offset),
                        Relocate::Relative,
                    ))
                });
                let above = above.into_iter().filter_map(|elem| {
                    Some(RelocateRenderElement::from_element(
                        CropRenderElement::from_element(
                            elem,
                            output_scale,
                            Rectangle::from_loc_and_size((0, 0), output_size),
                        )?,
                        (0, -offset + output_size.h),
                        Relocate::Relative,
                    ))
                });
                below.chain(above).collect()
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
                                Rectangle::from_loc_and_size((0, 0), output_size),
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
    fn new(output: Output) -> Self {
        Self {
            original_output: OutputId::new(&output),
            view_size: output_size(&output),
            working_area: Rectangle::from_loc_and_size((0, 0), output_size(&output)),
            output: Some(output),
            columns: vec![],
            active_column_idx: 0,
            focus_ring: FocusRing::default(),
            view_offset: 0,
            view_offset_anim: None,
            activate_prev_column_on_removal: false,
        }
    }

    fn new_no_outputs() -> Self {
        Self {
            output: None,
            original_output: OutputId(String::new()),
            view_size: Size::from((1280, 720)),
            working_area: Rectangle::from_loc_and_size((0, 0), (1280, 720)),
            columns: vec![],
            active_column_idx: 0,
            focus_ring: FocusRing::default(),
            view_offset: 0,
            view_offset_anim: None,
            activate_prev_column_on_removal: false,
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

        // This shall one day become a proper animation.
        if !self.columns.is_empty() {
            let col = &self.columns[self.active_column_idx];
            let active_win = &col.windows[col.active_window_idx];
            let geom = active_win.geometry();
            self.focus_ring.resize(geom.size);
            self.focus_ring.set_active(is_active);
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        let c = &config.focus_ring;
        self.focus_ring.is_off = c.off;
        self.focus_ring.width = c.width.into();
        self.focus_ring.active_color = c.active_color;
        self.focus_ring.inactive_color = c.inactive_color;
        // The focus ring buffer will be updated in a subsequent update_animations call.
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
            col.update_window_sizes(self.view_size, self.working_area);
        }
    }

    fn activate_column(&mut self, idx: usize) {
        if self.active_column_idx == idx {
            return;
        }

        let current_x = self.view_pos();

        self.active_column_idx = idx;

        // A different column was activated; reset the flag.
        self.activate_prev_column_on_removal = false;

        let new_x = self.column_x(idx) - PADDING;
        let new_view_offset = compute_new_view_offset(
            current_x,
            self.working_area.size.w,
            new_x,
            self.columns[idx].size().w,
        );

        let from_view_offset = current_x - new_x;
        self.view_offset_anim = Some(Animation::new(
            from_view_offset as f64,
            new_view_offset as f64,
            Duration::from_millis(250),
        ));
        self.view_offset = from_view_offset;
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
        let mut x = PADDING;

        for column in self.columns.iter().take(column_idx) {
            x += column.size().w + PADDING;
        }

        x
    }

    fn add_window(&mut self, window: W, activate: bool) {
        self.enter_output_for_window(&window);

        let idx = if self.columns.is_empty() {
            0
        } else {
            self.active_column_idx + 1
        };

        let column = Column::new(window, self.view_size, self.working_area);
        self.columns.insert(idx, column);

        if activate {
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
        column.update_window_sizes(self.view_size, self.working_area);
    }

    fn update_window(&mut self, window: &W) {
        let (idx, column) = self
            .columns
            .iter_mut()
            .enumerate()
            .find(|(_, col)| col.contains(window))
            .unwrap();
        column.update_window_sizes(self.view_size, self.working_area);

        if idx == self.active_column_idx {
            // We might need to move the view to ensure the resized window is still visible.
            let current_x = self.view_pos();
            let new_x = self.column_x(idx) - PADDING;

            let new_view_offset = compute_new_view_offset(
                current_x,
                self.working_area.size.w,
                new_x,
                self.columns[idx].size().w,
            );

            let cur_view_offset = self
                .view_offset_anim
                .as_ref()
                .map(|a| a.to().round() as i32)
                .unwrap_or(self.view_offset);
            if cur_view_offset != new_view_offset {
                let from_view_offset = current_x - new_x;
                self.view_offset_anim = Some(Animation::new(
                    from_view_offset as f64,
                    new_view_offset as f64,
                    Duration::from_millis(250),
                ));
                self.view_offset = from_view_offset;
            }
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

        let new_x = self.column_x(self.active_column_idx) - PADDING;
        let new_view_offset = compute_new_view_offset(
            current_x,
            self.working_area.size.w,
            new_x,
            self.columns[self.active_column_idx].size().w,
        );
        self.view_offset = new_view_offset;

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

        let new_x = self.column_x(self.active_column_idx) - PADDING;
        let new_view_offset = compute_new_view_offset(
            current_x,
            self.working_area.size.w,
            new_x,
            self.columns[self.active_column_idx].size().w,
        );
        self.view_offset = new_view_offset;

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
        target_column.add_window(self.view_size, self.working_area, window);
    }

    fn expel_from_column(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        let source_column = &mut self.columns[self.active_column_idx];
        if source_column.windows.len() == 1 {
            return;
        }

        let window = source_column.windows[source_column.active_window_idx].clone();
        self.remove_window(&window);

        self.add_window(window, true);
    }

    fn send_frame(&self, time: Duration) {
        let output = self.output.as_ref().unwrap();
        for win in self.windows() {
            win.send_frame(output, time, None, |_, _| Some(output.clone()));
        }
    }

    fn send_dmabuf_feedback(&self, feedback: &DmabufFeedback) {
        let output = self.output.as_ref().unwrap();
        for win in self.windows() {
            win.send_dmabuf_feedback(output, |_, _| Some(output.clone()), |_, _| feedback);
        }
    }

    fn view_pos(&self) -> i32 {
        self.column_x(self.active_column_idx) + self.view_offset - PADDING
    }

    fn window_under(
        &self,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<(&W, Point<i32, Logical>)> {
        if self.columns.is_empty() {
            return None;
        }

        let view_pos = self.view_pos();

        let mut pos = pos_within_output;
        pos.x += view_pos as f64;

        // Prefer the active window since it's drawn on top.
        let col = &self.columns[self.active_column_idx];
        let active_win = &col.windows[col.active_window_idx];
        let geom = active_win.geometry();
        let mut win_pos = Point::from((
            self.column_x(self.active_column_idx),
            col.window_y(col.active_window_idx),
        )) - geom.loc;
        if col.is_fullscreen {
            // FIXME: fullscreen windows are missing left padding
            win_pos.x -= PADDING;
        } else {
            win_pos += self.working_area.loc;
        }
        if active_win.is_in_input_region(&(pos - win_pos.to_f64())) {
            let mut win_pos_within_output = win_pos;
            win_pos_within_output.x -= view_pos;
            return Some((active_win, win_pos_within_output));
        }

        let mut x = PADDING;
        for col in &self.columns {
            let mut y = PADDING;

            for win in &col.windows {
                let geom = win.geometry();

                if win != active_win {
                    // x, y point at the top-left of the window geometry.
                    let mut win_pos = Point::from((x, y)) - geom.loc;
                    if col.is_fullscreen {
                        // FIXME: fullscreen windows are missing left padding
                        win_pos.x -= PADDING;
                        win_pos.y -= PADDING;
                    } else {
                        win_pos += self.working_area.loc;
                    }
                    if win.is_in_input_region(&(pos - win_pos.to_f64())) {
                        let mut win_pos_within_output = win_pos;
                        win_pos_within_output.x -= view_pos;
                        return Some((win, win_pos_within_output));
                    }
                }

                y += geom.size.h + PADDING;
            }

            x += col.size().w + PADDING;
        }

        None
    }

    fn toggle_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].toggle_width(self.view_size, self.working_area);
    }

    fn toggle_full_width(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.columns[self.active_column_idx].toggle_full_width(self.view_size, self.working_area);
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
            col.active_window_idx = min(col.active_window_idx, col.windows.len() - 1);
            col.update_window_sizes(self.view_size, self.working_area);

            col_idx += 1;
            self.columns.insert(
                col_idx,
                Column::new(window, self.view_size, self.working_area),
            );
            if self.active_column_idx >= col_idx || target_window_was_focused {
                self.active_column_idx += 1;
            }
            col = &mut self.columns[col_idx];
        }

        col.set_fullscreen(self.view_size, self.working_area, is_fullscreen);
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
}

impl Workspace<Window> {
    fn refresh(&self) {
        for (col_idx, col) in self.columns.iter().enumerate() {
            for (win_idx, win) in col.windows.iter().enumerate() {
                let active = self.active_column_idx == col_idx && col.active_window_idx == win_idx;
                win.set_activated(active);
                win.toplevel().send_pending_configure();
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
        let geom = active_win.geometry();
        let mut win_pos = Point::from((
            self.column_x(self.active_column_idx) - view_pos,
            col.window_y(col.active_window_idx),
        )) - geom.loc;
        if col.is_fullscreen {
            // FIXME: fullscreen windows are missing left padding
            win_pos.x -= PADDING;
        } else {
            win_pos += self.working_area.loc;
        }

        // Draw the window itself.
        rv.extend(active_win.render_elements(
            renderer,
            win_pos.to_physical_precise_round(output_scale),
            output_scale,
            1.,
        ));

        // Draw the focus ring.
        rv.extend(
            self.focus_ring
                .render(win_pos + geom.loc, output_scale)
                .map(Into::into),
        );

        let mut x = PADDING;
        for col in &self.columns {
            let mut y = PADDING;

            for win in &col.windows {
                let geom = win.geometry();

                if win != active_win {
                    let mut win_pos = Point::from((x - view_pos, y)) - geom.loc;
                    if col.is_fullscreen {
                        // FIXME: fullscreen windows are missing left padding
                        win_pos.x -= PADDING;
                        win_pos.y -= PADDING;
                    } else {
                        win_pos += self.working_area.loc;
                    }

                    rv.extend(win.render_elements(
                        renderer,
                        win_pos.to_physical_precise_round(output_scale),
                        output_scale,
                        1.,
                    ));
                }

                y += geom.size.h + PADDING;
            }

            x += col.size().w + PADDING;
        }

        rv
    }
}

impl<W: LayoutElement> Column<W> {
    fn new(
        window: W,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
    ) -> Self {
        let mut rv = Self {
            windows: vec![],
            active_window_idx: 0,
            width: ColumnWidth::default(),
            is_fullscreen: false,
        };

        rv.add_window(view_size, working_area, window);

        rv
    }

    fn window_count(&self) -> usize {
        self.windows.len()
    }

    fn set_width(
        &mut self,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
        width: ColumnWidth,
    ) {
        self.width = width;
        self.update_window_sizes(view_size, working_area);
    }

    fn contains(&self, window: &W) -> bool {
        self.windows.iter().any(|win| win == window)
    }

    fn activate_window(&mut self, window: &W) {
        let idx = self.windows.iter().position(|win| win == window).unwrap();
        self.active_window_idx = idx;
    }

    fn add_window(
        &mut self,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
        window: W,
    ) {
        self.is_fullscreen = false;
        self.windows.push(window);
        self.update_window_sizes(view_size, working_area);
    }

    fn update_window_sizes(
        &mut self,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
    ) {
        if self.is_fullscreen {
            self.windows[0].request_fullscreen(view_size);
            return;
        }

        let min_width = self
            .windows
            .iter()
            .filter_map(|win| {
                let w = win.min_size().w;
                if w == 0 {
                    None
                } else {
                    Some(w)
                }
            })
            .max()
            .unwrap_or(1);
        let max_width = self
            .windows
            .iter()
            .filter_map(|win| {
                let w = win.max_size().w;
                if w == 0 {
                    None
                } else {
                    Some(w)
                }
            })
            .min()
            .unwrap_or(i32::MAX);
        let max_width = max(max_width, min_width);

        let width = self.width.resolve(working_area.size.w - PADDING) - PADDING;
        let height = (working_area.size.h - PADDING) / self.window_count() as i32 - PADDING;
        let size = Size::from((max(min(width, max_width), min_width), max(height, 1)));

        for win in &self.windows {
            win.request_size(size);
        }
    }

    /// Computes the size of the column including top and bottom padding.
    fn size(&self) -> Size<i32, Logical> {
        let mut total = Size::from((0, PADDING));

        for window in &self.windows {
            let size = window.geometry().size;
            total.w = max(total.w, size.w);
            total.h += size.h + PADDING;
        }

        total
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
        self.active_window_idx = new_idx;
    }

    fn move_down(&mut self) {
        let new_idx = min(self.active_window_idx + 1, self.windows.len() - 1);
        if self.active_window_idx == new_idx {
            return;
        }

        self.windows.swap(self.active_window_idx, new_idx);
        self.active_window_idx = new_idx;
    }

    #[cfg(test)]
    fn verify_invariants(&self) {
        assert!(!self.windows.is_empty(), "columns can't be empty");
        assert!(self.active_window_idx < self.windows.len());

        if self.is_fullscreen {
            assert_eq!(self.windows.len(), 1);
        }
    }

    fn toggle_width(
        &mut self,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
    ) {
        let idx = match self.width {
            ColumnWidth::PresetProportion(idx) => (idx + 1) % WIDTH_PROPORTIONS.len(),
            _ => {
                let current = self.size().w;
                WIDTH_PROPORTIONS
                    .into_iter()
                    .position(|prop| {
                        prop.resolve(working_area.size.w - PADDING) - PADDING > current
                    })
                    .unwrap_or(0)
            }
        };
        let width = ColumnWidth::PresetProportion(idx);
        self.set_width(view_size, working_area, width);
    }

    fn toggle_full_width(
        &mut self,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
    ) {
        let width = match self.width {
            ColumnWidth::Proportion(x) if x == 1. => {
                // FIXME: would be good to restore to previous width here.
                ColumnWidth::default()
            }
            _ => ColumnWidth::Proportion(1.),
        };
        self.set_width(view_size, working_area, width);
    }

    fn set_fullscreen(
        &mut self,
        view_size: Size<i32, Logical>,
        working_area: Rectangle<i32, Logical>,
        is_fullscreen: bool,
    ) {
        assert_eq!(self.windows.len(), 1);
        self.is_fullscreen = is_fullscreen;
        self.update_window_sizes(view_size, working_area);
    }

    fn window_y(&self, window_idx: usize) -> i32 {
        if self.is_fullscreen {
            return 0;
        }

        let mut y = PADDING;

        for win in self.windows.iter().take(window_idx) {
            y += win.geometry().size.h + PADDING;
        }

        y
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

pub fn configure_new_window(working_area: Rectangle<i32, Logical>, window: &Window) {
    let width = ColumnWidth::default().resolve(working_area.size.w - PADDING) - PADDING;
    let height = working_area.size.h - PADDING * 2;
    let size = Size::from((max(width, 1), max(height, 1)));

    let bounds = Size::from((
        working_area.size.w - PADDING * 2,
        working_area.size.h - PADDING * 2,
    ));

    window.toplevel().with_pending_state(|state| {
        state.size = Some(size);
        state.bounds = Some(bounds);
    });
}

fn compute_new_view_offset(cur_x: i32, view_width: i32, new_x: i32, new_col_width: i32) -> i32 {
    // If the column is wider than the view, always left-align it.
    if new_col_width + PADDING * 2 >= view_width {
        return 0;
    }

    // If the column is already fully visible, leave the view as is.
    if new_x >= cur_x && new_x + new_col_width + PADDING * 2 <= cur_x + view_width {
        return -(new_x - cur_x);
    }

    // Otherwise, prefer the aligment that results in less motion from the current position.
    let dist_to_left = cur_x.abs_diff(new_x);
    let dist_to_right = (cur_x + view_width).abs_diff(new_x + new_col_width + PADDING * 2);
    if dist_to_left <= dist_to_right {
        0
    } else {
        -(view_width - new_col_width - PADDING * 2)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use smithay::output::{Mode, PhysicalProperties, Subpixel};
    use smithay::utils::IsAlive;

    use super::*;

    #[derive(Debug)]
    struct TestWindowInner {
        id: usize,
        bbox: Cell<Rectangle<i32, Logical>>,
    }

    #[derive(Debug, Clone)]
    struct TestWindow(Rc<TestWindowInner>);

    impl TestWindow {
        fn new(id: usize, bbox: Rectangle<i32, Logical>) -> Self {
            Self(Rc::new(TestWindowInner {
                id,
                bbox: Cell::new(bbox),
            }))
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
        fn request_size(&self, _size: Size<i32, Logical>) {}

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

        fn send_frame<T, F>(
            &self,
            _output: &Output,
            _time: T,
            _throttle: Option<Duration>,
            _primary_scan_out_output: F,
        ) where
            T: Into<Duration>,
            F: FnMut(&WlSurface, &SurfaceData) -> Option<Output> + Copy,
        {
        }

        fn send_dmabuf_feedback<'a, P, F>(
            &self,
            _output: &Output,
            _primary_scan_out_output: P,
            _select_dmabuf_feedback: F,
        ) where
            P: FnMut(&WlSurface, &SurfaceData) -> Option<Output> + Copy,
            F: Fn(&WlSurface, &SurfaceData) -> &'a DmabufFeedback + Copy,
        {
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Op {
        AddOutput(usize),
        RemoveOutput(usize),
        FocusOutput(usize),
        AddWindow {
            id: usize,
            bbox: Rectangle<i32, Logical>,
            activate: bool,
        },
        CloseWindow(usize),
        FocusColumnLeft,
        FocusColumnRight,
        MoveColumnLeft,
        MoveColumnRight,
        ConsumeWindowIntoColumn,
        ExpelWindowFromColumn,
        FocusWorkspaceDown,
        FocusWorkspaceUp,
        FocusWorkspace(u8),
        MoveWindowToWorkspaceDown,
        MoveWindowToWorkspaceUp,
        MoveWindowToWorkspace(u8),
    }

    impl Op {
        fn apply(self, monitor_set: &mut MonitorSet<TestWindow>) {
            match self {
                Op::AddOutput(id) => {
                    let name = format!("output{id}");
                    if monitor_set.outputs().any(|o| o.name() == name) {
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
                    monitor_set.add_output(output.clone());
                }
                Op::RemoveOutput(id) => {
                    let name = format!("output{id}");
                    let Some(output) = monitor_set.outputs().find(|o| o.name() == name).cloned()
                    else {
                        return;
                    };

                    monitor_set.remove_output(&output);
                }
                Op::FocusOutput(id) => {
                    let name = format!("output{id}");
                    let Some(output) = monitor_set.outputs().find(|o| o.name() == name).cloned()
                    else {
                        return;
                    };

                    monitor_set.focus_output(&output);
                }
                Op::AddWindow { id, bbox, activate } => {
                    let win = TestWindow::new(id, bbox);
                    monitor_set.add_window(win, activate);
                }
                Op::CloseWindow(id) => {
                    let dummy = TestWindow::new(id, Rectangle::default());
                    monitor_set.remove_window(&dummy);
                }
                Op::FocusColumnLeft => monitor_set.focus_left(),
                Op::FocusColumnRight => monitor_set.focus_right(),
                Op::MoveColumnLeft => monitor_set.move_left(),
                Op::MoveColumnRight => monitor_set.move_right(),
                Op::ConsumeWindowIntoColumn => monitor_set.consume_into_column(),
                Op::ExpelWindowFromColumn => monitor_set.expel_from_column(),
                Op::FocusWorkspaceDown => monitor_set.switch_workspace_down(),
                Op::FocusWorkspaceUp => monitor_set.switch_workspace_up(),
                Op::FocusWorkspace(idx) => monitor_set.switch_workspace(idx),
                Op::MoveWindowToWorkspaceDown => monitor_set.move_to_workspace_down(),
                Op::MoveWindowToWorkspaceUp => monitor_set.move_to_workspace_up(),
                Op::MoveWindowToWorkspace(idx) => monitor_set.move_to_workspace(idx),
            }
        }
    }

    #[test]
    fn operations() {
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
        ];

        for third in every_op {
            for second in every_op {
                for first in every_op {
                    eprintln!("{first:?}, {second:?}, {third:?}");

                    let mut monitor_set = MonitorSet::default();
                    first.apply(&mut monitor_set);
                    monitor_set.verify_invariants();
                    second.apply(&mut monitor_set);
                    monitor_set.verify_invariants();
                    third.apply(&mut monitor_set);
                    monitor_set.verify_invariants();
                }
            }
        }
    }
}
