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

use smithay::desktop::{Space, Window};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Size};

const PADDING: i32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputId(String);

#[derive(Debug)]
pub enum MonitorSet {
    /// At least one output is connected.
    Normal {
        monitors: Vec<Monitor>,
        /// Index of the primary monitor.
        primary_idx: usize,
        /// Index of the active monitor.
        active_monitor_idx: usize,
    },
    /// No outputs are connected, and these are the workspaces.
    // FIXME: preserve active output id?
    NoOutputs(Vec<Workspace>),
}

#[derive(Debug)]
pub struct Monitor {
    output: Output,
    // Must always contain at least one.
    workspaces: Vec<Workspace>,
    /// Index of the currently active workspace.
    active_workspace_idx: usize,
}

#[derive(Debug)]
pub struct Workspace {
    /// The original output of this workspace.
    ///
    /// Most of the time this will be the workspace's current output, however, after an output
    /// disconnection, it may remain pointing to the disconnected output.
    original_output: OutputId,

    layout: Layout,

    // The actual Space with windows in this workspace. Should be synchronized to the layout except
    // for a brief period during surface commit handling.
    pub space: Space<Window>,
}

#[derive(Debug)]
pub struct Layout {
    columns: Vec<Column>,
    /// Index of the currently active column, if any.
    active_column_idx: usize,
}

#[derive(Debug)]
pub struct Column {
    // Must be non-empty.
    windows: Vec<Window>,
    /// Index of the currently active window.
    active_window_idx: usize,
}

impl OutputId {
    pub fn new(output: &Output) -> Self {
        Self(output.name())
    }
}

