use crate::model::{KeyboardLayouts, Window, WindowId, Workspace, WorkspaceId};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    WorkspacesChanged(Vec<Workspace>),
    WorkspaceActivated {
        id: WorkspaceId,
        focused: bool,
    },
    WorkspaceActiveWindowChanged {
        workspace: WorkspaceId,
        window: Option<WindowId>,
    },
    WorkspaceUrgencyChanged {
        id: WorkspaceId,
        urgent: bool,
    },
    WindowsChanged(Vec<Window>),
    WindowOpenedOrChanged(Window),
    WindowClosed(WindowId),
    WindowFocusChanged(Option<WindowId>),
    WindowUrgencyChanged {
        id: WindowId,
        urgent: bool,
    },
    WindowLayoutsChanged(Vec<(WindowId, Option<u16>)>),
    KeyboardLayoutsChanged(KeyboardLayouts),
    KeyboardLayoutSwitched {
        idx: usize,
        name: Option<String>,
    },
    /// The compositor said something changed without saying what. The caller re-fetches the named
    /// part of the snapshot. Hyprland produces most of these; niri produces a handful.
    Resync(Resync),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resync {
    Structure,
    Keyboard,
    Outputs,
}
