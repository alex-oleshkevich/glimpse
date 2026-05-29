use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, bail, ensure};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::mpsc,
};

use crate::compositors::{
    ScreencastControlCapability, ScreencastKind, ScreencastSession, ScreencastStateCapability,
    ScreencastTarget,
    compositors::{
        CompositorCapabilities, CompositorEvent, CompositorRefresh, CompositorSnapshot,
        CompositorStructureSnapshot, KeyboardLayout, KeyboardLayoutSnapshot, Monitor, MonitorMode,
        Window, Workspace, is_builtin_connector,
    },
    keyboard_layout_code,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hyprland;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct HyprlandMonitorConfig {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub refresh_rate: Option<f64>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub scale: Option<f64>,
    pub transform: Option<u32>,
}

impl HyprlandMonitorConfig {
    pub(crate) fn keyword_args(&self, name: &str) -> String {
        let (Some(width), Some(height)) = (self.width, self.height) else {
            return format!("{name},preferred,auto,1");
        };
        let rate_part = match self.refresh_rate {
            Some(rate) => format!("{width}x{height}@{:.3}", rate),
            None => format!("{width}x{height}"),
        };
        let pos = format!("{}x{}", self.x.unwrap_or(0), self.y.unwrap_or(0));
        let scale = self.scale.unwrap_or(1.0);
        let mut out = format!("{name},{rate_part},{pos},{scale}");
        if let Some(transform) = self.transform
            && transform != 0
        {
            out.push_str(&format!(",transform,{transform}"));
        }
        out
    }
}

fn disabled_monitor_cache() -> &'static Mutex<HashMap<String, HyprlandMonitorConfig>> {
    static CACHE: OnceLock<Mutex<HashMap<String, HyprlandMonitorConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl Hyprland {
    pub async fn listen(self, sender: mpsc::Sender<CompositorEvent>) -> anyhow::Result<()> {
        let stream = UnixStream::connect(event_socket_path()?)
            .await
            .context("failed to connect to hyprland event socket")?;
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();
        let mut state = HyprlandEventState::default();

        while let Some(line) = lines.next_line().await? {
            for event in parse_hyprland_event(&line, &mut state) {
                if sender.send(event).await.is_err() {
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    pub async fn snapshot(&self) -> anyhow::Result<CompositorSnapshot> {
        let structure = self.structure_snapshot().await?;
        let keyboard = self.keyboard_layout_snapshot().await?;

        Ok(CompositorSnapshot {
            capabilities: self.capabilities(),
            windows: structure.windows,
            workspaces: structure.workspaces,
            monitors: structure.monitors,
            screencasts: Vec::new(),
            keyboard_layouts: keyboard.keyboard_layouts,
            current_keyboard_layout: keyboard.current_keyboard_layout,
            focused_window: structure.focused_window,
            current_workspace: structure.current_workspace,
        })
    }

    pub async fn structure_snapshot(&self) -> anyhow::Result<CompositorStructureSnapshot> {
        let monitors = parse_monitors(&json_command("j/monitors").await?);
        let mut workspaces = parse_workspaces(&json_command("j/workspaces").await?);
        let mut windows = parse_windows(&json_command("j/clients").await?);
        let active_window = json_command("j/activewindow")
            .await
            .ok()
            .and_then(|value| parse_window_id(value.get("address")?));
        let current_workspace = monitors
            .iter()
            .find(|monitor| monitor.focused)
            .and_then(|monitor| monitor.active_workspace)
            .or_else(|| {
                workspaces
                    .iter()
                    .find(|workspace| workspace.focused)
                    .map(|workspace| workspace.id)
            });
        for window in &mut windows {
            window.focused = Some(window.id) == active_window;
        }
        for workspace in &mut workspaces {
            workspace.active = monitors
                .iter()
                .any(|monitor| monitor.active_workspace == Some(workspace.id));
            workspace.focused = current_workspace == Some(workspace.id);
        }

        Ok(CompositorStructureSnapshot {
            windows,
            workspaces,
            monitors,
            focused_window: active_window,
            current_workspace,
        })
    }

    pub async fn keyboard_layout_snapshot(&self) -> anyhow::Result<KeyboardLayoutSnapshot> {
        let (keyboard_layouts, current_keyboard_layout) = read_keyboard_layouts().await;

        Ok(KeyboardLayoutSnapshot {
            keyboard_layouts,
            current_keyboard_layout,
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
            floating: true,
            window_titles: true,
            night_light: true,
            screencast_state: ScreencastStateCapability::ActiveKind,
            screencast_control: ScreencastControlCapability::None,
        }
    }

    pub async fn set_keyboard_layout(&self, layout: usize) -> anyhow::Result<()> {
        send_command(format!("switchxkblayout all {layout}")).await
    }

    pub async fn set_workspace(&self, workspace: usize) -> anyhow::Result<()> {
        ensure!(
            workspace > 0 && workspace <= i32::MAX as usize,
            "hyprland workspace id must be between 1 and {}",
            i32::MAX
        );
        send_command(format!("dispatch workspace {workspace}")).await
    }

    pub async fn rename_workspace(
        &self,
        workspace: usize,
        name: Option<&str>,
    ) -> anyhow::Result<()> {
        ensure!(
            workspace > 0 && workspace <= i32::MAX as usize,
            "hyprland workspace id must be between 1 and {}",
            i32::MAX
        );
        send_command(rename_workspace_command(workspace, name)).await
    }

    pub async fn focus_next_workspace(&self) -> anyhow::Result<()> {
        send_command("dispatch workspace +1").await
    }

    pub async fn focus_previous_workspace(&self) -> anyhow::Result<()> {
        send_command("dispatch workspace -1").await
    }

    pub async fn focus_window(&self, window: usize) -> anyhow::Result<()> {
        send_command(format!("dispatch focuswindow address:0x{window:x}")).await
    }

    pub async fn focus_next_window(&self) -> anyhow::Result<()> {
        send_command("dispatch cyclenext").await
    }

    pub async fn focus_previous_window(&self) -> anyhow::Result<()> {
        send_command("dispatch cyclenext prev").await
    }

    pub async fn set_monitor_enabled(&self, name: &str, on: bool) -> anyhow::Result<()> {
        if on {
            let cached = disabled_monitor_cache()
                .lock()
                .ok()
                .and_then(|mut cache| cache.remove(name));
            let args = match cached {
                Some(config) => config.keyword_args(name),
                None => format!("{name},preferred,auto,1"),
            };
            send_command(format!("keyword monitor {args}")).await
        } else {
            if let Ok(Some(config)) = hyprctl_monitor_config(name).await
                && let Ok(mut cache) = disabled_monitor_cache().lock()
            {
                cache.insert(name.to_owned(), config);
            }
            send_command(format!("keyword monitor {name},disable")).await
        }
    }
}

pub(crate) async fn hyprctl_monitor_config(
    name: &str,
) -> anyhow::Result<Option<HyprlandMonitorConfig>> {
    let value = json_command("j/monitors").await?;
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .find(|monitor| monitor.get("name").and_then(Value::as_str) == Some(name))
        .map(parse_monitor_config))
}

fn parse_monitor_config(value: &Value) -> HyprlandMonitorConfig {
    HyprlandMonitorConfig {
        width: value
            .get("width")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok()),
        height: value
            .get("height")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok()),
        refresh_rate: value.get("refreshRate").and_then(Value::as_f64),
        x: value
            .get("x")
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok()),
        y: value
            .get("y")
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok()),
        scale: value.get("scale").and_then(Value::as_f64),
        transform: value
            .get("transform")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok()),
    }
}

