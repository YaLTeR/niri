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

use std::mem;
use std::rc::Rc;
use std::time::Duration;

use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::{ImportAll, Renderer};
use smithay::desktop::space::SpaceElement;
use smithay::desktop::Window;
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::compositor::{send_surface_state, with_states};
use smithay::wayland::shell::xdg::SurfaceCachedState;

pub use self::monitor::MonitorRenderElement;
use self::monitor::{Monitor, WorkspaceSwitch, WorkspaceSwitchGesture};
use self::workspace::{
    compute_working_area, ColumnWidth, OutputId, Workspace, WorkspaceRenderElement,
};
use crate::animation::Animation;
use crate::config::{self, Config, SizeChange, Struts};
use crate::utils::output_size;

mod focus_ring;
mod monitor;
mod workspace;

pub trait LayoutElement: PartialEq {
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
    fn render<R: Renderer + ImportAll>(
        &self,
        renderer: &mut R,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
    ) -> Vec<WorkspaceRenderElement<R>>
    where
        <R as Renderer>::TextureId: 'static;

    fn request_size(&self, size: Size<i32, Logical>);
    fn request_fullscreen(&self, size: Size<i32, Logical>);
    fn min_size(&self) -> Size<i32, Logical>;
    fn max_size(&self) -> Size<i32, Logical>;
    fn is_wl_surface(&self, wl_surface: &WlSurface) -> bool;
    fn has_ssd(&self) -> bool;
    fn set_preferred_scale_transform(&self, scale: i32, transform: Transform);
    fn output_enter(&self, output: &Output);
    fn output_leave(&self, output: &Output);
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

#[derive(Debug, PartialEq)]
pub struct Options {
    /// Padding around windows in logical pixels.
    gaps: i32,
    /// Extra padding around the working area in logical pixels.
    struts: Struts,
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
            struts: Default::default(),
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
            struts: config.struts,
            focus_ring: config.focus_ring,
            preset_widths,
            default_width,
        }
    }
}

impl LayoutElement for Window {
    fn size(&self) -> Size<i32, Logical> {
        self.geometry().size
    }

    fn buf_loc(&self) -> Point<i32, Logical> {
        Point::from((0, 0)) - self.geometry().loc
    }

    fn is_in_input_region(&self, point: Point<f64, Logical>) -> bool {
        let surace_local = point + self.geometry().loc.to_f64();
        SpaceElement::is_in_input_region(self, &surace_local)
    }

    fn render<R: Renderer + ImportAll>(
        &self,
        renderer: &mut R,
        location: Point<i32, Logical>,
        scale: Scale<f64>,
    ) -> Vec<WorkspaceRenderElement<R>>
    where
        <R as Renderer>::TextureId: 'static,
    {
        let buf_pos = location - self.geometry().loc;
        self.render_elements(
            renderer,
            buf_pos.to_physical_precise_round(scale),
            scale,
            1.,
        )
    }

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

    fn output_enter(&self, output: &Output) {
        let overlap = Rectangle::from_loc_and_size((0, 0), (i32::MAX, i32::MAX));
        SpaceElement::output_enter(self, output, overlap)
    }

