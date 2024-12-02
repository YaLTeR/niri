//! Helpers for keeping track of the event stream state.
//!
//! 1. Create an [`EventStreamState`] using `Default::default()`, or any individual state part if
//!    you only care about part of the state.
//! 2. Connect to the niri socket and request an event stream.
//! 3. Pass every [`Event`] to [`EventStreamStatePart::apply`] on your state.
//! 4. Read the fields of the state as needed.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::{Event, KeyboardLayouts, Window, Workspace};

/// Part of the state communicated via the event stream.
pub trait EventStreamStatePart {
    /// Returns a sequence of events that replicates this state from default initialization.
    fn replicate(&self) -> Vec<Event>;

    /// Applies the event to this state.
    ///
    /// Returns `None` after applying the event, and `Some(event)` if the event is ignored by this
    /// part of the state.
    fn apply(&mut self, event: Event) -> Option<Event>;
}

/// The full state communicated over the event stream.
///
/// Different parts of the state are not guaranteed to be consistent across every single event
/// sent by niri. For example, you may receive the first [`Event::WindowOpenedOrChanged`] for a
/// just-opened window *after* an [`Event::WorkspaceActiveWindowChanged`] for that window. Between
/// these two events, the workspace active window id refers to a window that does not yet exist in
/// the windows state part.
#[derive(Debug, Default)]
pub struct EventStreamState {
    /// State of workspaces.
    pub workspaces: WorkspacesState,

    /// State of workspaces.
    pub windows: WindowsState,

    /// State of the keyboard layouts.
    pub keyboard_layouts: KeyboardLayoutsState,
}

/// The workspaces state communicated over the event stream.
#[derive(Debug, Default)]
pub struct WorkspacesState {
    /// Map from a workspace id to the workspace.
    pub workspaces: HashMap<u64, Workspace>,
}

/// The windows state communicated over the event stream.
#[derive(Debug, Default)]
pub struct WindowsState {
    /// Map from a window id to the window.
    pub windows: HashMap<u64, Window>,
}

/// The keyboard layout state communicated over the event stream.
#[derive(Debug, Default)]
pub struct KeyboardLayoutsState {
    /// Configured keyboard layouts.
    pub keyboard_layouts: Option<KeyboardLayouts>,
}

impl EventStreamStatePart for EventStreamState {
    fn replicate(&self) -> Vec<Event> {
        let mut events = Vec::new();
        events.extend(self.workspaces.replicate());
        events.extend(self.windows.replicate());
        events.extend(self.keyboard_layouts.replicate());
        events
    }

    fn apply(&mut self, event: Event) -> Option<Event> {
        let event = self.workspaces.apply(event)?;
        let event = self.windows.apply(event)?;
        let event = self.keyboard_layouts.apply(event)?;
        Some(event)
    }
}

impl EventStreamStatePart for WorkspacesState {
    fn replicate(&self) -> Vec<Event> {
        let workspaces = self.workspaces.values().cloned().collect();
        vec![Event::WorkspacesChanged { workspaces }]
    }

    fn apply(&mut self, event: Event) -> Option<Event> {
        match event {
            Event::WorkspacesChanged { workspaces } => {
                self.workspaces = workspaces.into_iter().map(|ws| (ws.id, ws)).collect();
            }
            Event::WorkspaceActivated { id, focused } => {
                let ws = self.workspaces.get(&id);
                let ws = ws.expect("activated workspace was missing from the map");
                let output = ws.output.clone();

                for ws in self.workspaces.values_mut() {
                    let got_activated = ws.id == id;
                    if ws.output == output {
                        ws.is_active = got_activated;
                    }

                    if focused {
                        ws.is_focused = got_activated;
                    }
                }
            }
            Event::WorkspaceActiveWindowChanged {
                workspace_id,
                active_window_id,
            } => {
                let ws = self.workspaces.get_mut(&workspace_id);
                let ws = ws.expect("changed workspace was missing from the map");
                ws.active_window_id = active_window_id;
            }
            event => return Some(event),
        }
        None
    }
}

impl EventStreamStatePart for WindowsState {
    fn replicate(&self) -> Vec<Event> {
        let windows = self.windows.values().cloned().collect();
        vec![Event::WindowsChanged { windows }]
    }

    fn apply(&mut self, event: Event) -> Option<Event> {
        match event {
            Event::WindowsChanged { windows } => {
                self.windows = windows.into_iter().map(|win| (win.id, win)).collect();
            }
            Event::WindowOpenedOrChanged { window } => {
                let (id, is_focused) = match self.windows.entry(window.id) {
                    Entry::Occupied(mut entry) => {
                        let entry = entry.get_mut();
                        *entry = window;
                        (entry.id, entry.is_focused)
                    }
                    Entry::Vacant(entry) => {
                        let entry = entry.insert(window);
                        (entry.id, entry.is_focused)
                    }
                };

                if is_focused {
                    for win in self.windows.values_mut() {
                        if win.id != id {
                            win.is_focused = false;
                        }
                    }
                }
            }
            Event::WindowClosed { id } => {
                let win = self.windows.remove(&id);
                win.expect("closed window was missing from the map");
            }
            Event::WindowFocusChanged { id } => {
                for win in self.windows.values_mut() {
                    win.is_focused = Some(win.id) == id;
                }
            }
            event => return Some(event),
        }
        None
    }
}

impl EventStreamStatePart for KeyboardLayoutsState {
    fn replicate(&self) -> Vec<Event> {
        if let Some(keyboard_layouts) = self.keyboard_layouts.clone() {
            vec![Event::KeyboardLayoutsChanged { keyboard_layouts }]
        } else {
            vec![]
        }
    }

    fn apply(&mut self, event: Event) -> Option<Event> {
        match event {
            Event::KeyboardLayoutsChanged { keyboard_layouts } => {
                self.keyboard_layouts = Some(keyboard_layouts);
            }
            Event::KeyboardLayoutSwitched { idx } => {
                let kb = self.keyboard_layouts.as_mut();
                let kb = kb.expect("keyboard layouts must be set before a layout can be switched");
                kb.current_idx = idx;
            }
            event => return Some(event),
        }
        None
    }
}
