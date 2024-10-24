use std::cmp::min;
use std::rc::Rc;
use std::time::Duration;

use niri_ipc::SizeChange;
use smithay::backend::renderer::element::utils::{
    CropRenderElement, Relocate, RelocateRenderElement,
};
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle};

use super::workspace::{
    compute_working_area, Column, ColumnWidth, OutputId, Workspace, WorkspaceId,
    WorkspaceRenderElement,
};
use super::{LayoutElement, Options};
use crate::animation::Animation;
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

    /// Returns `true` if the workspace switch is [`Animation`].
    ///
    /// [`Animation`]: WorkspaceSwitch::Animation
    #[must_use]
    fn is_animation(&self) -> bool {
        matches!(self, Self::Animation(..))
    }
}

impl<W: LayoutElement> Monitor<W> {
    pub fn new(output: Output, workspaces: Vec<Workspace<W>>, options: Rc<Options>) -> Self {
        Self {
            output_name: output.name(),
            output,
            workspaces,
            active_workspace_idx: 0,
            previous_workspace_id: None,
            workspace_switch: None,
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
            current_idx,
            idx as f64,
            0.,
            self.options.animations.workspace_switch.0,
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

        workspace.add_window(None, window, activate, width, is_full_width);

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

    pub fn add_window_right_of(
        &mut self,
        right_of: &W::Id,
        window: W,
        width: ColumnWidth,
        is_full_width: bool,
    ) {
        let workspace_idx = self
            .workspaces
            .iter_mut()
            .position(|ws| ws.has_window(right_of))
            .unwrap();
        let workspace = &mut self.workspaces[workspace_idx];

        workspace.add_window_right_of(right_of, window, width, is_full_width);

        // After adding a new window, workspace becomes this output's own.
        workspace.original_output = OutputId::new(&self.output);

        // Since we're adding window right of something, the workspace isn't empty, and therefore
        // cannot be the last one, so we never need to insert a new empty workspace.
    }

    pub fn add_column(&mut self, workspace_idx: usize, column: Column<W>, activate: bool) {
        let workspace = &mut self.workspaces[workspace_idx];

        workspace.add_column(None, column, activate, None);

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

    pub fn clean_up_workspaces(&mut self) {
        assert!(self.workspace_switch.is_none());

        for idx in (0..self.workspaces.len() - 1).rev() {
            if self.active_workspace_idx == idx {
                continue;
            }

            if !self.workspaces[idx].has_windows() && self.workspaces[idx].name.is_none() {
                self.workspaces.remove(idx);
                if self.active_workspace_idx > idx {
                    self.active_workspace_idx -= 1;
                }
            }
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

    pub fn move_left(&mut self) {
        self.active_workspace().move_left();
    }

    pub fn move_right(&mut self) {
        self.active_workspace().move_right();
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
        let workspace = self.active_workspace();
        if workspace.columns.is_empty() {
            return;
        }
        let column = &mut workspace.columns[workspace.active_column_idx];
        let curr_idx = column.active_tile_idx;
        let new_idx = min(column.active_tile_idx + 1, column.tiles.len() - 1);
        if curr_idx == new_idx {
            self.move_to_workspace_down();
        } else {
            workspace.move_down();
        }
    }

    pub fn move_up_or_to_workspace_up(&mut self) {
        let workspace = self.active_workspace();
        if workspace.columns.is_empty() {
            return;
        }
        let curr_idx = workspace.columns[workspace.active_column_idx].active_tile_idx;
        let new_idx = curr_idx.saturating_sub(1);
        if curr_idx == new_idx {
            self.move_to_workspace_up();
        } else {
            workspace.move_up();
        }
    }

    pub fn focus_left(&mut self) {
        self.active_workspace().focus_left();
    }

    pub fn focus_right(&mut self) {
        self.active_workspace().focus_right();
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

    pub fn focus_down(&mut self) {
        self.active_workspace().focus_down();
    }

    pub fn focus_up(&mut self) {
        self.active_workspace().focus_up();
    }

    pub fn focus_down_or_left(&mut self) {
        let workspace = self.active_workspace();
        if !workspace.columns.is_empty() {
            let column = &workspace.columns[workspace.active_column_idx];
            let curr_idx = column.active_tile_idx;
            let new_idx = min(column.active_tile_idx + 1, column.tiles.len() - 1);
            if curr_idx == new_idx {
                self.focus_left();
            } else {
                workspace.focus_down();
            }
        }
    }

    pub fn focus_down_or_right(&mut self) {
        let workspace = self.active_workspace();
        if !workspace.columns.is_empty() {
            let column = &workspace.columns[workspace.active_column_idx];
            let curr_idx = column.active_tile_idx;
            let new_idx = min(column.active_tile_idx + 1, column.tiles.len() - 1);
            if curr_idx == new_idx {
                self.focus_right();
            } else {
                workspace.focus_down();
            }
        }
    }

    pub fn focus_up_or_left(&mut self) {
        let workspace = self.active_workspace();
        if !workspace.columns.is_empty() {
            let curr_idx = workspace.columns[workspace.active_column_idx].active_tile_idx;
            let new_idx = curr_idx.saturating_sub(1);
            if curr_idx == new_idx {
                self.focus_left();
            } else {
                workspace.focus_up();
            }
        }
    }

    pub fn focus_up_or_right(&mut self) {
        let workspace = self.active_workspace();
        if workspace.columns.is_empty() {
            self.switch_workspace_up();
        } else {
            let curr_idx = workspace.columns[workspace.active_column_idx].active_tile_idx;
            let new_idx = curr_idx.saturating_sub(1);
            if curr_idx == new_idx {
                self.focus_right();
            } else {
                workspace.focus_up();
            }
        }
    }

    pub fn focus_window_or_workspace_down(&mut self) {
        let workspace = self.active_workspace();
        if workspace.columns.is_empty() {
            self.switch_workspace_down();
        } else {
            let column = &workspace.columns[workspace.active_column_idx];
            let curr_idx = column.active_tile_idx;
            let new_idx = min(column.active_tile_idx + 1, column.tiles.len() - 1);
            if curr_idx == new_idx {
                self.switch_workspace_down();
            } else {
                workspace.focus_down();
            }
        }
    }

    pub fn focus_window_or_workspace_up(&mut self) {
        let workspace = self.active_workspace();
        if workspace.columns.is_empty() {
            self.switch_workspace_up();
        } else {
            let curr_idx = workspace.columns[workspace.active_column_idx].active_tile_idx;
            let new_idx = curr_idx.saturating_sub(1);
            if curr_idx == new_idx {
                self.switch_workspace_up();
            } else {
                workspace.focus_up();
            }
        }
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

        let column = &workspace.columns[workspace.active_column_idx];
        let removed = workspace.remove_tile_by_idx(
            workspace.active_column_idx,
            column.active_tile_idx,
            Transaction::new(),
            None,
        );

        self.add_window(
            new_idx,
            removed.tile.into_window(),
            true,
            removed.width,
            removed.is_full_width,
        );
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

        let column = &workspace.columns[workspace.active_column_idx];
        let removed = workspace.remove_tile_by_idx(
            workspace.active_column_idx,
            column.active_tile_idx,
            Transaction::new(),
            None,
        );

        self.add_window(
            new_idx,
            removed.tile.into_window(),
            true,
            removed.width,
            removed.is_full_width,
        );
    }

    pub fn move_to_workspace(&mut self, window: Option<&W::Id>, idx: usize) {
        let (source_workspace_idx, col_idx, tile_idx) = if let Some(window) = window {
            self.workspaces
                .iter()
                .enumerate()
                .find_map(|(ws_idx, ws)| {
                    ws.columns.iter().enumerate().find_map(|(col_idx, col)| {
                        col.tiles
                            .iter()
                            .position(|tile| tile.window().id() == window)
                            .map(|tile_idx| (ws_idx, col_idx, tile_idx))
                    })
                })
                .unwrap()
        } else {
            let ws_idx = self.active_workspace_idx;

            let ws = &self.workspaces[ws_idx];
            if ws.columns.is_empty() {
                return;
            }

            let col_idx = ws.active_column_idx;
            let tile_idx = ws.columns[col_idx].active_tile_idx;
            (ws_idx, col_idx, tile_idx)
        };

        let new_idx = min(idx, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        let column = &workspace.columns[col_idx];
        let activate = source_workspace_idx == self.active_workspace_idx
            && col_idx == workspace.active_column_idx
            && tile_idx == column.active_tile_idx;

        let removed = workspace.remove_tile_by_idx(col_idx, tile_idx, Transaction::new(), None);

        self.add_window(
            new_idx,
            removed.tile.into_window(),
            activate,
            removed.width,
            removed.is_full_width,
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
        if workspace.columns.is_empty() {
            return;
        }

        let column = workspace.remove_column_by_idx(workspace.active_column_idx, None);
        self.add_column(new_idx, column, true);
    }

    pub fn move_column_to_workspace_down(&mut self) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(source_workspace_idx + 1, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.columns.is_empty() {
            return;
        }

        let column = workspace.remove_column_by_idx(workspace.active_column_idx, None);
        self.add_column(new_idx, column, true);
    }

    pub fn move_column_to_workspace(&mut self, idx: usize) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(idx, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.columns.is_empty() {
            return;
        }

        let column = workspace.remove_column_by_idx(workspace.active_column_idx, None);
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

    pub fn focus(&self) -> Option<&W> {
        let workspace = &self.workspaces[self.active_workspace_idx];
        if !workspace.has_windows() {
            return None;
        }

        let column = &workspace.columns[workspace.active_column_idx];
        Some(column.tiles[column.active_tile_idx].window())
    }

    pub fn advance_animations(&mut self, current_time: Duration) {
        if let Some(WorkspaceSwitch::Animation(anim)) = &mut self.workspace_switch {
            anim.set_current_time(current_time);
            if anim.is_done() {
                self.workspace_switch = None;
                self.clean_up_workspaces();
            }
        }

        for ws in &mut self.workspaces {
            ws.advance_animations(current_time);
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
        for ws in &mut self.workspaces {
            ws.update_config(options.clone());
        }

        if self.options.struts != options.struts {
            let scale = self.output.current_scale();
            let transform = self.output.current_transform();
            let view_size = output_size(&self.output);
            let working_area = compute_working_area(&self.output, options.struts);

            for ws in &mut self.workspaces {
                ws.set_view_size(scale, transform, view_size, working_area);
            }
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

        let previous_workspace_id = self.previous_workspace_id;
        self.activate_workspace(new_idx);
        self.workspace_switch = None;
        self.previous_workspace_id = previous_workspace_id;

        self.clean_up_workspaces();
    }

    pub fn move_workspace_up(&mut self) {
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
    ) -> impl Iterator<Item = MonitorRenderElement<R>> + '_ {
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
                ws.render_elements(renderer, target)
                    .into_iter()
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
            gesture.current_idx,
            new_idx as f64,
            velocity,
            self.options.animations.workspace_switch.0,
        )));

        true
    }
}