fn event_socket_path() -> anyhow::Result<PathBuf> {
    socket_path(".socket2.sock")
}

fn control_socket_path() -> anyhow::Result<PathBuf> {
    socket_path(".socket.sock")
}

fn socket_path(socket_name: &str) -> anyhow::Result<PathBuf> {
    let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .context("HYPRLAND_INSTANCE_SIGNATURE is not set")?;
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());

    Ok(PathBuf::from(runtime_dir)
        .join("hypr")
        .join(signature)
        .join(socket_name))
}

async fn send_command(command: impl AsRef<str>) -> anyhow::Result<()> {
    let reply = control_command(command).await?;
    let reply = reply.trim();

    if reply == "ok" || reply.is_empty() {
        Ok(())
    } else {
        bail!("hyprland IPC command failed: {reply}");
    }
}

fn rename_workspace_command(workspace: usize, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("dispatch renameworkspace {workspace} {name}"),
        None => format!("dispatch renameworkspace {workspace}"),
    }
}

async fn json_command(command: impl AsRef<str>) -> anyhow::Result<Value> {
    let reply = control_command(command).await?;
    serde_json::from_str(reply.trim()).context("invalid hyprland JSON reply")
}

async fn control_command(command: impl AsRef<str>) -> anyhow::Result<String> {
    let mut stream = UnixStream::connect(control_socket_path()?)
        .await
        .context("failed to connect to hyprland control socket")?;
    stream.write_all(command.as_ref().as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?;

    let mut reply = String::new();
    stream.read_to_string(&mut reply).await?;
    Ok(reply)
}

#[derive(Default)]
struct HyprlandEventState {
    current_workspace: Option<usize>,
}

fn parse_hyprland_event(line: &str, state: &mut HyprlandEventState) -> Vec<CompositorEvent> {
    if let Some(payload) = line.strip_prefix("workspacev2>>") {
        if let Some(workspace) = payload
            .split(',')
            .next()
            .and_then(|workspace| workspace.parse::<usize>().ok())
        {
            state.current_workspace = Some(workspace);
            return vec![
                CompositorEvent::WorkspaceChanged {
                    id: workspace,
                    focused: true,
                },
                CompositorEvent::RefreshRequested(CompositorRefresh::STRUCTURE),
            ];
        }
    }

    if let Some(payload) = line.strip_prefix("workspace>>") {
        if let Ok(workspace) = payload.parse::<usize>() {
            state.current_workspace = Some(workspace);
            return vec![
                CompositorEvent::WorkspaceChanged {
                    id: workspace,
                    focused: true,
                },
                CompositorEvent::RefreshRequested(CompositorRefresh::STRUCTURE),
            ];
        }
    }

    if let Some(payload) = line.strip_prefix("focusedmonv2>>") {
        let mut parts = payload.split(',');
        let monitor = parts.next().filter(|monitor| !monitor.is_empty());
        let workspace = parts.next().and_then(parse_usize);
        if let Some(workspace) = workspace {
            state.current_workspace = Some(workspace);
            let mut events = Vec::new();
            if let Some(monitor) = monitor {
                events.push(CompositorEvent::MonitorChanged {
                    name: monitor.to_owned(),
                    active_workspace: Some(workspace),
                    focused: true,
                });
            }
            events.push(CompositorEvent::WorkspaceChanged {
                id: workspace,
                focused: true,
            });
            return events;
        }
    }

    if let Some(payload) = line.strip_prefix("activewindowv2>>") {
        return vec![CompositorEvent::FocusedWindowChanged(
            parse_hyprland_window_address(payload),
        )];
    }

    if let Some(payload) = line.strip_prefix("fullscreen>>") {
        if let Some(fullscreen) = parse_bool_int(payload) {
            return vec![CompositorEvent::WindowFullscreenChanged {
                window: None,
                fullscreen,
            }];
        }
    }

    if let Some(payload) = line.strip_prefix("changefloatingmode>>") {
        let mut parts = payload.split(',');
        if let (Some(window), Some(floating)) = (
            parts.next().and_then(parse_hyprland_window_address),
            parts.next().and_then(parse_bool_int),
        ) {
            return vec![CompositorEvent::WindowFloatingChanged { window, floating }];
        }
    }

    if let Some(payload) = line.strip_prefix("windowtitlev2>>") {
        let mut parts = payload.splitn(2, ',');
        if let (Some(window), Some(title)) = (
            parts.next().and_then(parse_hyprland_window_address),
            parts.next().filter(|title| !title.is_empty()),
        ) {
            return vec![CompositorEvent::WindowTitleChanged {
                window,
                title: title.to_owned(),
            }];
        }
    }

    if let Some(payload) = line.strip_prefix("activelayout>>") {
        if let Some(layout) = payload
            .split(',')
            .nth(1)
            .filter(|layout| !layout.is_empty())
        {
            return vec![CompositorEvent::KeyboardLayoutChanged {
                index: None,
                name: Some(layout.to_owned()),
            }];
        }
    }

    if let Some(payload) = line.strip_prefix("screencast>>") {
        let mut parts = payload.split(',');
        if let (Some(active), Some(target)) = (
            parts.next().and_then(parse_bool_int),
            parts.next().and_then(parse_screencast_target),
        ) {
            return vec![CompositorEvent::ScreencastChanged(ScreencastSession {
                id: hyprland_screencast_id(target).into(),
                session_id: None,
                kind: ScreencastKind::Unknown,
                target,
                active,
                pipewire_node: None,
                client_pid: None,
                stoppable: false,
            })];
        }
    }

    if is_structural_event(line) {
        return vec![CompositorEvent::RefreshRequested(
            CompositorRefresh::STRUCTURE,
        )];
    }

    Vec::new()
}

fn parse_screencast_target(value: &str) -> Option<ScreencastTarget> {
    match value.trim() {
        "0" => Some(ScreencastTarget::Monitor),
        "1" => Some(ScreencastTarget::Window),
        _ => None,
    }
}

fn hyprland_screencast_id(target: ScreencastTarget) -> &'static str {
    match target {
        ScreencastTarget::Monitor => "hyprland:monitor",
        ScreencastTarget::Window => "hyprland:window",
        ScreencastTarget::Unknown => "hyprland:unknown",
    }
}