impl MonitorSet {
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
                        let mut ws = primary.workspaces.remove(i);
                        ws.space.unmap_output(&primary.output);
                        workspaces.push(ws);
                    }
                }
                workspaces.reverse();
                if workspaces
                    .iter()
                    .all(|ws| ws.space.elements().next().is_some())
                {
                    // Make sure there's always an empty workspace.
                    workspaces.push(Workspace {
                        original_output: id,
                        layout: Layout::new(),
                        space: Space::default(),
                    });
                }

                for ws in &mut workspaces {
                    ws.space.map_output(&output, (0, 0));
                }

                monitors.push(Monitor {
                    output,
                    workspaces,
                    active_workspace_idx: 0,
                });
                MonitorSet::Normal {
                    monitors,
                    primary_idx,
                    active_monitor_idx,
                }
            }
            MonitorSet::NoOutputs(mut workspaces) => {
                if workspaces.iter().all(|ws| ws.original_output != id) {
                    workspaces.insert(
                        0,
                        Workspace {
                            original_output: id.clone(),
                            layout: Layout::new(),
                            space: Space::default(),
                        },
                    );
                }

                for workspace in &mut workspaces {
                    workspace.space.map_output(&output, (0, 0));
                }

                let monitor = Monitor {
                    output,
                    workspaces,
                    active_workspace_idx: 0,
                };
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
                    ws.space.unmap_output(output);
                }

                // Get rid of empty workspaces.
                workspaces.retain(|ws| ws.space.elements().next().is_some());

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
                        ws.space.map_output(&primary.output, (0, 0));
                    }
                    primary.workspaces.extend(workspaces);

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

    pub fn configure_new_window(output: &Output, window: &Window) {
        let output_size = output_size(output);
        let size = Size::from((
            (output_size.w - PADDING * 3) / 2,
            output_size.h - PADDING * 2,
        ));
        let bounds = Size::from((output_size.w - PADDING * 2, output_size.h - PADDING * 2));

        window.toplevel().with_pending_state(|state| {
            state.size = Some(size);
            state.bounds = Some(bounds);
        });
    }

    pub fn add_window(
        &mut self,
        monitor_idx: usize,
        workspace_idx: usize,
        window: Window,
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

        let monitor = &mut monitors[monitor_idx];
        let workspace = &mut monitor.workspaces[workspace_idx];
        workspace.layout.add_window(window.clone(), activate);
        workspace.space.map_element(window.clone(), (0, 0), false);
        workspace.layout.sync_space(&mut workspace.space);

        MonitorSet::configure_new_window(&monitor.output, &window);
        window.toplevel().send_pending_configure();

        if activate {
            *active_monitor_idx = monitor_idx;
            monitor.active_workspace_idx = workspace_idx;
        }

        if workspace_idx == monitor.workspaces.len() - 1 {
            // Insert a new empty workspace.
            let mut ws = Workspace {
                original_output: OutputId::new(&monitor.output),
                layout: Layout::new(),
                space: Space::default(),
            };
            ws.space.map_output(&monitor.output, (0, 0));
            monitor.workspaces.push(ws);
        }
    }

    pub fn add_window_to_output(&mut self, output: &Output, window: Window, activate: bool) {
        let MonitorSet::Normal { monitors, .. } = self else {
            panic!()
        };

        let (monitor_idx, monitor) = monitors
            .iter()
            .enumerate()
            .find(|(_, mon)| &mon.output == output)
            .unwrap();
        let workspace_idx = monitor.active_workspace_idx;

        self.add_window(monitor_idx, workspace_idx, window, activate)
    }

    pub fn remove_window(&mut self, window: &Window) {
        let MonitorSet::Normal { monitors, .. } = self else {
            panic!()
        };

        let (output, workspace) = monitors
            .iter_mut()
            .flat_map(|mon| mon.workspaces.iter_mut().map(|ws| (&mon.output, ws)))
            .find(|(_, ws)| ws.space.elements().any(|win| win == window))
            .unwrap();

        workspace
            .layout
            .remove_window(window, output_size(output).h);
        workspace.space.unmap_elem(window);
        workspace.layout.sync_space(&mut workspace.space);

        // FIXME: remove empty unfocused workspaces.
    }

    pub fn update_window(&mut self, window: &Window) {
        let workspace = self
            .workspaces()
            .find(|ws| ws.space.elements().any(|w| w == window))
            .unwrap();
        workspace.layout.sync_space(&mut workspace.space);
    }

    pub fn activate_window(&mut self, window: &Window) {
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
                if ws.space.elements().any(|win| win == window) {
                    *active_monitor_idx = monitor_idx;
                    mon.active_workspace_idx = workspace_idx;

                    let changed = ws.layout.activate_window(window);
                    if changed {
                        ws.layout.sync_space(&mut ws.space);
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

    fn active_workspace(&mut self) -> Option<&mut Workspace> {
        let monitor = self.active_monitor()?;
        Some(&mut monitor.workspaces[monitor.active_workspace_idx])
    }

    fn active_monitor(&mut self) -> Option<&mut Monitor> {
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

    pub fn move_left(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        let changed = workspace.layout.move_left();
        if changed {
            workspace.layout.sync_space(&mut workspace.space);
        }
    }

    pub fn move_right(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        let changed = workspace.layout.move_right();
        if changed {
            workspace.layout.sync_space(&mut workspace.space);
        }
    }

    pub fn move_down(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        let changed = workspace.layout.move_down();
        if changed {
            workspace.layout.sync_space(&mut workspace.space);
        }
    }

    pub fn move_up(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        let changed = workspace.layout.move_up();
        if changed {
            workspace.layout.sync_space(&mut workspace.space);
        }
    }

    pub fn focus_left(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        workspace.layout.focus_left();
    }

    pub fn focus_right(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        workspace.layout.focus_right();
    }

    pub fn focus_down(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        workspace.layout.focus_down();
    }

    pub fn focus_up(&mut self) {
        let Some(workspace) = self.active_workspace() else {
            return;
        };
        workspace.layout.focus_up();
    }

    pub fn move_to_workspace_up(&mut self) {
        let MonitorSet::Normal {
            monitors,
            ref active_monitor_idx,
            ..
        } = self
        else {
            return;
        };

        let monitor = &mut monitors[*active_monitor_idx];

        let new_idx = monitor.active_workspace_idx.saturating_sub(1);
        if new_idx == monitor.active_workspace_idx {
            return;
        }

        let workspace = &mut monitor.workspaces[monitor.active_workspace_idx];
        if workspace.layout.columns.is_empty() {
            return;
        }

        let column = &mut workspace.layout.columns[workspace.layout.active_column_idx];
        let window = column.windows[column.active_window_idx].clone();
        workspace
            .layout
            .remove_window(&window, output_size(&monitor.output).h);
        workspace.space.unmap_elem(&window);
        workspace.layout.sync_space(&mut workspace.space);

        self.add_window(*active_monitor_idx, new_idx, window, true);

        // FIXME: remove empty unfocused workspaces.
    }

    pub fn move_to_workspace_down(&mut self) {
        let MonitorSet::Normal {
            monitors,
            ref active_monitor_idx,
            ..
        } = self
        else {
            return;
        };

        let monitor = &mut monitors[*active_monitor_idx];

        let new_idx = min(
            monitor.active_workspace_idx + 1,
            monitor.workspaces.len() - 1,
        );

        if new_idx == monitor.active_workspace_idx {
            return;
        }

        let workspace = &mut monitor.workspaces[monitor.active_workspace_idx];
        if workspace.layout.columns.is_empty() {
            return;
        }

        let column = &mut workspace.layout.columns[workspace.layout.active_column_idx];
        let window = column.windows[column.active_window_idx].clone();
        workspace
            .layout
            .remove_window(&window, output_size(&monitor.output).h);
        workspace.space.unmap_elem(&window);
        workspace.layout.sync_space(&mut workspace.space);

        self.add_window(*active_monitor_idx, new_idx, window, true);

        // FIXME: remove empty unfocused workspaces.
    }

    pub fn switch_workspace_up(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };

        monitor.active_workspace_idx = monitor.active_workspace_idx.saturating_sub(1);

        // FIXME: remove empty unfocused workspaces.
    }

    pub fn switch_workspace_down(&mut self) {
        let Some(monitor) = self.active_monitor() else {
            return;
        };
        monitor.active_workspace_idx = min(
            monitor.active_workspace_idx + 1,
            monitor.workspaces.len() - 1,
        );

        // FIXME: remove empty unfocused workspaces.
    }

    pub fn consume_into_column(&mut self) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = self
        else {
            return;
        };

        let monitor = &mut monitors[*active_monitor_idx];

        let workspace = &mut monitor.workspaces[monitor.active_workspace_idx];
        let changed = workspace
            .layout
            .consume_into_column(output_size(&monitor.output).h);
        if changed {
            workspace.layout.sync_space(&mut workspace.space);
        }
    }

    pub fn expel_from_column(&mut self) {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = self
        else {
            return;
        };

        let monitor = &mut monitors[*active_monitor_idx];

        let output_scale = monitor.output.current_scale().integer_scale();
        let output_transform = monitor.output.current_transform();
        let output_mode = monitor.output.current_mode().unwrap();
        let output_size = output_transform
            .transform_size(output_mode.size)
            .to_logical(output_scale);

        let workspace = &mut monitor.workspaces[monitor.active_workspace_idx];
        let changed = workspace.layout.expel_from_column(output_size.h);
        if changed {
            workspace.layout.sync_space(&mut workspace.space);
        }
    }

    pub fn focus(&self) -> Option<&Window> {
        let MonitorSet::Normal {
            monitors,
            active_monitor_idx,
            ..
        } = self
        else {
            return None;
        };

        let monitor = &monitors[*active_monitor_idx];
        let workspace = &monitor.workspaces[monitor.active_workspace_idx];
        if workspace.layout.columns.is_empty() {
            return None;
        }

        let column = &workspace.layout.columns[workspace.layout.active_column_idx];
        Some(&column.windows[column.active_window_idx])
    }

    pub fn workspace_for_output(&mut self, output: &Output) -> Option<&mut Workspace> {
        let MonitorSet::Normal { monitors, .. } = self else {
            return None;
        };

        monitors.iter_mut().find_map(|monitor| {
            if &monitor.output == output {
                Some(&mut monitor.workspaces[monitor.active_workspace_idx])
            } else {
                None
            }
        })
    }

    pub fn workspaces(&mut self) -> impl Iterator<Item = &mut Workspace> + '_ {
        match self {
            MonitorSet::Normal { monitors, .. } => {
                monitors.iter_mut().flat_map(|mon| &mut mon.workspaces)
            }
            MonitorSet::NoOutputs(_workspaces) => todo!(),
        }
    }

    pub fn spaces(&mut self) -> impl Iterator<Item = &Space<Window>> + '_ {
        self.workspaces().map(|workspace| &workspace.space)
    }

    pub fn find_window(&mut self, wl_surface: &WlSurface) -> Option<&Window> {
        self.workspaces()
            .flat_map(|workspace| workspace.space.elements())
            .find(|window| window.toplevel().wl_surface() == wl_surface)
    }

    pub fn find_window_and_space(
        &mut self,
        wl_surface: &WlSurface,
    ) -> Option<(Window, &Space<Window>)> {
        self.spaces().find_map(|space| {
            let window = space
                .elements()
                .find(|window| window.toplevel().wl_surface() == wl_surface)
                .cloned();
            window.map(|window| (window, space))
        })
    }

    /// Refreshes the `Space`s.
    pub fn refresh(&mut self) {
        for workspace in self.workspaces() {
            workspace.space.refresh();
        }
    }

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
                        !workspace.layout.has_windows(),
                        "with no outputs there cannot be empty workspaces"
                    );

                    workspace.layout.verify_invariants();
                }

                return;
            }
        };

        assert!(primary_idx <= monitors.len());
        assert!(active_monitor_idx <= monitors.len());

        for (idx, monitor) in monitors.iter().enumerate() {
            assert!(
                !monitor.workspaces.is_empty(),
                "monitor monitor must have at least one workspace"
            );

            let monitor_id = OutputId::new(&monitor.output);

            if idx == primary_idx {
                assert!(
                    monitor
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.original_output == monitor_id),
                    "primary monitor must have at least one own workspace"
                );
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
                workspace.layout.verify_invariants();
            }
        }
    }
}

