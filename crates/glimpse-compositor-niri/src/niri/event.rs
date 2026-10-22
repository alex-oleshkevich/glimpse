use std::collections::HashSet;
use std::future::Future;

use glimpse_compositors::event::{Event, LineDecoder, Resync};
use glimpse_compositors::model::{
    Cast, KeyboardLayouts, Rect, Window, WindowId, Workspace, WorkspaceId,
};
use glimpse_compositors::sanitize::{capped_app_id_str, capped_name_str, capped_title_str};
use serde::{Deserialize, Deserializer};

#[derive(Default)]
pub struct EventState {
    output_names: HashSet<String>,
}

impl EventState {
    fn translate(&mut self, event: Wire) -> Vec<Event> {
        match event {
            Wire::WorkspacesChanged { workspaces } => self.workspaces_changed(workspaces),
            Wire::WorkspaceActivated { id, focused } => vec![Event::WorkspaceActivated {
                id: WorkspaceId(id),
                focused,
            }],
            Wire::WorkspaceActiveWindowChanged {} | Wire::WorkspaceUrgencyChanged {} => Vec::new(),
            Wire::WindowsChanged { windows } => {
                let focused = windows
                    .iter()
                    .find(|window| window.is_focused)
                    .map(|window| WindowId(window.id));
                vec![Event::WindowsChanged {
                    windows: windows.into_iter().map(WireWindow::into_model).collect(),
                    focused,
                }]
            }
            Wire::WindowOpenedOrChanged { window } => vec![Event::WindowOpenedOrChanged {
                focused: window.is_focused,
                window: window.into_model(),
            }],
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
                vec![Event::KeyboardLayoutSwitched(u32::from(idx))]
            }
            Wire::CastsChanged { casts } => vec![Event::CastsChanged(into_casts(casts))],
            Wire::CastStartedOrChanged { cast } => {
                vec![Event::CastStartedOrChanged(cast.into_model())]
            }
            Wire::CastStopped { stream_id } => vec![Event::CastStopped(stream_id)],
            Wire::ConfigLoaded { failed: false } => vec![
                Event::Resync(Resync::Keyboard),
                Event::Resync(Resync::Outputs),
            ],
            Wire::ConfigLoaded { failed: true } => Vec::new(),
        }
    }

    fn workspaces_changed(&mut self, workspaces: Vec<WireWorkspace>) -> Vec<Event> {
        let seen: HashSet<String> = workspaces
            .iter()
            .filter_map(|workspace| workspace.output.clone())
            .collect();
        let moved = seen != self.output_names;
        self.output_names = seen;

        let (workspaces, focused) = into_workspaces(workspaces);
        let changed = Event::WorkspacesChanged {
            workspaces,
            focused,
        };
        match moved {
            true => vec![Event::Resync(Resync::Outputs), changed],
            false => vec![changed],
        }
    }
}

impl LineDecoder for EventState {
    type Event = Event;

    fn decode(&mut self, line: &str) -> impl Future<Output = Vec<Event>> + Send {
        std::future::ready(match serde_json::from_str::<Wire>(line) {
            Ok(event) => self.translate(event),
            Err(error) => {
                tracing::debug!(%error, "ignored an unrecognized niri event");
                Vec::new()
            }
        })
    }
}

pub fn into_casts(casts: Vec<WireCast>) -> Vec<Cast> {
    casts.into_iter().map(WireCast::into_model).collect()
}

pub fn into_workspaces(workspaces: Vec<WireWorkspace>) -> (Vec<Workspace>, Option<WorkspaceId>) {
    let focused = workspaces
        .iter()
        .find(|workspace| workspace.is_focused)
        .map(|workspace| WorkspaceId(workspace.id));
    (
        workspaces
            .into_iter()
            .map(WireWorkspace::into_model)
            .collect(),
        focused,
    )
}

#[derive(Deserialize)]
pub struct WireWorkspace {
    id: u64,
    #[serde(default)]
    idx: Option<u8>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    is_focused: bool,
}

impl WireWorkspace {
    fn into_model(self) -> Workspace {
        Workspace {
            id: WorkspaceId(self.id),
            index: self.idx,
            name: self.name.as_deref().and_then(capped_name_str),
            output: self.output.as_deref().and_then(capped_name_str),
            active: self.is_active,
        }
    }
}

