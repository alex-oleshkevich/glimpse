use std::collections::HashSet;
use std::future::Future;

use serde::{Deserialize, Deserializer};

use crate::event::{Event, LineDecoder, Resync};
use crate::keyboard::layout_code;
use crate::model::{
    Cast, CastKind, CastTarget, KeyboardLayouts, Window, WindowId, Workspace, WorkspaceId,
};

/// The little niri does not repeat in every event: which outputs the workspace list mentioned,
/// because niri has no output event and a monitor appearing or disappearing shows up here first.
#[derive(Default)]
pub struct EventState {
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
                vec![Event::KeyboardLayoutsChanged(keyboard_layouts.into())]
            }
            Wire::KeyboardLayoutSwitched { idx } => {
                vec![Event::KeyboardLayoutSwitched(usize::from(idx))]
            }
            Wire::CastsChanged { casts } => vec![Event::CastsChanged(into_casts(casts))],
            Wire::CastStartedOrChanged { cast } => vec![Event::CastStartedOrChanged {
                cast: cast.into_model(),
            }],
            Wire::CastStopped { stream_id } => vec![Event::CastStopped(stream_id)],
            // Niri reloads its own configuration without restarting, which can change the layout
            // list and an output's scale or mode, and niri sends no output event for either.
            Wire::ConfigLoaded { failed: false } => vec![
                Event::Resync(Resync::Keyboard),
                Event::Resync(Resync::Outputs),
            ],
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

impl LineDecoder for EventState {
    fn decode(&mut self, line: &str) -> impl Future<Output = Vec<Event>> + Send {
        std::future::ready(self.decode(line))
    }
}

pub(crate) fn into_casts(casts: Vec<WireCast>) -> Vec<Cast> {
    casts.into_iter().map(WireCast::into_model).collect()
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

#[derive(Deserialize)]
pub(crate) struct WireCast {
    stream_id: u64,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    session_id: Option<u64>,
    #[serde(default, deserialize_with = "cast_kind")]
    kind: CastKind,
    #[serde(default, deserialize_with = "cast_target")]
    target: CastTarget,
    #[serde(default)]
    pw_node_id: Option<u32>,
}

impl WireCast {
    pub(crate) fn into_model(self) -> Cast {
        Cast {
            stream_id: self.stream_id,
            session_id: self.session_id,
            kind: self.kind,
            target: self.target,
            pw_node_id: self.pw_node_id,
            active: self.is_active,
        }
    }
}

fn cast_kind<'de, D: Deserializer<'de>>(input: D) -> Result<CastKind, D::Error> {
    Ok(match serde_json::Value::deserialize(input)?.as_str() {
        Some("PipeWire") => CastKind::PipeWire,
        Some("WlrScreencopy") => CastKind::Screencopy,
        _ => CastKind::Unknown,
    })
}

fn cast_target<'de, D: Deserializer<'de>>(input: D) -> Result<CastTarget, D::Error> {
    let value = serde_json::Value::deserialize(input)?;
    let Some(object) = value.as_object() else {
        return Ok(CastTarget::Unknown);
    };

    if let Some(name) = object
        .get("Output")
        .and_then(|output| output.get("name"))
        .and_then(serde_json::Value::as_str)
    {
        return Ok(CastTarget::Output(name.to_owned()));
    }
    if let Some(id) = object
        .get("Window")
        .and_then(|window| window.get("id"))
        .and_then(serde_json::Value::as_u64)
    {
        return Ok(CastTarget::Window(WindowId(id)));
    }
    Ok(CastTarget::Unknown)
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
    CastsChanged {
        casts: Vec<WireCast>,
    },
    CastStartedOrChanged {
        cast: WireCast,
    },
    CastStopped {
        stream_id: u64,
    },
    ConfigLoaded {
        failed: bool,
    },
}