fn output_size(output: &Output) -> Size<i32, Logical> {
    let output_scale = output.current_scale().integer_scale();
    let output_transform = output.current_transform();
    let output_mode = output.current_mode().unwrap();
    let output_size = output_transform
        .transform_size(output_mode.size)
        .to_logical(output_scale);
    output_size
}

impl Default for MonitorSet {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    fn new() -> Self {
        Self {
            columns: vec![],
            active_column_idx: 0,
        }
    }

    fn sync_space(&self, space: &mut Space<Window>) {
        // FIXME: this is really inefficient
        let mut active_window = None;

        let mut x = PADDING;
        for (column_idx, column) in self.columns.iter().enumerate() {
            let mut y = PADDING;
            for (window_idx, window) in column.windows.iter().enumerate() {
                let active =
                    column_idx == self.active_column_idx && window_idx == column.active_window_idx;
                if active {
                    active_window = Some(window.clone());
                }

                window.set_activated(active);
                space.map_element(window.clone(), (x, y), false);
                window.toplevel().send_pending_configure();
                y += window.geometry().size.h + PADDING;
            }
            x += column.size().w + PADDING;
        }

        if let Some(window) = active_window {
            space.raise_element(&window, false);
        }
    }

    fn has_windows(&self) -> bool {
        self.columns.is_empty()
    }

