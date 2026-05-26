use std::{
    collections::{HashMap, HashSet},
    env,
};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::mpsc,
};

use crate::compositors::compositors::{
    CompositorCapabilities, CompositorEvent, CompositorRefresh, CompositorSnapshot, KeyboardLayout,
    Monitor, MonitorMode, ScreencastControlCapability, ScreencastKind, ScreencastSession,
    ScreencastStateCapability, ScreencastTarget, Window, Workspace, is_builtin_connector,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Niri;

impl Niri {
    pub async fn listen(self, sender: mpsc::Sender<CompositorEvent>) -> anyhow::Result<()> {
        let mut stream = connect().await?;
        write_request(&mut stream, &json!("EventStream")).await?;

        let reader = BufReader::new(stream);
        let mut lines = reader.lines();
        let reply = lines
            .next_line()
            .await?
            .context("niri event stream closed before initial reply")?;
        ensure_ok_reply(&reply)?;

        let mut state = NiriEventState::default();
        while let Some(line) = lines.next_line().await? {
            for event in parse_niri_event(&line, &mut state) {
                if sender.send(event).await.is_err() {
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    pub async fn snapshot(&self) -> anyhow::Result<CompositorSnapshot> {
        let mut monitors = request_ok(json!("Outputs"))
            .await?
            .get("Outputs")
            .map(parse_outputs)
            .unwrap_or_default();
        let workspaces: Vec<Workspace> = request_ok(json!("Workspaces"))
            .await?
            .get("Workspaces")
            .and_then(Value::as_array)
            .map(|workspaces| workspaces.iter().filter_map(parse_workspace).collect())
            .unwrap_or_default();
        let windows = request_ok(json!("Windows"))
            .await?
            .get("Windows")
            .and_then(Value::as_array)
            .map(|windows| windows.iter().filter_map(parse_window).collect::<Vec<_>>())
            .unwrap_or_default();
        let (keyboard_layouts, current_keyboard_layout) =
            parse_keyboard_layouts_response(&request_ok(json!("KeyboardLayouts")).await?);
        let focused_window = request_ok(json!("FocusedWindow"))
            .await?
            .get("FocusedWindow")
            .and_then(|window| {
                if window.is_null() {
                    None
                } else {
                    field_usize(window, "id")
                }
            });
        let focused_output = request_ok(json!("FocusedOutput"))
            .await
            .ok()
            .and_then(|reply| {
                reply
                    .get("FocusedOutput")
                    .and_then(|output| {
                        if output.is_null() {
                            None
                        } else {
                            output.get("name").and_then(Value::as_str)
                        }
                    })
                    .map(ToOwned::to_owned)
            });
        let current_workspace = workspaces
            .iter()
            .find(|workspace| workspace.focused)
            .map(|workspace| workspace.id);
        for monitor in &mut monitors {
            monitor.focused = focused_output.as_deref() == Some(monitor.name.as_str());
            monitor.active_workspace = workspaces
                .iter()
                .find(|workspace| {
                    workspace.active && workspace.monitor.as_deref() == Some(monitor.name.as_str())
                })
                .map(|workspace| workspace.id);
        }

        let screencast_result = request_ok(json!("Casts")).await;
        let screencasts = screencast_result
            .as_ref()
            .ok()
            .and_then(|value| value.get("Casts"))
            .and_then(Value::as_array)
            .map(|casts| casts.iter().filter_map(parse_cast).collect())
            .unwrap_or_default();
        let mut capabilities = self.capabilities();
        if screencast_result.is_err() {
            capabilities.screencast_state = ScreencastStateCapability::None;
            capabilities.screencast_control = ScreencastControlCapability::None;
        }

        Ok(CompositorSnapshot {
            capabilities,
            windows,
            workspaces,
            monitors,
            screencasts,
            keyboard_layouts,
            current_keyboard_layout,
            focused_window,
            current_workspace,
        })
    }

    pub fn capabilities(&self) -> CompositorCapabilities {
        CompositorCapabilities {
            windows: true,
            workspaces: true,
            monitors: true,
            keyboard_layouts: true,
            focused_window: true,
            current_workspace: true,
            fullscreen: true,
            floating: false,
            window_titles: true,
            night_light: true,
            screencast_state: ScreencastStateCapability::Sessions,
            screencast_control: ScreencastControlCapability::StopSession,
        }
    }

    pub async fn set_keyboard_layout(&self, layout: usize) -> anyhow::Result<()> {
        let layout = u8::try_from(layout).context("niri keyboard layout index is out of range")?;
        send_action(json!({
            "SwitchLayout": {
                "layout": {
                    "Index": layout
                }
            }
        }))
        .await
    }

    pub async fn set_workspace(&self, workspace: usize) -> anyhow::Result<()> {
        let workspace = u8::try_from(workspace).context("niri workspace index is out of range")?;
        send_action(json!({
            "FocusWorkspace": {
                "reference": {
                    "Index": workspace
                }
            }
        }))
        .await
    }

    pub async fn focus_next_workspace(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWorkspaceDown": {} })).await
    }

    pub async fn focus_previous_workspace(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWorkspaceUp": {} })).await
    }

    pub async fn focus_window(&self, window: usize) -> anyhow::Result<()> {
        send_action(json!({
            "FocusWindow": {
                "id": window as u64
            }
        }))
        .await
    }

    pub async fn focus_next_window(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWindowDownOrColumnRight": {} })).await
    }

    pub async fn focus_previous_window(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWindowUpOrColumnLeft": {} })).await
    }

    pub async fn stop_screencast(&self, session_id: &str) -> anyhow::Result<()> {
        let session_id = session_id
            .parse::<u64>()
            .context("niri screencast session id is not numeric")?;
        send_action(json!({
            "StopCast": {
                "session_id": session_id
            }
        }))
        .await
    }

    pub async fn set_monitor_enabled(&self, name: &str, on: bool) -> anyhow::Result<()> {
        let action = if on { "On" } else { "Off" };
        send_request(json!({
            "Output": {
                "output": name,
                "action": action
            }
        }))
        .await
    }
}

async fn send_request(request: Value) -> anyhow::Result<()> {
    let mut stream = connect().await?;
    write_request(&mut stream, &request).await?;

    let mut lines = BufReader::new(stream).lines();
    let reply = lines
        .next_line()
        .await?
        .context("niri action closed before reply")?;
    ensure_ok_reply(&reply)
}

async fn send_action(action: Value) -> anyhow::Result<()> {
    let mut stream = connect().await?;
    write_request(&mut stream, &json!({ "Action": action })).await?;

    let mut lines = BufReader::new(stream).lines();
    let reply = lines
        .next_line()
        .await?
        .context("niri action closed before reply")?;
    ensure_ok_reply(&reply)
}

async fn request_ok(request: Value) -> anyhow::Result<Value> {
    let mut stream = connect().await?;
    write_request(&mut stream, &request).await?;

    let mut lines = BufReader::new(stream).lines();
    let reply = lines
        .next_line()
        .await
        .context("failed to read niri reply")?
        .context("niri request closed before reply")?;
    let reply: Value = serde_json::from_str(&reply).context("invalid niri reply")?;
    if let Some(error) = reply.get("Err").and_then(Value::as_str) {
        bail!("niri IPC error: {error}");
    }

    reply
        .get("Ok")
        .cloned()
        .context("unexpected niri IPC reply without Ok")
}

async fn connect() -> anyhow::Result<UnixStream> {
    let socket = env::var("NIRI_SOCKET").context("NIRI_SOCKET is not set")?;
    UnixStream::connect(socket)
        .await
        .context("failed to connect to niri socket")
}

async fn write_request(stream: &mut UnixStream, request: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(request)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

fn ensure_ok_reply(line: &str) -> anyhow::Result<()> {
    let reply: Value = serde_json::from_str(line).context("invalid niri reply")?;
    if let Some(error) = reply.get("Err").and_then(Value::as_str) {
        bail!("niri IPC error: {error}");
    }

    if reply.get("Ok").is_none() {
        bail!("unexpected niri IPC reply: {line}");
    }

    Ok(())
}

#[derive(Default)]
struct NiriEventState {
    current_workspace: Option<usize>,
    focused_window: Option<usize>,
    layout_names: Vec<String>,
    window_workspaces: HashMap<usize, usize>,
    /// Most recently observed set of monitor names referenced by the
    /// workspace list. Niri's IPC has no dedicated output event, so we
    /// detect monitor connect/disconnect by watching this set shift
    /// across `WorkspacesChanged` events — workspaces get reassigned
    /// to surviving monitors when one disappears (and a new monitor's
    /// workspaces appear when one is plugged in), so this is a reliable
    /// proxy. When the set changes we ask the compositor service to
    /// re-snapshot, which refetches `Outputs` and drops the now-gone
    /// monitor from `state.monitors`.
    monitor_names: HashSet<String>,
}

fn parse_niri_event(line: &str, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let Ok(event) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };

    if let Some(workspaces) = event
        .get("WorkspacesChanged")
        .and_then(|event| event.get("workspaces"))
        .and_then(Value::as_array)
    {
        return parse_workspaces_changed(workspaces, state);
    }

    if let Some(event) = event.get("WorkspaceActivated") {
        return parse_workspace_activated(event, state);
    }

    if let Some(event) = event.get("WorkspaceActiveWindowChanged") {
        return parse_workspace_active_window_changed(event, state);
    }

    if let Some(windows) = event
        .get("WindowsChanged")
        .and_then(|event| event.get("windows"))
        .and_then(Value::as_array)
    {
        return parse_windows_changed(windows, state);
    }

    if let Some(window) = event
        .get("WindowOpenedOrChanged")
        .and_then(|event| event.get("window"))
    {
        return parse_window_changed(window, state);
    }

    if let Some(event) = event.get("WindowFocusChanged") {
        return parse_window_focus_changed(event, state);
    }

    if let Some(event) = event.get("WindowClosed") {
        if let Some(window) = field_usize(event, "id") {
            state.window_workspaces.remove(&window);
            if state.focused_window == Some(window) {
                state.focused_window = None;
            }
            return vec![CompositorEvent::WindowClosed(window)];
        }
    }

    if let Some(event) = event.get("KeyboardLayoutsChanged") {
        return parse_keyboard_layouts_changed(event, state);
    }

    if let Some(event) = event.get("KeyboardLayoutSwitched") {
        return parse_keyboard_layout_switched(event, state);
    }

    if let Some(casts) = event
        .get("CastsChanged")
        .and_then(|event| event.get("casts"))
        .and_then(Value::as_array)
    {
        return vec![CompositorEvent::ScreencastsChanged(
            casts.iter().filter_map(parse_cast).collect(),
        )];
    }

    if let Some(cast) = event
        .get("CastStartedOrChanged")
        .and_then(|event| event.get("cast"))
        .and_then(parse_cast)
    {
        return vec![CompositorEvent::ScreencastChanged(cast)];
    }

    if let Some(stream_id) = event
        .get("CastStopped")
        .and_then(|event| event.get("stream_id"))
        .and_then(Value::as_u64)
    {
        return vec![CompositorEvent::ScreencastStopped(stream_id.to_string())];
    }

    Vec::new()
}

fn parse_cast(value: &Value) -> Option<ScreencastSession> {
    let stream_id = field_usize(value, "stream_id")?.to_string();
    let session_id = field_usize(value, "session_id").map(|id| id.to_string());
    let kind = parse_cast_kind(value.get("kind"));
    let target = parse_cast_target(value.get("target"));
    let active = value
        .get("is_active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let pipewire_node = value
        .get("pw_node_id")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok());
    let client_pid = value
        .get("pid")
        .and_then(Value::as_i64)
        .and_then(|pid| i32::try_from(pid).ok());

    Some(ScreencastSession {
        id: stream_id,
        session_id,
        kind,
        target,
        active,
        pipewire_node,
        client_pid,
        stoppable: kind == ScreencastKind::PipeWire,
    })
}

fn parse_cast_kind(value: Option<&Value>) -> ScreencastKind {
    let Some(value) = value else {
        return ScreencastKind::Unknown;
    };
    let text = tagged_value_name(value).to_ascii_lowercase();

    if text.contains("pipewire") {
        ScreencastKind::PipeWire
    } else if text.contains("wlr") || text.contains("screencopy") {
        ScreencastKind::WlrScreencopy
    } else {
        ScreencastKind::Unknown
    }
}

fn parse_cast_target(value: Option<&Value>) -> ScreencastTarget {
    let Some(value) = value else {
        return ScreencastTarget::Unknown;
    };
    let text = tagged_value_name(value).to_ascii_lowercase();

    if text.contains("output") || text.contains("monitor") {
        ScreencastTarget::Monitor
    } else if text.contains("window") {
        ScreencastTarget::Window
    } else {
        ScreencastTarget::Unknown
    }
}

fn tagged_value_name(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        return value.to_owned();
    }

    value
        .as_object()
        .and_then(|object| object.keys().next())
        .cloned()
        .unwrap_or_default()
}

fn parse_workspaces_changed(
    workspaces: &[Value],
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let next = workspaces
        .iter()
        .filter_map(parse_workspace)
        .collect::<Vec<_>>();

    if let Some(workspace) = next.iter().find(|workspace| workspace.focused) {
        state.current_workspace = Some(workspace.id);
    }

    // Detect monitor topology shifts: if the set of monitor names a
    // workspace references has changed since the last event, output
    // configuration likely changed too. Ask for a structure re-snapshot
    // so `state.monitors` drops disconnected outputs (or picks up new
    // ones). The first event has an empty prior set, so we don't fire
    // a redundant refresh against the snapshot we just loaded at startup.
    let next_monitor_names: HashSet<String> = next
        .iter()
        .filter_map(|workspace| workspace.monitor.clone())
        .collect();
    let topology_shifted =
        !state.monitor_names.is_empty() && state.monitor_names != next_monitor_names;
    state.monitor_names = next_monitor_names;

    let mut events = Vec::new();
    if topology_shifted {
        events.push(CompositorEvent::RefreshRequested(
            CompositorRefresh::STRUCTURE,
        ));
    }
    events.push(CompositorEvent::WorkspacesChanged(next));
    events
}

fn parse_workspace_activated(event: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let focused = event
        .get("focused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(workspace) = field_usize(event, "id") else {
        return Vec::new();
    };

    if focused {
        state.current_workspace = Some(workspace);
    }
    vec![CompositorEvent::WorkspaceChanged {
        id: workspace,
        focused,
    }]
}

fn parse_workspace_active_window_changed(
    event: &Value,
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let Some(workspace) = field_usize(event, "workspace_id") else {
        return Vec::new();
    };

    if state.current_workspace == Some(workspace) {
        return vec![CompositorEvent::WorkspaceActiveWindowChanged {
            workspace,
            window: field_usize(event, "active_window_id"),
        }];
    }

    Vec::new()
}

fn parse_windows_changed(windows: &[Value], state: &mut NiriEventState) -> Vec<CompositorEvent> {
    state.window_workspaces.clear();
    let next = windows
        .iter()
        .filter_map(parse_window)
        .inspect(|window| {
            if let Some(workspace) = window.workspace {
                state.window_workspaces.insert(window.id, workspace);
            }
        })
        .collect::<Vec<_>>();

    if let Some(window) = next.iter().find(|window| window.focused) {
        state.focused_window = Some(window.id);
        if let Some(workspace) = window.workspace {
            state.current_workspace = Some(workspace);
        }
    }

    vec![CompositorEvent::WindowsChanged(next)]
}

fn parse_window_changed(window: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let Some(window) = parse_window(window) else {
        return Vec::new();
    };

    if let Some(workspace) = window.workspace {
        state.window_workspaces.insert(window.id, workspace);
    }

    if window.focused {
        state.focused_window = Some(window.id);
        if let Some(workspace) = window.workspace {
            state.current_workspace = Some(workspace);
        }
    }

    vec![CompositorEvent::WindowChanged(window)]
}

fn parse_window_focus_changed(event: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let window = field_usize(event, "id");
    state.focused_window = window;

    if let Some(workspace) = window.and_then(|window| state.window_workspaces.get(&window).copied())
    {
        state.current_workspace = Some(workspace);
    }

    vec![CompositorEvent::FocusedWindowChanged(window)]
}

fn parse_keyboard_layouts_changed(
    event: &Value,
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let Some(layouts) = event.get("keyboard_layouts") else {
        return Vec::new();
    };

    state.layout_names = layouts
        .get("names")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let Some(index) = field_usize(layouts, "current_idx") else {
        return Vec::new();
    };

    vec![CompositorEvent::KeyboardLayoutsChanged {
        layouts: keyboard_layouts(&state.layout_names),
        current: Some(index),
    }]
}

fn parse_keyboard_layout_switched(
    event: &Value,
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let Some(index) = field_usize(event, "idx") else {
        return Vec::new();
    };

    vec![CompositorEvent::KeyboardLayoutChanged {
        index: Some(index),
        name: state.layout_names.get(index).cloned(),
    }]
}

fn parse_outputs(value: &Value) -> Vec<Monitor> {
    let Some(outputs) = value.as_object() else {
        return Vec::new();
    };

    outputs
        .iter()
        .map(|(name, output)| {
            let name_str = output
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(name)
                .to_owned();
            let model = output
                .get("model")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let make = output
                .get("make")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let description = output
                .get("description")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let current_mode_index = output
                .get("current_mode")
                .and_then(|v| if v.is_null() { None } else { v.as_u64() });
            let enabled = current_mode_index.is_some();
            let current_mode = current_mode_index.and_then(|idx| {
                output
                    .get("modes")
                    .and_then(Value::as_array)
                    .and_then(|modes| modes.get(idx as usize))
                    .and_then(parse_mode_entry)
            });
            let built_in = is_builtin_connector(&name_str, None);
            Monitor {
                id: None,
                name: name_str,
                description,
                active_workspace: None,
                focused: false,
                make,
                model,
                enabled,
                built_in,
                current_mode,
            }
        })
        .collect()
}

fn parse_mode_entry(value: &Value) -> Option<MonitorMode> {
    let width = value.get("width").and_then(Value::as_u64)?;
    let height = value.get("height").and_then(Value::as_u64)?;
    let refresh = value.get("refresh_rate").and_then(Value::as_u64)?;
    Some(MonitorMode {
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        refresh_mhz: u32::try_from(refresh).ok()?,
    })
}

fn parse_workspace(value: &Value) -> Option<Workspace> {
    Some(Workspace {
        id: field_usize(value, "id")?,
        index: field_usize(value, "idx"),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        monitor: value
            .get("output")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        active: value
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        focused: value
            .get("is_focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        urgent: value
            .get("is_urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        active_window: field_usize(value, "active_window_id"),
    })
}

fn parse_window(value: &Value) -> Option<Window> {
    Some(Window {
        id: field_usize(value, "id")?,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        app_id: value
            .get("app_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        pid: value
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok()),
        layout_order: window_layout_order(value),
        workspace: field_usize(value, "workspace_id"),
        focused: value
            .get("is_focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        urgent: value
            .get("is_urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fullscreen: value
            .get("is_fullscreen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        floating: value.get("is_floating").and_then(Value::as_bool),
    })
}

fn window_layout_order(value: &Value) -> Option<usize> {
    value
        .get("layout")
        .and_then(|layout| layout.get("pos_in_scrolling_layout"))
        .and_then(Value::as_array)
        .and_then(|position| position.first())
        .and_then(Value::as_i64)
        .and_then(|position| usize::try_from(position).ok())
}

fn parse_keyboard_layouts_response(value: &Value) -> (Vec<KeyboardLayout>, Option<usize>) {
    let Some(layouts) = value.get("KeyboardLayouts") else {
        return (Vec::new(), None);
    };
    let names = layouts
        .get("names")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (
        keyboard_layouts(&names),
        field_usize(layouts, "current_idx"),
    )
}

fn keyboard_layouts(names: &[String]) -> Vec<KeyboardLayout> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| KeyboardLayout {
            index,
            name: name.clone(),
        })
        .collect()
}

fn field_usize(value: &Value, field: &str) -> Option<usize> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for the "disconnected monitor still appears in
    /// the display applet" bug on Niri. Niri's IPC has no dedicated
    /// output event, so we use a shift in the workspace→monitor set as
    /// the trigger to re-snapshot outputs. Sequence:
    ///   1. WorkspacesChanged with workspaces on DP-1 + eDP-1 → no
    ///      refresh (priming the prior set).
    ///   2. WorkspacesChanged with workspaces only on eDP-1 (DP-1 has
    ///      been physically disconnected) → must emit RefreshRequested
    ///      *before* the workspace event so the compositor service
    ///      re-fetches Outputs and drops DP-1 from `state.monitors`.
    #[test]
    fn workspaces_changed_emits_structure_refresh_when_monitor_set_shrinks() {
        let mut state = NiriEventState::default();

        // Priming event: two monitors visible. No refresh fires because
        // we don't have a prior set to compare against.
        let primed = parse_niri_event(
            r#"{"WorkspacesChanged":{"workspaces":[
                {"id":1,"output":"eDP-1","is_focused":true,"active_window_id":null},
                {"id":2,"output":"DP-1","is_focused":false,"active_window_id":null}
            ]}}"#,
            &mut state,
        );
        assert!(
            primed
                .iter()
                .all(|e| !matches!(e, CompositorEvent::RefreshRequested(_))),
            "first event must not fire a refresh"
        );

        // DP-1 unplugged: its workspace either disappears or moves to
        // eDP-1. Either way, the monitor set changes and the parser
        // emits a structure refresh.
        let after_unplug = parse_niri_event(
            r#"{"WorkspacesChanged":{"workspaces":[
                {"id":1,"output":"eDP-1","is_focused":true,"active_window_id":null}
            ]}}"#,
            &mut state,
        );
        assert!(
            matches!(
                after_unplug.first(),
                Some(CompositorEvent::RefreshRequested(_))
            ),
            "expected RefreshRequested first, got {after_unplug:?}"
        );
        assert!(
            matches!(
                after_unplug.last(),
                Some(CompositorEvent::WorkspacesChanged(_))
            ),
            "workspaces update must still be emitted"
        );
    }

    /// Counterpart: when workspaces shuffle but the monitor set is
    /// unchanged (e.g. user switches focus between workspaces on the
    /// same two monitors), no structure refresh fires. Without this
    /// check we'd refetch outputs on every workspace switch — wasteful
    /// and noisy.
    #[test]
    fn workspaces_changed_no_refresh_when_monitor_set_unchanged() {
        let mut state = NiriEventState::default();

        let _ = parse_niri_event(
            r#"{"WorkspacesChanged":{"workspaces":[
                {"id":1,"output":"eDP-1","is_focused":true,"active_window_id":null},
                {"id":2,"output":"DP-1","is_focused":false,"active_window_id":null}
            ]}}"#,
            &mut state,
        );
        let same_monitors = parse_niri_event(
            r#"{"WorkspacesChanged":{"workspaces":[
                {"id":1,"output":"eDP-1","is_focused":false,"active_window_id":null},
                {"id":2,"output":"DP-1","is_focused":true,"active_window_id":null}
            ]}}"#,
            &mut state,
        );
        assert!(
            same_monitors
                .iter()
                .all(|e| !matches!(e, CompositorEvent::RefreshRequested(_))),
            "no topology change → no refresh: {same_monitors:?}"
        );
    }

    #[test]
    fn parses_workspace_and_focused_window_events() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"WorkspacesChanged":{"workspaces":[{"id":4,"is_focused":true,"active_window_id":9}]}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![CompositorEvent::WorkspacesChanged(vec![Workspace {
                id: 4,
                index: None,
                name: None,
                monitor: None,
                active: false,
                focused: true,
                urgent: false,
                active_window: Some(9),
            }])]
        );
    }

    #[test]
    fn workspace_activated_emits_change_event_even_when_not_globally_focused() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"WorkspaceActivated":{"id":7,"focused":false}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![CompositorEvent::WorkspaceChanged {
                id: 7,
                focused: false,
            }]
        );
        assert_eq!(
            state.current_workspace, None,
            "current_workspace must not move when activation is on a non-focused output",
        );

        let events = parse_niri_event(
            r#"{"WorkspaceActivated":{"id":9,"focused":true}}"#,
            &mut state,
        );
        assert_eq!(
            events,
            vec![CompositorEvent::WorkspaceChanged {
                id: 9,
                focused: true,
            }]
        );
        assert_eq!(state.current_workspace, Some(9));
    }

    #[test]
    fn tracks_window_workspace_for_focus_events() {
        let mut state = NiriEventState::default();

        parse_niri_event(
            r#"{"WindowOpenedOrChanged":{"window":{"id":12,"workspace_id":6,"is_focused":false}}}"#,
            &mut state,
        );
        let events = parse_niri_event(r#"{"WindowFocusChanged":{"id":12}}"#, &mut state);

        assert_eq!(
            events,
            vec![CompositorEvent::FocusedWindowChanged(Some(12))]
        );
    }

    #[test]
    fn parses_window_layout_order_from_scrolling_layout() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"WindowOpenedOrChanged":{"window":{"id":12,"workspace_id":6,"layout":{"pos_in_scrolling_layout":[42,0]}}}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![CompositorEvent::WindowChanged(Window {
                id: 12,
                title: None,
                app_id: None,
                pid: None,
                layout_order: Some(42),
                workspace: Some(6),
                focused: false,
                urgent: false,
                fullscreen: false,
                floating: None,
            })]
        );
    }

    #[test]
    fn parses_screencast_events() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"CastStartedOrChanged":{"cast":{"stream_id":8,"session_id":5,"kind":"PipeWire","target":{"Output":"eDP-1"},"is_active":true,"pid":1234,"pw_node_id":42}}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![CompositorEvent::ScreencastChanged(ScreencastSession {
                id: "8".into(),
                session_id: Some("5".into()),
                kind: ScreencastKind::PipeWire,
                target: ScreencastTarget::Monitor,
                active: true,
                pipewire_node: Some(42),
                client_pid: Some(1234),
                stoppable: true,
            })]
        );

        assert_eq!(
            parse_niri_event(r#"{"CastStopped":{"stream_id":8}}"#, &mut state),
            vec![CompositorEvent::ScreencastStopped("8".into())]
        );
    }

    #[test]
    fn parse_outputs_with_null_current_mode_marks_disabled() {
        let value = json!({
            "DP-1": {
                "name": "DP-1",
                "make": "Dell Inc.",
                "model": "Dell U2723QE",
                "current_mode": null
            }
        });

        let monitors = parse_outputs(&value);
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "DP-1");
        assert!(!monitors[0].enabled);
        assert_eq!(monitors[0].current_mode, None);
    }

    #[test]
    fn parse_outputs_extracts_make_model_and_current_mode() {
        let value = json!({
            "DP-1": {
                "name": "DP-1",
                "make": "Dell Inc.",
                "model": "Dell U2723QE",
                "modes": [
                    { "width": 3840, "height": 2160, "refresh_rate": 60000, "is_preferred": true },
                    { "width": 2560, "height": 1440, "refresh_rate": 144000, "is_preferred": false }
                ],
                "current_mode": 0
            }
        });

        let monitors = parse_outputs(&value);
        assert_eq!(monitors.len(), 1);
        let monitor = &monitors[0];
        assert_eq!(monitor.make.as_deref(), Some("Dell Inc."));
        assert_eq!(monitor.model.as_deref(), Some("Dell U2723QE"));
        assert_eq!(monitor.description, None);
        assert!(monitor.enabled);
        assert_eq!(
            monitor.current_mode,
            Some(MonitorMode {
                width: 3840,
                height: 2160,
                refresh_mhz: 60000,
            })
        );
    }

    #[test]
    fn parse_outputs_uses_description_when_available() {
        let value = json!({
            "DP-1": {
                "name": "DP-1",
                "make": "Dell Inc.",
                "model": "Dell U2723QE",
                "description": "Dell Inc. Dell U2723QE 0x1234",
                "current_mode": null
            }
        });

        let monitors = parse_outputs(&value);
        assert_eq!(
            monitors[0].description.as_deref(),
            Some("Dell Inc. Dell U2723QE 0x1234")
        );
    }

    #[test]
    fn parse_outputs_marks_edp_as_builtin() {
        let value = json!({
            "eDP-1": { "name": "eDP-1" },
            "LVDS-1": { "name": "LVDS-1" },
            "DSI-1": { "name": "DSI-1" },
            "DP-1": { "name": "DP-1" },
            "HDMI-A-1": { "name": "HDMI-A-1" }
        });

        let monitors = parse_outputs(&value);
        let by_name = |n: &str| {
            monitors
                .iter()
                .find(|m| m.name == n)
                .unwrap_or_else(|| panic!("missing {n}"))
        };
        assert!(by_name("eDP-1").built_in);
        assert!(by_name("LVDS-1").built_in);
        assert!(by_name("DSI-1").built_in);
        assert!(!by_name("DP-1").built_in);
        assert!(!by_name("HDMI-A-1").built_in);
    }
}
