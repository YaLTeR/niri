use std::cmp::min;
use std::rc::Rc;
use std::time::Duration;

use niri_ipc::SizeChange;
use smithay::backend::renderer::element::utils::{
    CropRenderElement, Relocate, RelocateRenderElement,
};
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle};

use super::scrolling::{Column, ColumnWidth};
use super::tile::Tile;
use super::workspace::{
    OutputId, Workspace, WorkspaceAddWindowTarget, WorkspaceId, WorkspaceRenderElement,
};
use super::{ActivateWindow, LayoutElement, Options};
use crate::animation::{Animation, Clock};
use crate::input::swipe_tracker::SwipeTracker;
use crate::render_helpers::renderer::NiriRenderer;
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

pub type MonitorRenderElement<R> =
    RelocateRenderElement<CropRenderElement<WorkspaceRenderElement<R>>>;

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
                .map_or(false, |name| name.eq_ignore_ascii_case(workspace_name))
        })
    }

    pub fn find_named_workspace_index(&self, workspace_name: &str) -> Option<usize> {
        self.workspaces.iter().position(|ws| {
            ws.name
                .as_ref()
                .map_or(false, |name| name.eq_ignore_ascii_case(workspace_name))
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
        workspace.original_output = OutputId::new(&self.output);

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
        workspace.original_output = OutputId::new(&self.output);

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
        workspace.original_output = OutputId::new(&self.output);

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

    pub fn unname_workspace(&mut self, workspace_name: &str) -> bool {
        for ws in &mut self.workspaces {
            if ws
                .name
                .as_ref()
                .map_or(false, |name| name.eq_ignore_ascii_case(workspace_name))
            {
                ws.unname();
                return true;
            }
        }
        false
    }

    pub fn move_left(&mut self) -> bool {
        self.active_workspace().move_left()
    }

    pub fn move_right(&mut self) -> bool {
        self.active_workspace().move_right()
    }

    pub fn move_column_to_first(&mut self) {
        self.active_workspace().move_column_to_first();
    }

    pub fn move_column_to_last(&mut self) {
        self.active_workspace().move_column_to_last();
    }

    pub fn move_down(&mut self) {
        self.active_workspace().move_down();
    }

    pub fn move_up(&mut self) {
        self.active_workspace().move_up();
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

    pub fn focus_left(&mut self) -> bool {
        self.active_workspace().focus_left()
    }

    pub fn focus_right(&mut self) -> bool {
        self.active_workspace().focus_right()
    }

    pub fn focus_column_first(&mut self) {
        self.active_workspace().focus_column_first();
    }

    pub fn focus_column_last(&mut self) {
        self.active_workspace().focus_column_last();
    }

    pub fn focus_column_right_or_first(&mut self) {
        self.active_workspace().focus_column_right_or_first();
    }

    pub fn focus_column_left_or_last(&mut self) {
        self.active_workspace().focus_column_left_or_last();
    }

    pub fn focus_down(&mut self) -> bool {
        self.active_workspace().focus_down()
    }

    pub fn focus_up(&mut self) -> bool {
        self.active_workspace().focus_up()
    }

    pub fn focus_down_or_left(&mut self) {
        self.active_workspace().focus_down_or_left();
    }

    pub fn focus_down_or_right(&mut self) {
        self.active_workspace().focus_down_or_right();
    }

    pub fn focus_up_or_left(&mut self) {
        self.active_workspace().focus_up_or_left();
    }

    pub fn focus_up_or_right(&mut self) {
        self.active_workspace().focus_up_or_right();
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

    pub fn move_to_workspace(&mut self, window: Option<&W::Id>, idx: usize) {
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

        let activate = window.map_or(true, |win| {
            self.active_window().map(|win| win.id()) == Some(win)
        });
        let activate = if activate {
            ActivateWindow::Yes
        } else {
            ActivateWindow::No
        };

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
            activate,
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
            self.move_to_workspace(None, idx);
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

    pub fn consume_into_column(&mut self) {
        self.active_workspace().consume_into_column();
    }

    pub fn expel_from_column(&mut self) {
        self.active_workspace().expel_from_column();
    }

    pub fn center_column(&mut self) {
        self.active_workspace().center_column();
    }

    pub fn active_window(&self) -> Option<&W> {
        self.active_workspace_ref().active_window()
    }

    pub fn is_active_fullscreen(&self) -> bool {
        self.active_workspace_ref().is_active_fullscreen()
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
        match &self.workspace_switch {
            Some(switch) => {
                let render_idx = switch.current_idx();
                let before_idx = render_idx.floor();
                let after_idx = render_idx.ceil();

                if after_idx < 0. || before_idx as usize >= self.workspaces.len() {
                    return;
                }

                let after_idx = after_idx as usize;
                if after_idx < self.workspaces.len() {
                    self.workspaces[after_idx].update_render_elements(is_active);

                    if before_idx < 0. {
                        return;
                    }
                }

                let before_idx = before_idx as usize;
                self.workspaces[before_idx].update_render_elements(is_active);
            }
            None => {
                self.workspaces[self.active_workspace_idx].update_render_elements(is_active);
            }
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

    pub fn toggle_width(&mut self) {
        self.active_workspace().toggle_width();
    }

    pub fn toggle_full_width(&mut self) {
        self.active_workspace().toggle_full_width();
    }

    pub fn set_column_width(&mut self, change: SizeChange) {
        self.active_workspace().set_column_width(change);
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

    /// Returns the geometry of the active tile relative to and clamped to the output.
    ///
    /// During animations, assumes the final view position.
    pub fn active_tile_visual_rectangle(&self) -> Option<Rectangle<f64, Logical>> {
        let mut rect = self.active_workspace_ref().active_tile_visual_rectangle()?;

        if let Some(switch) = &self.workspace_switch {
            let size = output_size(&self.output).to_f64();

            let offset = switch.target_idx() - self.active_workspace_idx as f64;
            let offset = offset * size.h;

            let clip_rect = Rectangle::from_loc_and_size((0., -offset), size);
            rect = rect.intersection(clip_rect)?;
        }

        Some(rect)
    }

    pub fn workspaces_with_render_positions(
        &self,
    ) -> impl Iterator<Item = (&Workspace<W>, Point<f64, Logical>)> {
        let mut first = None;
        let mut second = None;

        match &self.workspace_switch {
            Some(switch) => {
                let render_idx = switch.current_idx();
                let before_idx = render_idx.floor();
                let after_idx = render_idx.ceil();

                if after_idx >= 0. && before_idx < self.workspaces.len() as f64 {
                    let scale = self.output.current_scale().fractional_scale();
                    let size = output_size(&self.output);
                    let offset =
                        round_logical_in_physical(scale, (render_idx - before_idx) * size.h);

                    // Ceil the height in physical pixels.
                    let height = (size.h * scale).ceil() / scale;

                    if before_idx >= 0. {
                        let before_idx = before_idx as usize;
                        let before_offset = Point::from((0., -offset));
                        first = Some((&self.workspaces[before_idx], before_offset));
                    }

                    let after_idx = after_idx as usize;
                    if after_idx < self.workspaces.len() {
                        let after_offset = Point::from((0., -offset + height));
                        second = Some((&self.workspaces[after_idx], after_offset));
                    }
                }
            }
            None => {
                first = Some((
                    &self.workspaces[self.active_workspace_idx],
                    Point::from((0., 0.)),
                ));
            }
        }

        first.into_iter().chain(second)
    }

    pub fn workspace_under(
        &self,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<(&Workspace<W>, Point<f64, Logical>)> {
        let size = output_size(&self.output);
        let (ws, bounds) = self
            .workspaces_with_render_positions()
            .map(|(ws, offset)| (ws, Rectangle::from_loc_and_size(offset, size)))
            .find(|(_, bounds)| bounds.contains(pos_within_output))?;
        Some((ws, bounds.loc))
    }

    pub fn window_under(
        &self,
        pos_within_output: Point<f64, Logical>,
    ) -> Option<(&W, Option<Point<f64, Logical>>)> {
        let (ws, offset) = self.workspace_under(pos_within_output)?;
        let (win, win_pos) = ws.window_under(pos_within_output - offset)?;
        Some((win, win_pos.map(|p| p + offset)))
    }

    pub fn resize_edges_under(&self, pos_within_output: Point<f64, Logical>) -> Option<ResizeEdge> {
        let (ws, offset) = self.workspace_under(pos_within_output)?;
        ws.resize_edges_under(pos_within_output - offset)
    }

    pub fn render_above_top_layer(&self) -> bool {
        // Render above the top layer only if the view is stationary.
        if self.workspace_switch.is_some() {
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
    ) -> impl Iterator<Item = MonitorRenderElement<R>> + 'a {
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
        let crop_bounds = if self.workspace_switch.is_some() {
            Rectangle::from_loc_and_size((-i32::MAX / 2, 0), (i32::MAX, height))
        } else {
            Rectangle::from_loc_and_size((-i32::MAX / 2, -i32::MAX / 2), (i32::MAX, i32::MAX))
        };

        self.workspaces_with_render_positions()
            .flat_map(move |(ws, offset)| {
                ws.render_elements(renderer, target, focus_ring)
                    .filter_map(move |elem| {
                        CropRenderElement::from_element(elem, scale, crop_bounds)
                    })
                    .map(move |elem| {
                        RelocateRenderElement::from_element(
                            elem,
                            // The offset we get from workspaces_with_render_positions() is already
                            // rounded to physical pixels, but it's in the logical coordinate
                            // space, so we need to convert it to physical.
                            offset.to_physical_precise_round(scale),
                            Relocate::Relative,
                        )
                    })
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
        };
        self.workspace_switch = Some(WorkspaceSwitch::Gesture(gesture));
    }

    pub fn workspace_switch_gesture_update(
        &mut self,
        delta_y: f64,
        timestamp: Duration,
        is_touchpad: bool,
    ) -> Option<bool> {
        let Some(WorkspaceSwitch::Gesture(gesture)) = &mut self.workspace_switch else {
            return None;
        };

        if gesture.is_touchpad != is_touchpad {
            return None;
        }

        gesture.tracker.push(delta_y, timestamp);

        let total_height = if gesture.is_touchpad {
            WORKSPACE_GESTURE_MOVEMENT
        } else {
            self.workspaces[0].view_size().h
        };
        let pos = gesture.tracker.pos() / total_height;

        let min = gesture.center_idx.saturating_sub(1) as f64;
        let max = (gesture.center_idx + 1).min(self.workspaces.len() - 1) as f64;
        let new_idx = gesture.center_idx as f64 + pos;
        let new_idx = WORKSPACE_GESTURE_RUBBER_BAND.clamp(min, max, new_idx);

        if gesture.current_idx == new_idx {
            return Some(false);
        }

        gesture.current_idx = new_idx;
        Some(true)
    }

    pub fn workspace_switch_gesture_end(
        &mut self,
        cancelled: bool,
        is_touchpad: Option<bool>,
    ) -> bool {
        let Some(WorkspaceSwitch::Gesture(gesture)) = &mut self.workspace_switch else {
            return false;
        };

        if is_touchpad.map_or(false, |x| gesture.is_touchpad != x) {
            return false;
        }

        if cancelled {
            self.workspace_switch = None;
            self.clean_up_workspaces();
            return true;
        }

        let total_height = if gesture.is_touchpad {
            WORKSPACE_GESTURE_MOVEMENT
        } else {
            self.workspaces[0].view_size().h
        };

        let mut velocity = gesture.tracker.velocity() / total_height;
        let current_pos = gesture.tracker.pos() / total_height;
        let pos = gesture.tracker.projected_end_pos() / total_height;

        let min = gesture.center_idx.saturating_sub(1) as f64;
        let max = (gesture.center_idx + 1).min(self.workspaces.len() - 1) as f64;
        let new_idx = gesture.center_idx as f64 + pos;

        let new_idx = WORKSPACE_GESTURE_RUBBER_BAND.clamp(min, max, new_idx);
        let new_idx = new_idx.round() as usize;

        velocity *= WORKSPACE_GESTURE_RUBBER_BAND.clamp_derivative(
            min,
            max,
            gesture.center_idx as f64 + current_pos,
        );

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
}