    fn output_leave(&self, output: &Output) {
        SpaceElement::output_leave(self, output)
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
        width: Option<ColumnWidth>,
        is_full_width: bool,
    ) -> Option<&Output> {
        let width = width
            .or(self.options.default_width)
            .unwrap_or_else(|| ColumnWidth::Fixed(window.size().w));

        match &mut self.monitor_set {
            MonitorSet::Normal {
                monitors,
                active_monitor_idx,
                ..
            } => {
                let mon = &mut monitors[*active_monitor_idx];

                // Don't steal focus from an active fullscreen window.
                let mut activate = true;
                let ws = &mon.workspaces[mon.active_workspace_idx];
                if !ws.columns.is_empty() && ws.columns[ws.active_column_idx].is_fullscreen {
                    activate = false;
                }

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
                ws.add_window(window, true, width, is_full_width);
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

    pub fn find_window_and_output(&self, wl_surface: &WlSurface) -> Option<(&W, &Output)> {
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

    pub fn window_y(&self, window: &W) -> Option<i32> {
        match &self.monitor_set {
            MonitorSet::Normal { monitors, .. } => {
                for mon in monitors {
                    for ws in &mon.workspaces {
                        for col in &ws.columns {
                            if let Some(idx) = col.position(window) {
                                return Some(col.window_y(idx));
                            }
                        }
                    }
                }
            }
            MonitorSet::NoOutputs { workspaces, .. } => {
                for ws in workspaces {
                    for col in &ws.columns {
                        if let Some(idx) = col.position(window) {
                            return Some(col.window_y(idx));
                        }
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
                let working_area = compute_working_area(output, self.options.struts);

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
            for (workspace_idx, ws) in mon.workspaces.iter_mut().enumerate() {
                if ws.has_window(window) {
                    *active_monitor_idx = monitor_idx;
                    ws.activate_window(window);

                    // Switch to that workspace if not already during a transition.
                    if mon.workspace_switch.is_none() {
                        mon.switch_workspace(workspace_idx);
                    }

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

    pub fn active_window(&self) -> Option<(&W, &Output)> {
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
        Some((&col.windows[col.active_window_idx], &mon.output))
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

    pub fn move_to_workspace(&mut self, idx: usize) {
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

    pub fn switch_workspace(&mut self, idx: usize) {
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
            let width = column.width;
            let is_full_width = column.is_full_width;
            let window = ws.remove_window_by_idx(ws.active_column_idx, column.active_window_idx);

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
                        if col.contains(&window) {
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use proptest::prelude::*;
    use proptest_derive::Arbitrary;
    use smithay::output::{Mode, PhysicalProperties, Subpixel};

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

    impl LayoutElement for TestWindow {
        fn size(&self) -> Size<i32, Logical> {
            self.0.bbox.get().size
        }

        fn buf_loc(&self) -> Point<i32, Logical> {
            (0, 0).into()
        }

        fn is_in_input_region(&self, _point: Point<f64, Logical>) -> bool {
            false
        }

        fn render<R: Renderer + ImportAll>(
            &self,
            _renderer: &mut R,
            _location: Point<i32, Logical>,
            _scale: Scale<f64>,
        ) -> Vec<WorkspaceRenderElement<R>> {
            vec![]
        }

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

        fn output_enter(&self, _output: &Output) {}

        fn output_leave(&self, _output: &Output) {}
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
        },
        CloseWindow(#[proptest(strategy = "1..=5usize")] usize),
        FullscreenWindow(#[proptest(strategy = "1..=5usize")] usize),
        FocusColumnLeft,
        FocusColumnRight,
        FocusWindowDown,
        FocusWindowUp,
        FocusWindowOrWorkspaceDown,
        FocusWindowOrWorkspaceUp,
        MoveColumnLeft,
        MoveColumnRight,
        MoveWindowDown,
        MoveWindowUp,
        MoveWindowDownOrToWorkspaceDown,
        MoveWindowUpOrToWorkspaceUp,
        ConsumeWindowIntoColumn,
        ExpelWindowFromColumn,
        CenterColumn,
        FocusWorkspaceDown,
        FocusWorkspaceUp,
        FocusWorkspace(#[proptest(strategy = "0..=4usize")] usize),
        MoveWindowToWorkspaceDown,
        MoveWindowToWorkspaceUp,
        MoveWindowToWorkspace(#[proptest(strategy = "0..=4usize")] usize),
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
                Op::AddWindow { id, bbox } => {
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
                    layout.add_window(win, None, false);
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
                Op::FocusWindowOrWorkspaceDown => layout.focus_window_or_workspace_down(),
                Op::FocusWindowOrWorkspaceUp => layout.focus_window_or_workspace_up(),
                Op::MoveColumnLeft => layout.move_left(),
                Op::MoveColumnRight => layout.move_right(),
                Op::MoveWindowDown => layout.move_down(),
                Op::MoveWindowUp => layout.move_up(),
                Op::MoveWindowDownOrToWorkspaceDown => layout.move_down_or_to_workspace_down(),
                Op::MoveWindowUpOrToWorkspaceUp => layout.move_up_or_to_workspace_up(),
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
            },
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
            },
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
            },
            Op::CloseWindow(0),
            Op::CloseWindow(1),
            Op::CloseWindow(2),
            Op::FocusColumnLeft,
            Op::FocusColumnRight,
            Op::FocusWindowUp,
            Op::FocusWindowOrWorkspaceUp,
            Op::FocusWindowDown,
            Op::FocusWindowOrWorkspaceDown,
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
            Op::MoveWindowDownOrToWorkspaceDown,
            Op::MoveWindowUp,
            Op::MoveWindowUpOrToWorkspaceUp,
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
            },
            Op::MoveWindowToWorkspaceDown,
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
            },
            Op::AddWindow {
                id: 3,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
            },
            Op::FocusColumnLeft,
            Op::ConsumeWindowIntoColumn,
            Op::AddWindow {
                id: 4,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
            },
            Op::AddOutput(2),
            Op::AddWindow {
                id: 5,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
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
            },
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
            },
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
            },
            Op::CloseWindow(0),
            Op::CloseWindow(1),
            Op::CloseWindow(2),
            Op::FocusColumnLeft,
            Op::FocusColumnRight,
            Op::FocusWindowUp,
            Op::FocusWindowOrWorkspaceUp,
            Op::FocusWindowDown,
            Op::FocusWindowOrWorkspaceDown,
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
            Op::MoveWindowDownOrToWorkspaceDown,
            Op::MoveWindowUp,
            Op::MoveWindowUpOrToWorkspaceUp,
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
            },
            Op::FocusOutput(2),
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
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
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                id: 1,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
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
            },
            Op::FocusWorkspaceDown,
            Op::AddWindow {
                id: 2,
                bbox: Rectangle::from_loc_and_size((0, 0), (100, 200)),
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