    /// Computes the width of the layout including left and right padding, in Logical coordinates.
    fn width(&self) -> i32 {
        let mut total = PADDING;

        for column in &self.columns {
            total += column.size().w + PADDING;
        }

        total
    }

    /// Computes the X position of the windows in the given column, in logical coordinates.
    fn column_x(&self, column_idx: usize) -> i32 {
        let mut x = PADDING;

        for column in self.columns.iter().take(column_idx) {
            x += column.size().w + PADDING;
        }

        x
    }

    fn add_window(&mut self, window: Window, activate: bool) {
        let idx = if self.columns.is_empty() {
            0
        } else {
            self.active_column_idx + 1
        };

        let column = Column {
            windows: vec![window],
            active_window_idx: 0,
        };
        self.columns.insert(idx, column);

        if activate {
            self.active_column_idx = idx;
        }
    }

    fn remove_window(&mut self, window: &Window, total_height: i32) {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.windows.contains(window))
            .unwrap();
        let column = &mut self.columns[column_idx];

        let window_idx = column.windows.iter().position(|win| win == window).unwrap();
        column.windows.remove(window_idx);
        if column.windows.is_empty() {
            self.columns.remove(column_idx);
            if self.columns.is_empty() {
                return;
            }

            self.active_column_idx = min(self.active_column_idx, self.columns.len() - 1);
            return;
        }

        column.active_window_idx = min(column.active_window_idx, column.windows.len() - 1);

        // Update window sizes.
        let window_count = column.windows.len() as i32;
        let height = (total_height - PADDING * (window_count + 1)) / window_count;
        let width = column.size().w;