fn is_structural_event(line: &str) -> bool {
    [
        "monitorremoved>>",
        "monitorremovedv2>>",
        "monitoradded>>",
        "monitoraddedv2>>",
        "createworkspace>>",
        "createworkspacev2>>",
        "destroyworkspace>>",
        "destroyworkspacev2>>",
        "moveworkspace>>",
        "moveworkspacev2>>",
        "renameworkspace>>",
        "openwindow>>",
        "closewindow>>",
        "movewindow>>",
        "movewindowv2>>",
        "windowtitle>>",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn parse_hyprland_window_address(value: &str) -> Option<usize> {
    let value = value.trim();
    let value = value.strip_prefix("0x").unwrap_or(value);
    usize::from_str_radix(value, 16).ok()
}

fn parse_usize(value: &str) -> Option<usize> {
    value.parse::<usize>().ok()
}

fn parse_bool_int(value: &str) -> Option<bool> {
    match value.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

fn parse_monitors(value: &Value) -> Vec<Monitor> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|monitor| {
            let name = field_string(monitor, "name")?;
            let disabled = monitor
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let enabled = !disabled;
            let current_mode = if enabled {
                parse_current_mode(monitor)
            } else {
                None
            };
            Some(Monitor {
                id: field_usize(monitor, "id"),
                built_in: is_builtin_connector(&name, None),
                name,
                description: field_string(monitor, "description"),
                active_workspace: monitor
                    .get("activeWorkspace")
                    .and_then(|workspace| field_usize(workspace, "id")),
                focused: monitor
                    .get("focused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                make: field_string(monitor, "make"),
                model: field_string(monitor, "model"),
                enabled,
                current_mode,
            })
        })
        .collect()
}

fn parse_current_mode(value: &Value) -> Option<MonitorMode> {
    let width = value.get("width").and_then(Value::as_u64)?;
    let height = value.get("height").and_then(Value::as_u64)?;
    let refresh_hz = value.get("refreshRate").and_then(Value::as_f64)?;
    let refresh_mhz = (refresh_hz * 1000.0).round();
    if !refresh_mhz.is_finite() || refresh_mhz < 0.0 {
        return None;
    }
    Some(MonitorMode {
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        refresh_mhz: refresh_mhz as u32,
    })
}

fn parse_workspaces(value: &Value) -> Vec<Workspace> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|workspace| {
            Some(Workspace {
                id: field_usize(workspace, "id")?,
                index: field_usize(workspace, "id"),
                name: field_string(workspace, "name"),
                monitor: field_string(workspace, "monitor"),
                active: false,
                focused: false,
                urgent: false,
                active_window: field_usize(workspace, "lastwindow"),
            })
        })
        .collect()
}

