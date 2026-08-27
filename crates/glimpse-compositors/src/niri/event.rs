use std::collections::HashSet;

use serde::Deserialize;

use crate::event::{Event, Resync};
use crate::keyboard::layout_code;
use crate::model::{KeyboardLayouts, Window, WindowId, Workspace, WorkspaceId};

/// The little niri does not repeat in every event: which layouts exist, so a switch carrying only
/// an index can be named, and which outputs the workspace list mentioned, because niri has no
/// output event and a monitor appearing or disappearing shows up here first.
#[derive(Default)]
pub struct EventState {
    layout_names: Vec<String>,
    output_names: HashSet<String>,
}

impl EventState {
    pub fn decode(&mut self, line: &str) -> Vec<Event> {
        match serde_json::from_str::<Wire>(line) {
            Ok(event) => self.translate(event),
            Err(error) => {
                tracing::debug!(%error, "ignored an unrecognized niri event");
                Vec::new()
            }
        }
    }

    fn translate(&mut self, event: Wire) -> Vec<Event> {
        match event {
            Wire::WorkspacesChanged { workspaces } => self.workspaces_changed(workspaces),
            Wire::WorkspaceActivated { id, focused } => vec![Event::WorkspaceActivated {
                id: WorkspaceId(id),
                focused,
            }],
            Wire::WorkspaceActiveWindowChanged {
                workspace_id,
                active_window_id,
            } => vec![Event::WorkspaceActiveWindowChanged {
                workspace: WorkspaceId(workspace_id),
                window: active_window_id.map(WindowId),
            }],
            Wire::WorkspaceUrgencyChanged { id, urgent } => {
                vec![Event::WorkspaceUrgencyChanged {
                    id: WorkspaceId(id),
                    urgent,
                }]
            }
            Wire::WindowsChanged { windows } => vec![Event::WindowsChanged(windows)],
            Wire::WindowOpenedOrChanged { window } => {
                vec![Event::WindowOpenedOrChanged(window)]
            }
            Wire::WindowClosed { id } => vec![Event::WindowClosed(WindowId(id))],
            Wire::WindowFocusChanged { id } => {
                vec![Event::WindowFocusChanged(id.map(WindowId))]
            }
            Wire::WindowUrgencyChanged { id, urgent } => vec![Event::WindowUrgencyChanged {
                id: WindowId(id),
                urgent,
            }],
            Wire::WindowLayoutsChanged { changes } => vec![Event::WindowLayoutsChanged(
                changes
                    .into_iter()
                    .map(|(id, layout)| (WindowId(id), layout.column()))
                    .collect(),
            )],
            Wire::KeyboardLayoutsChanged { keyboard_layouts } => {
                self.layout_names = keyboard_layouts.names.clone();
                vec![Event::KeyboardLayoutsChanged(keyboard_layouts.into())]
            }
            Wire::KeyboardLayoutSwitched { idx } => vec![Event::KeyboardLayoutSwitched {
                idx: usize::from(idx),
                name: self.layout_names.get(usize::from(idx)).cloned(),
            }],
            // Niri reloads its own configuration without restarting, and the layout list is the one
            // thing in this snapshot that a reload can change under us.
            Wire::ConfigLoaded { failed: false } => vec![Event::Resync(Resync::Keyboard)],
            Wire::ConfigLoaded { failed: true } => Vec::new(),
        }
    }

    /// Niri emits no output event. Workspaces are reassigned to surviving monitors on unplug and
    /// appear on plug, so the set of names this list mentions is the only notice we get.
    fn workspaces_changed(&mut self, workspaces: Vec<Workspace>) -> Vec<Event> {
        let seen: HashSet<String> = workspaces
            .iter()
            .filter_map(|workspace| workspace.output.clone())
            .collect();
        let moved = seen != self.output_names;
        self.output_names = seen;

        match moved {
            true => vec![
                Event::Resync(Resync::Outputs),
                Event::WorkspacesChanged(workspaces),
            ],
            false => vec![Event::WorkspacesChanged(workspaces)],
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct WireLayouts {
    pub names: Vec<String>,
    pub current_idx: u8,
}

impl From<WireLayouts> for KeyboardLayouts {
    fn from(layouts: WireLayouts) -> Self {
        Self {
            codes: layouts.names.iter().map(|name| layout_code(name)).collect(),
            current: Some(usize::from(layouts.current_idx)),
            names: layouts.names,
        }
    }
}

#[derive(Deserialize)]
struct WireLayout {
    #[serde(default)]
    pos_in_scrolling_layout: Option<(u16, u16)>,
}

impl WireLayout {
    fn column(&self) -> Option<u16> {
        self.pos_in_scrolling_layout.map(|(column, _row)| column)
    }
}

#[derive(Deserialize)]
enum Wire {
    WorkspacesChanged {
        workspaces: Vec<Workspace>,
    },
    WorkspaceActivated {
        id: u64,
        focused: bool,
    },
    WorkspaceActiveWindowChanged {
        workspace_id: u64,
        active_window_id: Option<u64>,
    },
    WorkspaceUrgencyChanged {
        id: u64,
        urgent: bool,
    },
    WindowsChanged {
        windows: Vec<Window>,
    },
    WindowOpenedOrChanged {
        window: Window,
    },
    WindowClosed {
        id: u64,
    },
    WindowFocusChanged {
        id: Option<u64>,
    },
    WindowUrgencyChanged {
        id: u64,
        urgent: bool,
    },
    WindowLayoutsChanged {
        changes: Vec<(u64, WireLayout)>,
    },
    KeyboardLayoutsChanged {
        keyboard_layouts: WireLayouts,
    },
    KeyboardLayoutSwitched {
        idx: u8,
    },
    ConfigLoaded {
        failed: bool,
    },
}
