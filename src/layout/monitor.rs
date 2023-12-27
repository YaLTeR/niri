use std::cmp::min;
use std::rc::Rc;
use std::time::Duration;

use smithay::backend::renderer::element::utils::{
    CropRenderElement, Relocate, RelocateRenderElement,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::Window;
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Scale};

use super::workspace::{
    compute_working_area, ColumnWidth, OutputId, Workspace, WorkspaceRenderElement,
};
use super::{LayoutElement, Options};
use crate::animation::Animation;
use crate::config::SizeChange;
use crate::utils::output_size;

#[derive(Debug)]
pub struct Monitor<W: LayoutElement> {
    /// Output for this monitor.
    pub output: Output,
    // Must always contain at least one.
    pub workspaces: Vec<Workspace<W>>,
    /// Index of the currently active workspace.
    pub active_workspace_idx: usize,
    /// In-progress switch between workspaces.
    pub workspace_switch: Option<WorkspaceSwitch>,
    /// Configurable properties of the layout.
    pub options: Rc<Options>,
}

#[derive(Debug)]
pub enum WorkspaceSwitch {
    Animation(Animation),
    Gesture(WorkspaceSwitchGesture),
}

#[derive(Debug)]
pub struct WorkspaceSwitchGesture {
    /// Index of the workspace where the gesture was started.
    pub center_idx: usize,
    /// Current, fractional workspace index.
    pub current_idx: f64,
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
            output,
            workspaces,
            active_workspace_idx: 0,
            workspace_switch: None,
            options,
        }
    }

    pub fn active_workspace(&mut self) -> &mut Workspace<W> {
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

        workspace.add_window(window, activate, width, is_full_width);

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

    pub fn move_down_or_to_workspace_down(&mut self) {
        let workspace = self.active_workspace();
        if workspace.columns.is_empty() {
            return;
        }
        let column = &mut workspace.columns[workspace.active_column_idx];
        let curr_idx = column.active_window_idx;
        let new_idx = min(column.active_window_idx + 1, column.windows.len() - 1);
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
        let curr_idx = workspace.columns[workspace.active_column_idx].active_window_idx;
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

    pub fn focus_down(&mut self) {
        self.active_workspace().focus_down();
    }

    pub fn focus_up(&mut self) {
        self.active_workspace().focus_up();
    }

    pub fn focus_window_or_workspace_down(&mut self) {
        let workspace = self.active_workspace();
        if workspace.columns.is_empty() {
            self.switch_workspace_down();
        } else {
            let column = &workspace.columns[workspace.active_column_idx];
            let curr_idx = column.active_window_idx;
            let new_idx = min(column.active_window_idx + 1, column.windows.len() - 1);
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
            let curr_idx = workspace.columns[workspace.active_column_idx].active_window_idx;
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
        let width = column.width;
        let is_full_width = column.is_full_width;
        let window =
            workspace.remove_window_by_idx(workspace.active_column_idx, column.active_window_idx);

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

        let column = &workspace.columns[workspace.active_column_idx];
        let width = column.width;
        let is_full_width = column.is_full_width;
        let window =
            workspace.remove_window_by_idx(workspace.active_column_idx, column.active_window_idx);

        self.add_window(new_idx, window, true, width, is_full_width);
    }

    pub fn move_to_workspace(&mut self, idx: usize) {
        let source_workspace_idx = self.active_workspace_idx;

        let new_idx = min(idx, self.workspaces.len() - 1);
        if new_idx == source_workspace_idx {
            return;
        }

        let workspace = &mut self.workspaces[source_workspace_idx];
        if workspace.columns.is_empty() {
            return;
        }

        let column = &workspace.columns[workspace.active_column_idx];
        let width = column.width;
        let is_full_width = column.is_full_width;
        let window =
            workspace.remove_window_by_idx(workspace.active_column_idx, column.active_window_idx);

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

    pub fn switch_workspace(&mut self, idx: usize) {
        self.activate_workspace(min(idx, self.workspaces.len() - 1));
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
        Some(column.windows[column.active_window_idx].window())
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

    pub fn update_config(&mut self, options: Rc<Options>) {
        for ws in &mut self.workspaces {
            ws.update_config(options.clone());
        }

        if self.options.struts != options.struts {
            let view_size = output_size(&self.output);
            let working_area = compute_working_area(&self.output, options.struts);

            for ws in &mut self.workspaces {
                ws.set_view_size(view_size, working_area);
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

    pub fn set_window_height(&mut self, change: SizeChange) {
        self.active_workspace().set_window_height(change);
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

        self.activate_workspace(new_idx);
        self.workspace_switch = None;

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
                                // HACK: set infinite crop bounds due to a damage tracking bug
                                // which causes glitched rendering for maximized GTK windows.
                                // FIXME: use proper bounds after fixing the Crop element.
                                Rectangle::from_loc_and_size(
                                    (-i32::MAX / 2, -i32::MAX / 2),
                                    (i32::MAX, i32::MAX),
                                ),
                                // Rectangle::from_loc_and_size((0, 0), size),
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
