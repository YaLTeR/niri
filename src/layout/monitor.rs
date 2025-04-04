use std::cmp::min;
use std::iter::zip;
use std::rc::Rc;
use std::time::Duration;

use smithay::backend::renderer::element::utils::{
    CropRenderElement, Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::scrolling::{Column, ColumnWidth};
use super::tile::Tile;
use super::workspace::{
    OutputId, Workspace, WorkspaceAddWindowTarget, WorkspaceId, WorkspaceRenderElement,
};
use super::{ActivateWindow, HitType, LayoutElement, Options, OVERVIEW_WORKSPACE_SCALE};
use crate::animation::{Animation, Clock};
use crate::input::swipe_tracker::SwipeTracker;
use crate::niri_render_elements;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::shadow::ShadowRenderElement;
use crate::render_helpers::RenderTarget;
use crate::rubber_band::RubberBand;
use crate::utils::transaction::Transaction;
use crate::utils::{output_size, round_logical_in_physical, ResizeEdge};

/// Amount of touchpad movement to scroll the height of one workspace.
const WORKSPACE_GESTURE_MOVEMENT: f64 = 300.;

const WORKSPACE_GESTURE_RUBBER_BAND: RubberBand = RubberBand {
    stiffness: 0.5,
    limit: 0.05,
};

#[derive(Debug)]
pub struct Monitor<W: LayoutElement> {
    /// Output for this monitor.
    pub(super) output: Output,
    /// Cached name of the output.
    output_name: String,
    // Must always contain at least one.
    pub(super) workspaces: Vec<Workspace<W>>,
    /// Index of the currently active workspace.
    pub(super) active_workspace_idx: usize,
    /// ID of the previously active workspace.
    pub(super) previous_workspace_id: Option<WorkspaceId>,
    /// In-progress switch between workspaces.
    pub(super) workspace_switch: Option<WorkspaceSwitch>,
    /// Whether the overview is open.
    pub(super) overview_open: bool,
    /// Progress of the overview zoom animation, 1 is fully in overview.
    pub(super) overview_progress: Option<f64>,
    /// Clock for driving animations.
    pub(super) clock: Clock,
    /// Configurable properties of the layout.
    pub(super) options: Rc<Options>,
}

#[derive(Debug)]
pub enum WorkspaceSwitch {
    Animation(Animation),
    Gesture(WorkspaceSwitchGesture),
}

#[derive(Debug)]
pub struct WorkspaceSwitchGesture {
    /// Index of the workspace where the gesture was started.
    center_idx: usize,
    /// Current, fractional workspace index.
    pub(super) current_idx: f64,
    tracker: SwipeTracker,
    /// Whether the gesture is controlled by the touchpad.
    is_touchpad: bool,
    /// Whether the gesture is clamped to +-1 workspace around the center.
    is_clamped: bool,

    // If this gesture is for drag-and-drop scrolling, this is the last event's unadjusted
    // timestamp.
    dnd_last_event_time: Option<Duration>,
    // Time when the drag-and-drop scroll delta became non-zero, used for debouncing.
    //
    // If `None` then the scroll delta is currently zero.
    dnd_nonzero_start_time: Option<Duration>,
}

/// Where to put a newly added window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MonitorAddWindowTarget<'a, W: LayoutElement> {
    /// No particular preference.
    #[default]
    Auto,
    /// On this workspace.
    Workspace {
        /// Id of the target workspace.
        id: WorkspaceId,
        /// Override where the window will open as a new column.
        column_idx: Option<usize>,
    },
    /// Next to this existing window.
    NextTo(&'a W::Id),
}

niri_render_elements! {
    MonitorRenderElement<R> => {
        Workspace = RelocateRenderElement<RescaleRenderElement<CropRenderElement<
            WorkspaceRenderElement<R>>
            >>,
        Shadow = RelocateRenderElement<RescaleRenderElement<ShadowRenderElement>>,
    }
}

impl WorkspaceSwitch {
    pub fn current_idx(&self) -> f64 {
        match self {
            WorkspaceSwitch::Animation(anim) => anim.value(),
            WorkspaceSwitch::Gesture(gesture) => gesture.current_idx,
        }
    }

    pub fn target_idx(&self) -> f64 {
        match self {
            WorkspaceSwitch::Animation(anim) => anim.to(),
            WorkspaceSwitch::Gesture(gesture) => gesture.current_idx,
        }
    }

    pub fn offset(&mut self, delta: isize) {
        match self {
            WorkspaceSwitch::Animation(anim) => anim.offset(delta as f64),
            WorkspaceSwitch::Gesture(gesture) => {
                if delta >= 0 {
                    gesture.center_idx += delta as usize;
                } else {
                    gesture.center_idx -= (-delta) as usize;
                }
                gesture.current_idx += delta as f64;
            }
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

impl<W: LayoutElement> Monitor<W> {
    pub fn new(
        output: Output,
        workspaces: Vec<Workspace<W>>,
        clock: Clock,
        options: Rc<Options>,
    ) -> Self {
        Self {
            output_name: output.name(),
            output,
            workspaces,
            active_workspace_idx: 0,
            previous_workspace_id: None,
            overview_open: false,
            overview_progress: None,
            workspace_switch: None,
            clock,
            options,
        }
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    pub fn output_name(&self) -> &String {
        &self.output_name
    }

    pub fn active_workspace_idx(&self) -> usize {
        self.active_workspace_idx
    }

    pub fn active_workspace_ref(&self) -> &Workspace<W> {
        &self.workspaces[self.active_workspace_idx]
    }

    pub fn find_named_workspace(&self, workspace_name: &str) -> Option<&Workspace<W>> {
        self.workspaces.iter().find(|ws| {
            ws.name
                .as_ref()
                .is_some_and(|name| name.eq_ignore_ascii_case(workspace_name))
        })
    }

    pub fn find_named_workspace_index(&self, workspace_name: &str) -> Option<usize> {
        self.workspaces.iter().position(|ws| {
            ws.name
                .as_ref()
                .is_some_and(|name| name.eq_ignore_ascii_case(workspace_name))
        })
    }

    pub fn active_workspace(&mut self) -> &mut Workspace<W> {
        &mut self.workspaces[self.active_workspace_idx]
    }

    pub fn windows(&self) -> impl Iterator<Item = &W> {
        self.workspaces.iter().flat_map(|ws| ws.windows())
    }

    pub fn has_window(&self, window: &W::Id) -> bool {
        self.windows().any(|win| win.id() == window)
    }

    pub fn add_workspace_top(&mut self) {
        let ws = Workspace::new(
            self.output.clone(),
            self.clock.clone(),
            self.options.clone(),
        );
        self.workspaces.insert(0, ws);
        self.active_workspace_idx += 1;

        if let Some(switch) = &mut self.workspace_switch {
            switch.offset(1);
        }
    }

    pub fn add_workspace_bottom(&mut self) {
        let ws = Workspace::new(
            self.output.clone(),
            self.clock.clone(),
            self.options.clone(),
        );
        self.workspaces.push(ws);
    }

    fn activate_workspace(&mut self, idx: usize) {
        if self.active_workspace_idx == idx {
            return;
        }

        // FIXME: also compute and use current velocity.
        let current_idx = self
            .workspace_switch
            .as_ref()
            .map(|s| s.current_idx())
            .unwrap_or(self.active_workspace_idx as f64);

        self.previous_workspace_id = Some(self.workspaces[self.active_workspace_idx].id());

        self.active_workspace_idx = idx;

        self.workspace_switch = Some(WorkspaceSwitch::Animation(Animation::new(
            self.clock.clone(),
            current_idx,
            idx as f64,
            0.,
            self.options.animations.workspace_switch.0,
        )));
    }

    pub fn add_window(
        &mut self,
        window: W,
        target: MonitorAddWindowTarget<W>,
        activate: ActivateWindow,
        width: ColumnWidth,
        is_full_width: bool,
        is_floating: bool,
    ) {
        // Currently, everything a workspace sets on a Tile is the same across all workspaces of a
        // monitor. So we can use any workspace, not necessarily the exact target workspace.
        let tile = self.workspaces[0].make_tile(window);

        self.add_tile(tile, target, activate, width, is_full_width, is_floating);
    }

    pub fn add_column(&mut self, mut workspace_idx: usize, column: Column<W>, activate: bool) {
        let workspace = &mut self.workspaces[workspace_idx];

        workspace.add_column(column, activate);

        // After adding a new window, workspace becomes this output's own.
        if workspace.name().is_none() {
            workspace.original_output = OutputId::new(&self.output);
        }

        if workspace_idx == self.workspaces.len() - 1 {
            self.add_workspace_bottom();
        }
        if self.options.empty_workspace_above_first && workspace_idx == 0 {
            self.add_workspace_top();
            workspace_idx += 1;
        }

        if activate {
            self.activate_workspace(workspace_idx);
        }
    }

    pub fn add_tile(
        &mut self,
        tile: Tile<W>,
        target: MonitorAddWindowTarget<W>,
        activate: ActivateWindow,
        width: ColumnWidth,
        is_full_width: bool,
        is_floating: bool,
    ) {
        let (mut workspace_idx, target) = match target {
            MonitorAddWindowTarget::Auto => {
                (self.active_workspace_idx, WorkspaceAddWindowTarget::Auto)
            }
            MonitorAddWindowTarget::Workspace { id, column_idx } => {
                let idx = self.workspaces.iter().position(|ws| ws.id() == id).unwrap();
                let target = if let Some(column_idx) = column_idx {
                    WorkspaceAddWindowTarget::NewColumnAt(column_idx)
                } else {
                    WorkspaceAddWindowTarget::Auto
                };
                (idx, target)
            }
            MonitorAddWindowTarget::NextTo(win_id) => {
                let idx = self
                    .workspaces
                    .iter_mut()
                    .position(|ws| ws.has_window(win_id))
                    .unwrap();
                (idx, WorkspaceAddWindowTarget::NextTo(win_id))
            }
        };

        let workspace = &mut self.workspaces[workspace_idx];

        workspace.add_tile(tile, target, activate, width, is_full_width, is_floating);

        // After adding a new window, workspace becomes this output's own.
        if workspace.name().is_none() {
            workspace.original_output = OutputId::new(&self.output);
        }

        if workspace_idx == self.workspaces.len() - 1 {
            // Insert a new empty workspace.
            self.add_workspace_bottom();
        }

        if self.options.empty_workspace_above_first && workspace_idx == 0 {
            self.add_workspace_top();
            workspace_idx += 1;
        }

        if activate.map_smart(|| false) {
            self.activate_workspace(workspace_idx);
        }
    }

    pub fn add_tile_to_column(
        &mut self,
        workspace_idx: usize,
        column_idx: usize,
        tile_idx: Option<usize>,
        tile: Tile<W>,
        activate: bool,
    ) {
        let workspace = &mut self.workspaces[workspace_idx];

        workspace.add_tile_to_column(column_idx, tile_idx, tile, activate);

        // After adding a new window, workspace becomes this output's own.
        if workspace.name().is_none() {
            workspace.original_output = OutputId::new(&self.output);
        }

        // Since we're adding window to an existing column, the workspace isn't empty, and
        // therefore cannot be the last one, so we never need to insert a new empty workspace.

        if activate {
            self.activate_workspace(workspace_idx);
        }
    }

    pub fn clean_up_workspaces(&mut self) {
        assert!(self.workspace_switch.is_none());

        let range_start = if self.options.empty_workspace_above_first {
            1
        } else {
            0
        };
        for idx in (range_start..self.workspaces.len() - 1).rev() {
            if self.active_workspace_idx == idx {
                continue;
            }

            if !self.workspaces[idx].has_windows_or_name() {
                self.workspaces.remove(idx);
                if self.active_workspace_idx > idx {
                    self.active_workspace_idx -= 1;
                }
            }
        }

        // Special case handling when empty_workspace_above_first is set and all workspaces
        // are empty.
        if self.options.empty_workspace_above_first && self.workspaces.len() == 2 {
            assert!(!self.workspaces[0].has_windows_or_name());
            assert!(!self.workspaces[1].has_windows_or_name());
            self.workspaces.remove(1);
            self.active_workspace_idx = 0;
        }
    }

    pub fn unname_workspace(&mut self, id: WorkspaceId) -> bool {
        let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id() == id) else {
            return false;
        };

        ws.unname();
        true
    }

    pub fn move_down_or_to_workspace_down(&mut self) {
        if !self.active_workspace().move_down() {
            self.move_to_workspace_down();
        }
    }

    pub fn move_up_or_to_workspace_up(&mut self) {
        if !self.active_workspace().move_up() {
            self.move_to_workspace_up();
        }
    }

    pub fn focus_window_or_workspace_down(&mut self) {
        if !self.active_workspace().focus_down() {
            self.switch_workspace_down();
        }
    }

    pub fn focus_window_or_workspace_up(&mut self) {
        if !self.active_workspace().focus_up() {
            self.switch_workspace_up();
        }
    }

    pub fn move_to_workspace_up(&mut self) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = source_workspace_idx.saturating_sub(1);
        if new_idx == source_workspace_idx {
            return;
        }
        let new_id = self.workspaces[new_idx].id();

        let workspace = &mut self.workspaces[source_workspace_idx];
        let Some(removed) = workspace.remove_active_tile(Transaction::new()) else {
            return;
        };

        self.add_tile(
            removed.tile,
            MonitorAddWindowTarget::Workspace {
                id: new_id,
                column_idx: None,
            },
            ActivateWindow::Yes,
            removed.width,
            removed.is_full_width,
            removed.is_floating,
        );
    }

    pub fn move_to_workspace_down(&mut self) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(source_workspace_idx + 1, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }
        let new_id = self.workspaces[new_idx].id();

        let workspace = &mut self.workspaces[source_workspace_idx];
        let Some(removed) = workspace.remove_active_tile(Transaction::new()) else {
            return;
        };

        self.add_tile(
            removed.tile,
            MonitorAddWindowTarget::Workspace {
                id: new_id,
                column_idx: None,
            },
            ActivateWindow::Yes,
            removed.width,
            removed.is_full_width,
            removed.is_floating,
        );
    }

    pub fn move_to_workspace(
        &mut self,
        window: Option<&W::Id>,
        idx: usize,
        activate: ActivateWindow,
    ) {
        let source_workspace_idx = if let Some(window) = window {
            self.workspaces
                .iter()
                .position(|ws| ws.has_window(window))
                .unwrap()
        } else {
            self.active_workspace_idx
        };

        let new_idx = min(idx, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }
        let new_id = self.workspaces[new_idx].id();

        let activate = activate.map_smart(|| {
            window.map_or(true, |win| {
                self.active_window().map(|win| win.id()) == Some(win)
            })
        });

        let workspace = &mut self.workspaces[source_workspace_idx];
        let transaction = Transaction::new();
        let removed = if let Some(window) = window {
            workspace.remove_tile(window, transaction)
        } else if let Some(removed) = workspace.remove_active_tile(transaction) {
            removed
        } else {
            return;
        };

        self.add_tile(
            removed.tile,
            MonitorAddWindowTarget::Workspace {
                id: new_id,
                column_idx: None,
            },
            if activate {
                ActivateWindow::Yes
            } else {
                ActivateWindow::No
            },
            removed.width,
            removed.is_full_width,
            removed.is_floating,
        );

        if self.workspace_switch.is_none() {
            self.clean_up_workspaces();
        }
    }

    pub fn move_column_to_workspace_up(&mut self) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = source_workspace_idx.saturating_sub(1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.floating_is_active() {
            self.move_to_workspace_up();
            return;
        }

        let Some(column) = workspace.remove_active_column() else {
            return;
        };

        self.add_column(new_idx, column, true);
    }

    pub fn move_column_to_workspace_down(&mut self) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(source_workspace_idx + 1, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.floating_is_active() {
            self.move_to_workspace_down();
            return;
        }

        let Some(column) = workspace.remove_active_column() else {
            return;
        };

        self.add_column(new_idx, column, true);
    }

    pub fn move_column_to_workspace(&mut self, idx: usize) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(idx, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.floating_is_active() {
            self.move_to_workspace(None, idx, ActivateWindow::Smart);
            return;
        }

        let Some(column) = workspace.remove_active_column() else {
            return;
        };

        self.add_column(new_idx, column, true);
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

    fn previous_workspace_idx(&self) -> Option<usize> {
        let id = self.previous_workspace_id?;
        self.workspaces.iter().position(|w| w.id() == id)
    }

    pub fn switch_workspace(&mut self, idx: usize) {
        self.activate_workspace(min(idx, self.workspaces.len() - 1));
    }

    pub fn switch_workspace_auto_back_and_forth(&mut self, idx: usize) {
        let idx = min(idx, self.workspaces.len() - 1);

        if idx == self.active_workspace_idx {
            if let Some(prev_idx) = self.previous_workspace_idx() {
                self.switch_workspace(prev_idx);
            }
        } else {
            self.switch_workspace(idx);
        }
    }

    pub fn switch_workspace_previous(&mut self) {
        if let Some(idx) = self.previous_workspace_idx() {
            self.switch_workspace(idx);
        }
    }

    pub fn active_window(&self) -> Option<&W> {
        self.active_workspace_ref().active_window()
    }

    pub fn advance_animations(&mut self) {
        if let Some(WorkspaceSwitch::Animation(anim)) = &mut self.workspace_switch {
            if anim.is_done() {
                self.workspace_switch = None;
                self.clean_up_workspaces();
            }
        }

        for ws in &mut self.workspaces {
            ws.advance_animations();
        }
    }

    pub(super) fn are_animations_ongoing(&self) -> bool {
        self.workspace_switch
            .as_ref()
            .is_some_and(|s| s.is_animation())
            || self.workspaces.iter().any(|ws| ws.are_animations_ongoing())
    }

    pub fn are_transitions_ongoing(&self) -> bool {
        self.workspace_switch.is_some()
            || self
                .workspaces
                .iter()
                .any(|ws| ws.are_transitions_ongoing())
    }

    pub fn update_render_elements(&mut self, is_active: bool) {
        let is_overview_open = self.overview_open;
        for (ws, _) in self.workspaces_with_render_geo_mut() {
            ws.update_render_elements(is_active, is_overview_open);
        }
    }

    pub fn update_config(&mut self, options: Rc<Options>) {
        if self.options.empty_workspace_above_first != options.empty_workspace_above_first
            && self.workspaces.len() > 1
        {
            if options.empty_workspace_above_first {
                self.add_workspace_top();
            } else if self.workspace_switch.is_none() && self.active_workspace_idx != 0 {
                self.workspaces.remove(0);
                self.active_workspace_idx = self.active_workspace_idx.saturating_sub(1);
            }
        }

        for ws in &mut self.workspaces {
            ws.update_config(options.clone());
        }

        self.options = options;
    }

    pub fn move_workspace_down(&mut self) {
        let mut new_idx = min(self.active_workspace_idx + 1, self.workspaces.len() - 1);
        if new_idx == self.active_workspace_idx {
            return;
        }

        self.workspaces.swap(self.active_workspace_idx, new_idx);

        if new_idx == self.workspaces.len() - 1 {
            // Insert a new empty workspace.
            self.add_workspace_bottom();
        }

        if self.options.empty_workspace_above_first && self.active_workspace_idx == 0 {
            self.add_workspace_top();
            new_idx += 1;
        }

        let previous_workspace_id = self.previous_workspace_id;
        self.activate_workspace(new_idx);
        self.workspace_switch = None;
        self.previous_workspace_id = previous_workspace_id;

        self.clean_up_workspaces();
    }

    pub fn move_workspace_up(&mut self) {
        let mut new_idx = self.active_workspace_idx.saturating_sub(1);
        if new_idx == self.active_workspace_idx {
            return;
        }

        self.workspaces.swap(self.active_workspace_idx, new_idx);

        if self.active_workspace_idx == self.workspaces.len() - 1 {
            // Insert a new empty workspace.
            self.add_workspace_bottom();
        }

        if self.options.empty_workspace_above_first && new_idx == 0 {
            self.add_workspace_top();
            new_idx += 1;
        }

        let previous_workspace_id = self.previous_workspace_id;
        self.activate_workspace(new_idx);
        self.workspace_switch = None;
        self.previous_workspace_id = previous_workspace_id;

        self.clean_up_workspaces();
    }

    pub fn move_workspace_to_idx(&mut self, old_idx: usize, new_idx: usize) {
        let mut new_idx = new_idx.clamp(0, self.workspaces.len() - 1);
        if old_idx == new_idx {
            return;
        }

        let ws = self.workspaces.remove(old_idx);
        self.workspaces.insert(new_idx, ws);

        if new_idx > old_idx {
            if new_idx == self.workspaces.len() - 1 {
                // Insert a new empty workspace.
                self.add_workspace_bottom();
            }

            if self.options.empty_workspace_above_first && old_idx == 0 {
                self.add_workspace_top();
                new_idx += 1;
            }
        } else {
            if old_idx == self.workspaces.len() - 1 {
                // Insert a new empty workspace.
                self.add_workspace_bottom();
            }

            if self.options.empty_workspace_above_first && new_idx == 0 {
                self.add_workspace_top();
                new_idx += 1;
            }
        }

        // Only refocus the workspace if it was already focused
        if self.active_workspace_idx == old_idx {
            self.active_workspace_idx = new_idx;
        // If the workspace order was switched so that the current workspace moved down the
        // workspace stack, focus correctly
        } else if new_idx <= self.active_workspace_idx && old_idx > self.active_workspace_idx {
            self.active_workspace_idx += 1;
        } else if new_idx >= self.active_workspace_idx && old_idx < self.active_workspace_idx {
            self.active_workspace_idx = self.active_workspace_idx.saturating_sub(1);
        }

        self.workspace_switch = None;

        self.clean_up_workspaces();
    }

    /// Returns the geometry of the active tile relative to and clamped to the output.
    ///
    /// During animations, assumes the final view position.
    pub fn active_tile_visual_rectangle(&self) -> Option<Rectangle<f64, Logical>> {
        if self.overview_open {
            return None;
        }
        // TODO: unify logic
        let mut rect = self.active_workspace_ref().active_tile_visual_rectangle()?;

        if let Some(switch) = &self.workspace_switch {
            let size = output_size(&self.output).to_f64();

            let offset = switch.target_idx() - self.active_workspace_idx as f64;
            let offset = offset * size.h;

            let clip_rect = Rectangle::new(Point::from((0., -offset)), size);
            rect = rect.intersection(clip_rect)?;
        }

        Some(rect)
    }

    pub fn workspace_scale(&self) -> f64 {
        if let Some(v) = self.overview_progress {
            (1. - v * (1. - OVERVIEW_WORKSPACE_SCALE)).max(0.)
        } else {
            1.
        }
    }

    pub fn workspaces_render_geo(&self) -> impl Iterator<Item = Rectangle<f64, Logical>> {
        let render_idx = if let Some(switch) = &self.workspace_switch {
            switch.current_idx()
        } else {
            self.active_workspace_idx as f64
        };

        let before_idx = render_idx.floor();

        let scale = self.output.current_scale().fractional_scale();
        let size = output_size(&self.output);

        let ws_scale = self.workspace_scale();

        let gap = round_logical_in_physical(scale, size.h * 0.1 * ws_scale);

        let static_offset = Point::from((size.w, size.h))
            .upscale((1. - ws_scale) / 2.)
            .to_physical_precise_round(scale)
            .to_logical(scale);

        // Ceil the workspace size in physical pixels.
        let ws_size = Size::from((size.w, size.h))
            .upscale(ws_scale)
            .to_physical_precise_ceil(scale)
            .to_logical(scale);

        // Compute the offset in such a way that if render_idx is active_workspace_idx and the
        // workspace scale is 1., then its offset will be (0., 0.).
        let before_ws_y = (before_idx - render_idx) * (ws_size.h + gap);

        let first_ws_y = before_ws_y - (ws_size.h + gap) * before_idx;

        (0..self.workspaces.len()).map(move |idx| {
            let y = first_ws_y + idx as f64 * (ws_size.h + gap);
            let loc = Point::from((0., y)) + static_offset;
            let loc = loc.to_physical_precise_round(scale).to_logical(scale);
            Rectangle::new(loc, ws_size)
        })
    }

    pub fn workspaces_with_render_geo(
        &self,
    ) -> impl Iterator<Item = (&Workspace<W>, Rectangle<f64, Logical>)> {
        let output_size = output_size(&self.output);
        let output_geo = Rectangle::new(Point::from((0., 0.)), output_size);

        let geo = self.workspaces_render_geo();
        zip(self.workspaces.iter(), geo)
            // Cull out workspaces outside the output.
            .filter(move |(_ws, geo)| geo.intersection(output_geo).is_some())
    }

    pub fn workspaces_with_render_geo_mut(
        &mut self,
    ) -> impl Iterator<Item = (&mut Workspace<W>, Rectangle<f64, Logical>)> {
        let output_size = output_size(&self.output);
        let output_geo = Rectangle::new(Point::from((0., 0.)), output_size);

        let geo = self.workspaces_render_geo();
        zip(self.workspaces.iter_mut(), geo)
            // Cull out workspaces outside the output.
            .filter(move |(_ws, geo)| geo.intersection(output_geo).is_some())
    }

    pub fn workspace_under(
        &self,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<(&Workspace<W>, Rectangle<f64, Logical>)> {
        let size = output_size(&self.output);
        let (ws, geo) = self.workspaces_with_render_geo().find_map(|(ws, geo)| {
            // Extend width to entire output.
            let loc = Point::from((0., geo.loc.y));
            let size = Size::from((size.w, geo.size.h));
            let bounds = Rectangle::new(loc, size);

            bounds.contains(pos_within_output).then_some((ws, geo))
        })?;
        Some((ws, geo))
    }

    pub fn workspace_under_narrow(
        &self,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<&Workspace<W>> {
        self.workspaces_with_render_geo()
            .find_map(|(ws, geo)| geo.contains(pos_within_output).then_some(ws))
    }

    pub fn window_under(&self, pos_within_output: Point<f64, Logical>) -> Option<(&W, HitType)> {
        let (ws, geo) = self.workspace_under(pos_within_output)?;

        if self.overview_progress.is_some() {
            let ws_scale = self.workspace_scale().max(0.0001);
            let pos_within_workspace = (pos_within_output - geo.loc).downscale(ws_scale);
            let (win, hit) = ws.window_under(pos_within_workspace)?;
            // During the overview animation, we cannot do input hits because we cannot really
            // represent scaled windows properly.
            Some((win, hit.to_activate()))
        } else {
            let (win, hit) = ws.window_under(pos_within_output - geo.loc)?;
            Some((win, hit.offset_win_pos(geo.loc)))
        }
    }

    pub fn resize_edges_under(&self, pos_within_output: Point<f64, Logical>) -> Option<ResizeEdge> {
        if self.overview_progress.is_some() {
            return None;
        }

        let (ws, geo) = self.workspace_under(pos_within_output)?;
        ws.resize_edges_under(pos_within_output - geo.loc)
    }

    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer only if the view is stationary.
        if self.workspace_switch.is_some() || self.overview_progress.is_some() {
            return false;
        }

        let ws = &self.workspaces[self.active_workspace_idx];
        ws.render_above_top_layer()
    }

    pub fn render_elements<'a, R: NiriRenderer>(
        &'a self,
        renderer: &'a mut R,
        target: RenderTarget,
        focus_ring: bool,
    ) -> impl Iterator<
        Item = (
            Rectangle<f64, Logical>,
            impl Iterator<Item = MonitorRenderElement<R>>,
        ),
    > + 'a {
        let _span = tracy_client::span!("Monitor::render_elements");

        let scale = self.output.current_scale().fractional_scale();
        let size = output_size(&self.output);
        // Ceil the height in physical pixels.
        let height = (size.h * scale).ceil() as i32;

        // Crop the elements to prevent them overflowing, currently visible during a workspace
        // switch.
        //
        // HACK: crop to infinite bounds at least horizontally where we
        // know there's no workspace joining or monitor bounds, otherwise
        // it will cut pixel shaders and mess up the coordinate space.
        // There's also a damage tracking bug which causes glitched
        // rendering for maximized GTK windows.
        //
        // FIXME: use proper bounds after fixing the Crop element.
        let crop_bounds = if self.workspace_switch.is_some() || self.overview_progress.is_some() {
            Rectangle::new(
                Point::from((-i32::MAX / 2, 0)),
                Size::from((i32::MAX, height)),
            )
        } else {
            Rectangle::new(
                Point::from((-i32::MAX / 2, -i32::MAX / 2)),
                Size::from((i32::MAX, i32::MAX)),
            )
        };

        let ws_scale = self.workspace_scale();
        // FIXME: would be better to use clamped_value() here, but not a big deal.
        let overview_anim_value = self.overview_progress;
        let is_overview_open = self.overview_open;
        self.workspaces_with_render_geo().map(move |(ws, geo)| {
            let iter = ws
                .render_elements(renderer, target, focus_ring, is_overview_open)
                .filter_map(move |elem| CropRenderElement::from_element(elem, scale, crop_bounds))
                // .map(move |elem| {
                //     let elem_scale = 1. - (1. - ws_scale) / OVERVIEW_WORKSPACE_SCALE * 0.03;
                //     RescaleRenderElement::from_element(
                //         elem,
                //         size.downscale(2.)
                //             .to_physical_precise_round(scale)
                //             .to_point(),
                //         elem_scale,
                //     )
                // })
                .map(move |elem| {
                    RescaleRenderElement::from_element(elem, Point::from((0, 0)), ws_scale)
                })
                .map(move |elem| {
                    RelocateRenderElement::from_element(
                        elem,
                        // The offset we get from workspaces_with_render_positions() is already
                        // rounded to physical pixels, but it's in the logical coordinate
                        // space, so we need to convert it to physical.
                        geo.loc.to_physical_precise_round(scale),
                        Relocate::Relative,
                    )
                })
                .map(MonitorRenderElement::Workspace);
            let shadow = if let Some(value) = overview_anim_value {
                Vec::from_iter(
                    ws.render_shadow(renderer)
                        .map(move |elem| elem.with_alpha(value.clamp(0., 1.) as f32))
                        .map(move |elem| {
                            RescaleRenderElement::from_element(elem, Point::from((0, 0)), ws_scale)
                        })
                        .map(move |elem| {
                            RelocateRenderElement::from_element(
                                elem,
                                geo.loc.to_physical_precise_round(scale),
                                Relocate::Relative,
                            )
                        })
                        .map(MonitorRenderElement::Shadow),
                )
            } else {
                Vec::new()
            };
            (geo, iter.chain(shadow))
        })
    }

    pub fn workspace_switch_gesture_begin(&mut self, is_touchpad: bool) {
        let center_idx = self.active_workspace_idx;
        let current_idx = self
            .workspace_switch
            .as_ref()
            .map(|s| s.current_idx())
            .unwrap_or(center_idx as f64);

        let gesture = WorkspaceSwitchGesture {
            center_idx,
            current_idx,
            tracker: SwipeTracker::new(),
            is_touchpad,
            is_clamped: !self.overview_open,
            dnd_last_event_time: None,
            dnd_nonzero_start_time: None,
        };
        self.workspace_switch = Some(WorkspaceSwitch::Gesture(gesture));
    }

    pub fn dnd_scroll_gesture_begin(&mut self) {
        if let Some(WorkspaceSwitch::Gesture(WorkspaceSwitchGesture {
            dnd_last_event_time: Some(_),
            ..
        })) = &self.workspace_switch
        {
            // Already active.
            return;
        }

        if !self.overview_open {
            // This gesture is only for the overview.
            return;
        }

        let center_idx = self.active_workspace_idx;
        let current_idx = self
            .workspace_switch
            .as_ref()
            .map(|s| s.current_idx())
            .unwrap_or(center_idx as f64);

        let gesture = WorkspaceSwitchGesture {
            center_idx,
            current_idx,
            tracker: SwipeTracker::new(),
            is_touchpad: false,
            is_clamped: false,
            dnd_last_event_time: Some(self.clock.now_unadjusted()),
            dnd_nonzero_start_time: None,
        };
        self.workspace_switch = Some(WorkspaceSwitch::Gesture(gesture));
    }

    pub fn workspace_switch_gesture_update(
        &mut self,
        delta_y: f64,
        timestamp: Duration,
        is_touchpad: bool,
    ) -> Option<bool> {
        let ws_scale = self.workspace_scale().max(0.0001);

        let Some(WorkspaceSwitch::Gesture(gesture)) = &mut self.workspace_switch else {
            return None;
        };

        if gesture.is_touchpad != is_touchpad || gesture.dnd_last_event_time.is_some() {
            return None;
        }

        // Reduce the effect of ws_scale on the touchpad somewhat.
        let delta_scale = if gesture.is_touchpad {
            (ws_scale - 1.) / 2.5 + 1.
        } else {
            ws_scale
        };

        let delta_y = delta_y / delta_scale;
        let mut rubber_band = WORKSPACE_GESTURE_RUBBER_BAND;
        rubber_band.limit /= ws_scale;

        gesture.tracker.push(delta_y, timestamp);

        let total_height = if gesture.is_touchpad {
            WORKSPACE_GESTURE_MOVEMENT
        } else {
            // Account for the gap.
            self.workspaces[0].view_size().h * 1.1
        };
        let pos = gesture.tracker.pos() / total_height;

        let (min, max) = if gesture.is_clamped {
            let min = gesture.center_idx.saturating_sub(1) as f64;
            let max = (gesture.center_idx + 1).min(self.workspaces.len() - 1) as f64;
            (min, max)
        } else {
            (0., (self.workspaces.len() - 1) as f64)
        };
        let new_idx = gesture.center_idx as f64 + pos;
        let new_idx = rubber_band.clamp(min, max, new_idx);

        if gesture.current_idx == new_idx {
            return Some(false);
        }

        gesture.current_idx = new_idx;
        Some(true)
    }

    pub fn dnd_scroll_gesture_scroll(&mut self, pos: Point<f64, Logical>, speed: f64) {
        let Some(WorkspaceSwitch::Gesture(gesture)) = &mut self.workspace_switch else {
            return;
        };

        let Some(last_time) = gesture.dnd_last_event_time else {
            // Not a DnD scroll.
            return;
        };

        let config = &self.options.gestures.dnd_edge_workspace_switch;
        let trigger_height = config.trigger_height.0;

        // This working area intentionally does not include extra struts from Options.
        // TODO: working area
        let y = pos.y;
        let height = output_size(&self.output).h;

        let y = y.clamp(0., height);
        let trigger_height = trigger_height.clamp(0., height / 2.);

        let delta = if y < trigger_height {
            -(trigger_height - y)
        } else if height - y < trigger_height {
            trigger_height - (height - y)
        } else {
            0.
        };

        let delta = if trigger_height < 0.01 {
            // Sanity check for trigger-height 0 or small window sizes.
            0.
        } else {
            // Normalize to [0, 1].
            delta / trigger_height
        };
        let delta = delta * speed;

        let now = self.clock.now_unadjusted();
        gesture.dnd_last_event_time = Some(now);

        if delta == 0. {
            // We're outside the scrolling zone.
            gesture.dnd_nonzero_start_time = None;
            return;
        }

        let nonzero_start = *gesture.dnd_nonzero_start_time.get_or_insert(now);

        // Delay starting the gesture a bit to avoid unwanted movement when dragging across
        // monitors.
        let delay = Duration::from_millis(u64::from(config.delay_ms));
        if now.saturating_sub(nonzero_start) < delay {
            return;
        }

        let time_delta = now.saturating_sub(last_time).as_secs_f64();

        let delta = delta * time_delta * config.max_speed.0;

        gesture.tracker.push(delta, now);

        // Account for the gap.
        let total_height = self.workspaces[0].view_size().h * 1.1;
        let pos = gesture.tracker.pos() / total_height;

        let (min, max) = if gesture.is_clamped {
            let min = gesture.center_idx.saturating_sub(1) as f64;
            let max = (gesture.center_idx + 1).min(self.workspaces.len() - 1) as f64;
            (min, max)
        } else {
            (0., (self.workspaces.len() - 1) as f64)
        };
        let new_idx = gesture.center_idx as f64 + pos;
        let new_idx = new_idx.clamp(min, max);

        gesture.current_idx = new_idx;
    }

    pub fn workspace_switch_gesture_end(
        &mut self,
        cancelled: bool,
        is_touchpad: Option<bool>,
    ) -> bool {
        let ws_scale = self.workspace_scale().max(0.0001);

        let Some(WorkspaceSwitch::Gesture(gesture)) = &mut self.workspace_switch else {
            return false;
        };

        if is_touchpad.is_some_and(|x| gesture.is_touchpad != x) {
            return false;
        }

        if cancelled {
            self.workspace_switch = None;
            self.clean_up_workspaces();
            return true;
        }

        // Take into account any idle time between the last event and now.
        let now = self.clock.now_unadjusted();
        gesture.tracker.push(0., now);

        let mut rubber_band = WORKSPACE_GESTURE_RUBBER_BAND;
        rubber_band.limit /= ws_scale;

        let total_height = if gesture.is_touchpad {
            WORKSPACE_GESTURE_MOVEMENT
        } else {
            // Account for the gap.
            self.workspaces[0].view_size().h * 1.1
        };

        let mut velocity = gesture.tracker.velocity() / total_height;
        let current_pos = gesture.tracker.pos() / total_height;
        let pos = gesture.tracker.projected_end_pos() / total_height;

        let (min, max) = if gesture.is_clamped {
            let min = gesture.center_idx.saturating_sub(1) as f64;
            let max = (gesture.center_idx + 1).min(self.workspaces.len() - 1) as f64;
            (min, max)
        } else {
            (0., (self.workspaces.len() - 1) as f64)
        };
        let new_idx = gesture.center_idx as f64 + pos;

        let new_idx = new_idx.clamp(min, max);
        let new_idx = new_idx.round() as usize;

        velocity *= rubber_band.clamp_derivative(min, max, gesture.center_idx as f64 + current_pos);

        self.previous_workspace_id = Some(self.workspaces[self.active_workspace_idx].id());

        self.active_workspace_idx = new_idx;
        self.workspace_switch = Some(WorkspaceSwitch::Animation(Animation::new(
            self.clock.clone(),
            gesture.current_idx,
            new_idx as f64,
            velocity,
            self.options.animations.workspace_switch.0,
        )));

        true
    }

    pub fn dnd_scroll_gesture_end(&mut self) {
        if !matches!(
            self.workspace_switch,
            Some(WorkspaceSwitch::Gesture(WorkspaceSwitchGesture {
                dnd_last_event_time: Some(_),
                ..
            }))
        ) {
            // Not a DnD scroll.
            return;
        };

        self.workspace_switch_gesture_end(false, None);
    }
}