        for window in &mut column.windows {
            window
                .toplevel()
                .with_pending_state(|state| state.size = Some(Size::from((width, height))));
            window.toplevel().send_pending_configure();
        }
    }

    fn activate_window(&mut self, window: &Window) -> bool {
        let column_idx = self
            .columns
            .iter()
            .position(|col| col.windows.contains(window))
            .unwrap();
        let column = &mut self.columns[column_idx];

        let window_idx = column.windows.iter().position(|win| win == window).unwrap();

        if column.active_window_idx != window_idx || self.active_column_idx != column_idx {
            column.active_window_idx = window_idx;
            self.active_column_idx = column_idx;
            true
        } else {
            false
        }
    }

    fn verify_invariants(&self) {
        for column in &self.columns {
            column.verify_invariants();
        }
    }

    fn focus_left(&mut self) {
        self.active_column_idx = self.active_column_idx.saturating_sub(1);
    }

    fn focus_right(&mut self) {
        if self.columns.is_empty() {
            return;
        }

        self.active_column_idx = min(self.active_column_idx + 1, self.columns.len() - 1);
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

    fn move_left(&mut self) -> bool {
        let new_idx = self.active_column_idx.saturating_sub(1);
        if self.active_column_idx == new_idx {
            return false;
        }

        self.columns.swap(self.active_column_idx, new_idx);
        self.active_column_idx = new_idx;
        true
    }

    fn move_right(&mut self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        let new_idx = min(self.active_column_idx + 1, self.columns.len() - 1);
        if self.active_column_idx == new_idx {
            return false;
        }

        self.columns.swap(self.active_column_idx, new_idx);
        self.active_column_idx = new_idx;
        true
    }

    fn move_down(&mut self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        self.columns[self.active_column_idx].move_down()
    }

    fn move_up(&mut self) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        self.columns[self.active_column_idx].move_up()
    }

    fn consume_into_column(&mut self, total_height: i32) -> bool {
        if self.columns.len() < 2 {
            return false;
        }

        if self.active_column_idx == self.columns.len() - 1 {
            return false;
        }

        let source_column_idx = self.active_column_idx + 1;

        let source_column = &mut self.columns[source_column_idx];
        let window = source_column.windows[0].clone();
        self.remove_window(&window, total_height);

        let target_column = &mut self.columns[self.active_column_idx];

        let window_count = target_column.windows.len() as i32 + 1;
        let height = (total_height - PADDING * (window_count + 1)) / window_count;
        let width = target_column.size().w;

        target_column.windows.push(window);

        for window in &mut target_column.windows {
            window
                .toplevel()
                .with_pending_state(|state| state.size = Some(Size::from((width, height))));
            window.toplevel().send_pending_configure();
        }
        true
    }

    fn expel_from_column(&mut self, total_height: i32) -> bool {
        if self.columns.is_empty() {
            return false;
        }

        let source_column = &mut self.columns[self.active_column_idx];
        if source_column.windows.len() == 1 {
            return false;
        }

        let window = source_column.windows[source_column.active_window_idx].clone();
        self.remove_window(&window, total_height);

        window.toplevel().with_pending_state(|state| {
            state.size = Some(Size::from((state.size.unwrap().w, total_height)))
        });
        window.toplevel().send_pending_configure();
        self.add_window(window, true);

        true
    }
}

impl Column {
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

    fn window_y(&self, window_idx: usize) -> i32 {
        let mut y = PADDING;

        for window in self.windows.iter().take(window_idx) {
            let size = window.geometry().size;
            y += size.h + PADDING;
        }

        y
    }

    fn focus_up(&mut self) {
        self.active_window_idx = self.active_window_idx.saturating_sub(1);
    }

    fn focus_down(&mut self) {
        self.active_window_idx = min(self.active_window_idx + 1, self.windows.len() - 1);
    }

    fn move_up(&mut self) -> bool {
        let new_idx = self.active_window_idx.saturating_sub(1);
        if self.active_window_idx == new_idx {
            return false;
        }

        self.windows.swap(self.active_window_idx, new_idx);
        self.active_window_idx = new_idx;
        true
    }

    fn move_down(&mut self) -> bool {
        let new_idx = min(self.active_window_idx + 1, self.windows.len() - 1);
        if self.active_window_idx == new_idx {
            return false;
        }

        self.windows.swap(self.active_window_idx, new_idx);
        self.active_window_idx = new_idx;
        true
    }

    fn verify_invariants(&self) {
        assert!(!self.windows.is_empty(), "columns can't be empty");
    }
}