#[derive(Deserialize)]
pub struct WireWindow {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    pid: Option<i32>,
    #[serde(default)]
    workspace_id: Option<u64>,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    is_floating: bool,
    #[serde(default)]
    is_urgent: bool,
    #[serde(default)]
    layout: Option<WireLayout>,
}

impl WireWindow {
    pub fn id(&self) -> WindowId {
        WindowId(self.id)
    }

    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    pub fn into_model(self) -> Window {
        Window {
            id: WindowId(self.id),
            title: self.title.as_deref().and_then(capped_title_str),
            app_id: self.app_id.as_deref().and_then(capped_app_id_str),
            pid: self.pid,
            workspace: self.workspace_id.map(WorkspaceId),
            floating: self.is_floating,
            urgent: self.is_urgent,
            order: self.layout.as_ref().and_then(WireLayout::column),
        }
    }

    pub fn rect(&self) -> Option<Rect> {
        let layout = self.layout.as_ref()?;
        let (x, y) = layout.tile_pos_in_workspace_view?;
        let (offset_x, offset_y) = layout.window_offset_in_tile.unwrap_or((0.0, 0.0));
        let (width, height) = layout.window_size?;
        (width >= 0 && height >= 0).then(|| Rect {
            x: x + offset_x,
            y: y + offset_y,
            width: f64::from(width),
            height: f64::from(height),
        })
    }
}

#[derive(Deserialize)]
pub struct WireLayout {
    #[serde(default)]
    pos_in_scrolling_layout: Option<(u16, u16)>,
    #[serde(default)]
    window_size: Option<(i32, i32)>,
    #[serde(default)]
    tile_pos_in_workspace_view: Option<(f64, f64)>,
    #[serde(default)]
    window_offset_in_tile: Option<(f64, f64)>,
}

impl WireLayout {
    fn column(&self) -> Option<u16> {
        self.pos_in_scrolling_layout.map(|(column, _row)| column)
    }
}

#[derive(Deserialize)]
pub struct WireLayouts {
    names: Vec<String>,
    current_idx: u8,
}

impl From<WireLayouts> for KeyboardLayouts {
    fn from(layouts: WireLayouts) -> Self {
        Self {
            codes: Vec::new(),
            current: Some(u32::from(layouts.current_idx)),
            names: layouts.names,
        }
    }
}

#[derive(Deserialize)]
pub struct WireCast {
    stream_id: u64,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    session_id: Option<u64>,
    #[serde(default, rename = "kind", deserialize_with = "is_pipewire")]
    pipewire: bool,
    #[serde(default, rename = "target", deserialize_with = "target_output")]
    output: Option<String>,
    #[serde(default)]
    pw_node_id: Option<u32>,
}

impl WireCast {
    pub fn into_model(self) -> Cast {
        Cast {
            id: self.stream_id,
            active: self.is_active,
            output: self.output.as_deref().and_then(capped_name_str),
            session: self.session_id.filter(|_| self.pipewire),
            pw_node: self.pw_node_id,
        }
    }
}

fn is_pipewire<'de, D: Deserializer<'de>>(input: D) -> Result<bool, D::Error> {
    Ok(serde_json::Value::deserialize(input)?.as_str() == Some("PipeWire"))
}

fn target_output<'de, D: Deserializer<'de>>(input: D) -> Result<Option<String>, D::Error> {
    Ok(serde_json::Value::deserialize(input)?
        .pointer("/Output/name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

#[derive(Deserialize)]
pub enum Wire {
    WorkspacesChanged { workspaces: Vec<WireWorkspace> },
    WorkspaceActivated { id: u64, focused: bool },
    WorkspaceActiveWindowChanged {},
    WorkspaceUrgencyChanged {},
    WindowsChanged { windows: Vec<WireWindow> },
    WindowOpenedOrChanged { window: WireWindow },
    WindowClosed { id: u64 },
    WindowFocusChanged { id: Option<u64> },
    WindowUrgencyChanged { id: u64, urgent: bool },
    WindowLayoutsChanged { changes: Vec<(u64, WireLayout)> },
    KeyboardLayoutsChanged { keyboard_layouts: WireLayouts },
    KeyboardLayoutSwitched { idx: u8 },
    CastsChanged { casts: Vec<WireCast> },
    CastStartedOrChanged { cast: WireCast },
    CastStopped { stream_id: u64 },
    ConfigLoaded { failed: bool },
}