fn parse_windows(value: &Value) -> Vec<Window> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_window)
        .collect()
}

fn parse_window(value: &Value) -> Option<Window> {
    Some(Window {
        id: parse_window_id(value.get("address")?)?,
        title: field_string(value, "title"),
        app_id: field_string(value, "class"),
        pid: value
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok()),
        layout_order: None,
        workspace: value
            .get("workspace")
            .and_then(|workspace| field_usize(workspace, "id")),
        focused: false,
        urgent: value
            .get("urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fullscreen: value
            .get("fullscreen")
            .and_then(Value::as_i64)
            .map(|fullscreen| fullscreen != 0)
            .unwrap_or(false),
        floating: value.get("floating").and_then(Value::as_bool),
    })
}

async fn read_keyboard_layouts() -> (Vec<KeyboardLayout>, Option<usize>) {
    let names = json_command("j/getoption input:kb_layout")
        .await
        .ok()
        .and_then(|value| field_string(&value, "str"))
        .map(|layouts| {
            layouts
                .split(',')
                .map(str::trim)
                .filter(|layout| !layout.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let current = active_keymap()
        .await
        .as_deref()
        .and_then(|active| active_layout_index(&names, active));

    (
        names
            .into_iter()
            .enumerate()
            .map(|(index, name)| KeyboardLayout { index, name })
            .collect(),
        current,
    )
}

fn active_layout_index(layouts: &[String], active_keymap: &str) -> Option<usize> {
    layouts
        .iter()
        .position(|layout| layout_matches_active_keymap(layout, active_keymap))
}

fn layout_matches_active_keymap(layout: &str, active_keymap: &str) -> bool {
    let layout = layout.trim().to_lowercase();
    let active_keymap = active_keymap.trim();
    let active_keymap_lower = active_keymap.to_lowercase();

    layout == active_keymap_lower
        || parenthesized_code(active_keymap)
            .map(|code| layout == code.to_lowercase())
            .unwrap_or(false)
        || layout == keyboard_layout_code(active_keymap).to_lowercase()
}

fn parenthesized_code(value: &str) -> Option<&str> {
    let start = value.find('(')?;
    let rest = &value[start + 1..];
    let end = rest.find(')')?;
    let code = rest[..end].trim();

    (!code.is_empty()).then_some(code)
}

async fn active_keymap() -> Option<String> {
    let devices = json_command("j/devices").await.ok()?;
    devices
        .get("keyboards")?
        .as_array()?
        .iter()
        .find(|keyboard| {
            keyboard
                .get("main")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .or_else(|| devices.get("keyboards")?.as_array()?.first())?
        .get("active_keymap")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn parse_window_id(value: &Value) -> Option<usize> {
    value.as_str().and_then(parse_hyprland_window_address)
}

fn field_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn field_usize(value: &Value, field: &str) -> Option<usize> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .and_then(|value| usize::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_workspace_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("workspacev2>>3,code", &mut state),
            vec![
                CompositorEvent::WorkspaceChanged {
                    id: 3,
                    focused: true,
                },
                CompositorEvent::RefreshRequested(CompositorRefresh::STRUCTURE),
            ]
        );
        assert_eq!(
            parse_hyprland_event("focusedmonv2>>DP-1,4", &mut state),
            vec![
                CompositorEvent::MonitorChanged {
                    name: "DP-1".into(),
                    active_workspace: Some(4),
                    focused: true,
                },
                CompositorEvent::WorkspaceChanged {
                    id: 4,
                    focused: true,
                },
            ]
        );
    }

    #[test]
    fn rename_workspace_command_sets_or_clears_workspace_name() {
        assert_eq!(
            rename_workspace_command(3, Some("chat")),
            "dispatch renameworkspace 3 chat"
        );
        assert_eq!(
            rename_workspace_command(3, None),
            "dispatch renameworkspace 3"
        );
    }

    #[test]
    fn parses_focused_window_after_workspace_is_known() {
        let mut state = HyprlandEventState::default();
        parse_hyprland_event("workspacev2>>3,code", &mut state);

        assert_eq!(
            parse_hyprland_event("activewindowv2>>3f2", &mut state),
            vec![CompositorEvent::FocusedWindowChanged(Some(0x3f2))]
        );
    }

    #[test]
    fn parses_keyboard_layout_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("activelayout>>keyboard,English (US)", &mut state),
            vec![CompositorEvent::KeyboardLayoutChanged {
                index: None,
                name: Some("English (US)".into())
            }]
        );
    }

    #[test]
    fn active_layout_index_matches_hyprland_display_names_to_configured_codes() {
        let layouts = vec!["us".into(), "ru".into(), "pl".into()];

        assert_eq!(active_layout_index(&layouts, "English (US)"), Some(0));
        assert_eq!(active_layout_index(&layouts, "Russian"), Some(1));
        assert_eq!(active_layout_index(&layouts, "Polish"), Some(2));
        assert_eq!(active_layout_index(&layouts, "us"), Some(0));
        assert_eq!(active_layout_index(&layouts, "Unknown"), None);
    }

    #[test]
    fn structural_window_events_request_refresh() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("openwindow>>3f2,1,kitty,Terminal", &mut state),
            vec![CompositorEvent::RefreshRequested(
                CompositorRefresh::STRUCTURE
            )]
        );
    }

    #[test]
    fn parses_window_update_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("windowtitlev2>>3f2,Terminal", &mut state),
            vec![CompositorEvent::WindowTitleChanged {
                window: 0x3f2,
                title: "Terminal".into(),
            }]
        );
        assert_eq!(
            parse_hyprland_event("fullscreen>>1", &mut state),
            vec![CompositorEvent::WindowFullscreenChanged {
                window: None,
                fullscreen: true,
            }]
        );
        assert_eq!(
            parse_hyprland_event("changefloatingmode>>3f2,1", &mut state),
            vec![CompositorEvent::WindowFloatingChanged {
                window: 0x3f2,
                floating: true,
            }]
        );
    }

    #[test]
    fn parses_screencast_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("screencast>>1,0", &mut state),
            vec![CompositorEvent::ScreencastChanged(ScreencastSession {
                id: "hyprland:monitor".into(),
                session_id: None,
                kind: ScreencastKind::Unknown,
                target: ScreencastTarget::Monitor,
                active: true,
                pipewire_node: None,
                client_pid: None,
                stoppable: false,
            })]
        );
        assert_eq!(
            parse_hyprland_event("screencast>>0,1", &mut state),
            vec![CompositorEvent::ScreencastChanged(ScreencastSession {
                id: "hyprland:window".into(),
                session_id: None,
                kind: ScreencastKind::Unknown,
                target: ScreencastTarget::Window,
                active: false,
                pipewire_node: None,
                client_pid: None,
                stoppable: false,
            })]
        );
    }

    #[test]
    fn parse_monitors_with_disabled_field_sets_enabled_false_and_no_mode() {
        let value = json!([
            {
                "id": 0,
                "name": "DP-1",
                "make": "Dell Inc.",
                "model": "Dell U2723QE",
                "width": 3840,
                "height": 2160,
                "refreshRate": 59.997,
                "disabled": true
            }
        ]);

        let monitors = parse_monitors(&value);
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "DP-1");
        assert!(!monitors[0].enabled);
        assert_eq!(monitors[0].current_mode, None);
    }

    #[test]
    fn parse_monitors_extracts_make_model_and_current_mode() {
        let value = json!([
            {
                "id": 0,
                "name": "DP-1",
                "make": "Dell Inc.",
                "model": "Dell U2723QE",
                "width": 3840,
                "height": 2160,
                "refreshRate": 59.997
            }
        ]);

        let monitors = parse_monitors(&value);
        assert_eq!(monitors.len(), 1);
        let monitor = &monitors[0];
        assert_eq!(monitor.make.as_deref(), Some("Dell Inc."));
        assert_eq!(monitor.model.as_deref(), Some("Dell U2723QE"));
        assert!(monitor.enabled);
        assert_eq!(
            monitor.current_mode,
            Some(MonitorMode {
                width: 3840,
                height: 2160,
                refresh_mhz: 59997,
            })
        );
    }

    #[test]
    fn parse_monitors_marks_edp_as_builtin() {
        let value = json!([
            { "name": "eDP-1" },
            { "name": "LVDS-1" },
            { "name": "DSI-1" },
            { "name": "DP-1" },
            { "name": "HDMI-A-1" }
        ]);

        let monitors = parse_monitors(&value);
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

    #[test]
    fn hyprland_monitor_config_struct_round_trips_to_keyword_string() {
        let config = HyprlandMonitorConfig {
            width: Some(3840),
            height: Some(2160),
            refresh_rate: Some(59.997),
            x: Some(1920),
            y: Some(0),
            scale: Some(1.5),
            transform: Some(1),
        };
        assert_eq!(
            config.keyword_args("DP-1"),
            "DP-1,3840x2160@59.997,1920x0,1.5,transform,1"
        );

        let no_transform = HyprlandMonitorConfig {
            transform: Some(0),
            ..config.clone()
        };
        assert_eq!(
            no_transform.keyword_args("DP-1"),
            "DP-1,3840x2160@59.997,1920x0,1.5"
        );

        let no_transform_field = HyprlandMonitorConfig {
            transform: None,
            ..config
        };
        assert_eq!(
            no_transform_field.keyword_args("DP-1"),
            "DP-1,3840x2160@59.997,1920x0,1.5"
        );
    }

    #[test]
    fn hyprland_monitor_config_falls_back_to_preferred_when_unknown() {
        let empty = HyprlandMonitorConfig::default();
        assert_eq!(empty.keyword_args("DP-1"), "DP-1,preferred,auto,1");

        let no_dimensions = HyprlandMonitorConfig {
            refresh_rate: Some(60.0),
            ..Default::default()
        };
        assert_eq!(
            no_dimensions.keyword_args("HDMI-A-1"),
            "HDMI-A-1,preferred,auto,1"
        );
    }

    #[test]
    fn hyprland_monitor_config_without_refresh_rate_omits_at_clause() {
        let config = HyprlandMonitorConfig {
            width: Some(1920),
            height: Some(1080),
            refresh_rate: None,
            x: Some(0),
            y: Some(0),
            scale: Some(1.0),
            transform: None,
        };
        assert_eq!(config.keyword_args("DP-2"), "DP-2,1920x1080,0x0,1");
    }

    #[test]
    fn parse_monitor_config_extracts_fields_from_hyprctl_json() {
        let value = json!({
            "name": "DP-1",
            "width": 2560,
            "height": 1440,
            "refreshRate": 144.0,
            "x": 0,
            "y": 0,
            "scale": 1.25,
            "transform": 0
        });
        let config = parse_monitor_config(&value);
        assert_eq!(config.width, Some(2560));
        assert_eq!(config.height, Some(1440));
        assert_eq!(config.refresh_rate, Some(144.0));
        assert_eq!(config.x, Some(0));
        assert_eq!(config.y, Some(0));
        assert_eq!(config.scale, Some(1.25));
        assert_eq!(config.transform, Some(0));
    }
}
