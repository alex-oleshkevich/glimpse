mod event;
#[cfg(test)]
mod testing;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures_util::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::CompositorError;
use crate::event::Event;
use crate::model::{
    KeyboardLayouts, LayoutTarget, Logical, Mode, Output, Snapshot, Window, WindowTarget,
    Workspace, WorkspaceId, WorkspaceTarget, capped_app_id_str, capped_title_str, is_built_in,
};
use event::{EventState, address, layout_index};

pub(crate) const CAPABILITIES: crate::Capabilities = crate::Capabilities { floating: true };

const CONTROL_SOCKET: &str = ".socket.sock";
const EVENT_SOCKET: &str = ".socket2.sock";

/// The monitor-config cache is shared across clones on purpose: `keyword monitor <name>,disable`
/// discards the mode, so the only way back to it is to have remembered it, and a service clones
/// this handle for every command it hands to a task.
#[derive(Debug, Clone)]
pub struct Hyprland {
    dir: PathBuf,
    disabled: Arc<Mutex<HashMap<String, MonitorConfig>>>,
}

impl Hyprland {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            disabled: Arc::default(),
        }
    }

    pub(crate) async fn snapshot(&self) -> Result<Snapshot, CompositorError> {
        let (monitors, workspaces, clients, active, layouts) = tokio::try_join!(
            self.json::<Vec<WireMonitor>>("j/monitors"),
            self.json::<Vec<WireWorkspace>>("j/workspaces"),
            self.json::<Vec<WireClient>>("j/clients"),
            self.json::<Value>("j/activewindow"),
            self.keyboard_layouts(),
        )?;

        let focused_output = monitors
            .iter()
            .find(|monitor| monitor.focused)
            .map(|monitor| monitor.name.clone());
        let active_workspaces: HashMap<u64, bool> = monitors
            .iter()
            .filter_map(|monitor| {
                let workspace = monitor.active_workspace.as_ref()?;
                Some((workspace.id, monitor.focused))
            })
            .collect();

        // Hyprland reports focus once, per session, rather than on each client — so unlike niri
        // the flag has to be filled in here or `is_focused` would be false on every window.
        let focused_window = active
            .get("address")
            .and_then(Value::as_str)
            .and_then(address);

        Ok(Snapshot {
            workspaces: workspaces
                .into_iter()
                .map(|workspace| workspace.into_model(&active_workspaces))
                .collect(),
            windows: clients
                .into_iter()
                .filter_map(WireClient::into_model)
                .map(|window| Window {
                    is_focused: Some(window.id) == focused_window,
                    ..window
                })
                .collect(),
            outputs: monitors.into_iter().map(WireMonitor::into_model).collect(),
            keyboard: layouts,
            focused_window,
            focused_output,
        })
    }

    pub(crate) async fn events(&self) -> Result<BoxStream<'static, Event>, CompositorError> {
        // Read before subscribing: Hyprland never announces the configured layouts, so without
        // this every `activelayout` line would arrive with nothing to resolve its index against.
        let codes = self.keyboard_layouts().await.unwrap_or_default().codes;

        let path = self.dir.join(EVENT_SOCKET);
        let stream = UnixStream::connect(&path)
            .await
            .map_err(|error| CompositorError::connect(&path, error))?;

        let start = (
            BufReader::new(stream).lines(),
            EventState::new(codes),
            VecDeque::new(),
        );
        Ok(
            stream::unfold(start, |(mut lines, mut state, mut pending)| async move {
                loop {
                    if let Some(event) = pending.pop_front() {
                        return Some((event, (lines, state, pending)));
                    }
                    let line = lines.next_line().await.ok()??;
                    pending.extend(state.decode(&line));
                }
            })
            .boxed(),
        )
    }

    pub(crate) async fn switch_keyboard_layout(
        &self,
        to: LayoutTarget,
    ) -> Result<(), CompositorError> {
        self.dispatch(&layout_command(to)).await
    }

    pub(crate) async fn focus_workspace(&self, to: WorkspaceTarget) -> Result<(), CompositorError> {
        self.dispatch(&workspace_command(&to)).await
    }

    pub(crate) async fn rename_workspace(
        &self,
        id: WorkspaceId,
        name: Option<&str>,
    ) -> Result<(), CompositorError> {
        self.dispatch(&rename_command(id, name)).await
    }

    pub(crate) async fn focus_window(&self, to: WindowTarget) -> Result<(), CompositorError> {
        self.dispatch(&window_command(to)).await
    }

    pub(crate) async fn set_output_enabled(
        &self,
        connector: &str,
        on: bool,
    ) -> Result<(), CompositorError> {
        if !on {
            // Remembered before the disable, because afterwards Hyprland no longer reports it.
            if let Ok(monitors) = self.json::<Vec<WireMonitor>>("j/monitors").await
                && let Some(monitor) = monitors.iter().find(|monitor| monitor.name == connector)
                && let Ok(mut cache) = self.disabled.lock()
            {
                cache.insert(connector.to_owned(), monitor.config());
            }
            return self
                .dispatch(&format!("keyword monitor {connector},disable"))
                .await;
        }

        let remembered = self
            .disabled
            .lock()
            .ok()
            .and_then(|mut cache| cache.remove(connector));
        let args = match remembered {
            Some(config) => config.keyword_args(connector),
            None => format!("{connector},preferred,auto,1"),
        };
        self.dispatch(&format!("keyword monitor {args}")).await
    }

    async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, CompositorError> {
        let (configured, devices) = tokio::try_join!(
            self.json::<WireOption>("j/getoption input:kb_layout"),
            self.json::<WireDevices>("j/devices"),
        )?;

        let codes: Vec<String> = configured
            .str
            .split(',')
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let active = devices.active_keymap();
        let current = active
            .as_deref()
            .and_then(|keymap| layout_index(&codes, keymap));

        // Hyprland names only the layout in use, so the rest keep their codes rather than have a
        // description invented for them.
        let mut names = codes.clone();
        if let (Some(index), Some(keymap)) = (current, active) {
            names[index] = keymap;
        }

        Ok(KeyboardLayouts {
            names,
            codes,
            current,
        })
    }

    async fn json<T: for<'de> Deserialize<'de>>(
        &self,
        command: &str,
    ) -> Result<T, CompositorError> {
        let reply = self.control(command).await?;
        serde_json::from_str(&reply).map_err(CompositorError::protocol)
    }

    async fn dispatch(&self, command: &str) -> Result<(), CompositorError> {
        let reply = self.control(command).await?;
        match reply.trim() {
            "ok" | "" => Ok(()),
            refusal => Err(CompositorError::Refused(refusal.to_owned())),
        }
    }

    async fn control(&self, command: &str) -> Result<String, CompositorError> {
        let path = self.dir.join(CONTROL_SOCKET);
        let mut stream = UnixStream::connect(&path)
            .await
            .map_err(|error| CompositorError::connect(&path, error))?;

        stream
            .write_all(command.as_bytes())
            .await
            .map_err(|error| CompositorError::connect(&path, error))?;
        stream.flush().await.map_err(CompositorError::protocol)?;

        let mut reply = String::new();
        stream
            .read_to_string(&mut reply)
            .await
            .map_err(CompositorError::protocol)?;

        match reply.is_empty() {
            true => Err(CompositorError::Closed),
            false => Ok(reply),
        }
    }
}

pub(crate) fn from_env() -> Option<Hyprland> {
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    match signature.is_empty() {
        true => None,
        false => Some(Hyprland::at(
            dirs::runtime_dir()?.join("hypr").join(signature),
        )),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct MonitorConfig {
    width: u32,
    height: u32,
    refresh_hz: f64,
    x: i32,
    y: i32,
    scale: f64,
    transform: u32,
}

impl MonitorConfig {
    /// Restores the mode `keyword monitor <name>,disable` threw away. A zero size means the monitor
    /// was already off when it was cached, and `preferred` is the only honest answer.
    fn keyword_args(&self, connector: &str) -> String {
        if self.width == 0 || self.height == 0 {
            return format!("{connector},preferred,auto,1");
        }

        let mut args = format!(
            "{connector},{}x{}@{:.3},{}x{},{}",
            self.width, self.height, self.refresh_hz, self.x, self.y, self.scale
        );
        if self.transform != 0 {
            args.push_str(&format!(",transform,{}", self.transform));
        }
        args
    }
}

fn scaled(pixels: u32, scale: f64) -> u32 {
    match scale > 0.0 {
        true => (f64::from(pixels) / scale).round() as u32,
        false => pixels,
    }
}

fn mhz(refresh_hz: f64) -> Option<u32> {
    let mhz = (refresh_hz * 1000.0).round();
    (mhz.is_finite() && (0.0..=f64::from(u32::MAX)).contains(&mhz)).then_some(mhz as u32)
}

#[derive(Deserialize)]
struct WireOption {
    #[serde(default)]
    str: String,
}

#[derive(Deserialize)]
struct WireDevices {
    #[serde(default)]
    keyboards: Vec<WireKeyboard>,
}

impl WireDevices {
    fn active_keymap(&self) -> Option<String> {
        self.keyboards
            .iter()
            .find(|keyboard| keyboard.main)
            .or_else(|| self.keyboards.first())
            .and_then(|keyboard| keyboard.active_keymap.clone())
    }
}

#[derive(Deserialize)]
struct WireKeyboard {
    #[serde(default)]
    main: bool,
    #[serde(default)]
    active_keymap: Option<String>,
}

#[derive(Deserialize)]
struct WireActiveWorkspace {
    id: u64,
}

#[derive(Deserialize)]
struct WireMonitor {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    make: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default, rename = "refreshRate")]
    refresh_rate: f64,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default = "one")]
    scale: f64,
    #[serde(default)]
    transform: u32,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    disabled: bool,
    #[serde(default, rename = "activeWorkspace")]
    active_workspace: Option<WireActiveWorkspace>,
}

fn one() -> f64 {
    1.0
}

impl WireMonitor {
    fn config(&self) -> MonitorConfig {
        MonitorConfig {
            width: self.width,
            height: self.height,
            refresh_hz: self.refresh_rate,
            x: self.x,
            y: self.y,
            scale: self.scale,
            transform: self.transform,
        }
    }

    fn into_model(self) -> Output {
        let enabled = !self.disabled;
        let current_mode = enabled.then(|| {
            Some(Mode {
                width: self.width,
                height: self.height,
                refresh_mhz: mhz(self.refresh_rate)?,
            })
        });

        Output {
            built_in: is_built_in(&self.name),
            // Hyprland reports the mode's pixels here; niri reports the scaled logical size, and
            // the model follows niri because that is what a layout calculation needs.
            logical: enabled.then(|| Logical {
                x: self.x,
                y: self.y,
                width: scaled(self.width, self.scale),
                height: scaled(self.height, self.scale),
                scale: self.scale,
            }),
            current_mode: current_mode.flatten(),
            connector: self.name,
            description: self.description,
            make: self.make,
            model: self.model,
            enabled,
        }
    }
}

#[derive(Deserialize)]
struct WireWorkspace {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    monitor: Option<String>,
    #[serde(default)]
    lastwindow: String,
}

impl WireWorkspace {
    fn into_model(self, active: &HashMap<u64, bool>) -> Workspace {
        Workspace {
            id: WorkspaceId(self.id),
            idx: None,
            name: (!self.name.is_empty()).then_some(self.name),
            output: self.monitor,
            is_active: active.contains_key(&self.id),
            is_focused: active.get(&self.id).copied().unwrap_or(false),
            is_urgent: false,
            active_window_id: address(&self.lastwindow),
        }
    }
}

#[derive(Deserialize)]
struct WireWorkspaceRef {
    id: u64,
}

#[derive(Deserialize)]
struct WireClient {
    address: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    class: String,
    #[serde(default)]
    pid: i32,
    #[serde(default)]
    workspace: Option<WireWorkspaceRef>,
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    urgent: bool,
}

impl WireClient {
    fn into_model(self) -> Option<Window> {
        Some(Window {
            id: address(&self.address)?,
            title: capped_title_str(&self.title),
            app_id: capped_app_id_str(&self.class),
            pid: (self.pid > 0).then_some(self.pid),
            workspace_id: self.workspace.map(|workspace| WorkspaceId(workspace.id)),
            // Hyprland reports focus per monitor, never on the client, so the snapshot fills this
            // in from `j/activewindow` rather than from here.
            is_focused: false,
            is_floating: self.floating,
            is_urgent: self.urgent,
            layout_order: None,
        })
    }
}

fn layout_command(to: LayoutTarget) -> String {
    let target = match to {
        LayoutTarget::Next => "next".to_owned(),
        LayoutTarget::Prev => "prev".to_owned(),
        LayoutTarget::Index(index) => index.to_string(),
    };
    format!("dispatch switchxkblayout all {target}")
}

fn workspace_command(to: &WorkspaceTarget) -> String {
    match to {
        WorkspaceTarget::Next => "dispatch workspace +1".to_owned(),
        WorkspaceTarget::Prev => "dispatch workspace -1".to_owned(),
        WorkspaceTarget::Id(id) => format!("dispatch workspace {}", id.0),
        WorkspaceTarget::Index(index) => format!("dispatch workspace {index}"),
        WorkspaceTarget::Name(name) => format!("dispatch workspace name:{name}"),
    }
}

fn window_command(to: WindowTarget) -> String {
    match to {
        WindowTarget::Next => "dispatch cyclenext".to_owned(),
        WindowTarget::Prev => "dispatch cyclenext prev".to_owned(),
        WindowTarget::Id(id) => format!("dispatch focuswindow address:0x{:x}", id.0),
    }
}

fn rename_command(id: WorkspaceId, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("dispatch renameworkspace {} {name}", id.0),
        None => format!("dispatch renameworkspace {}", id.0),
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use serde_json::json;

    use super::testing::FakeHyprland;
    use super::*;
    use crate::event::Resync;
    use crate::model::WindowId;

    fn snapshot_server() -> FakeHyprland {
        FakeHyprland::spawn(
            |command| match command {
                "j/monitors" => json!([
                    { "name": "eDP-1", "description": "Samsung 14\"", "make": "Samsung",
                      "model": "ATNA60CL10", "width": 2880, "height": 1800,
                      "refreshRate": 120.001, "x": 0, "y": 0, "scale": 1.25, "transform": 0,
                      "focused": true, "disabled": false, "activeWorkspace": { "id": 5 } },
                    { "name": "DP-3", "disabled": true }
                ])
                .to_string(),
                "j/workspaces" => json!([
                    { "id": 5, "name": "5", "monitor": "eDP-1", "lastwindow": "0x9" },
                    { "id": 6, "name": "chat", "monitor": "DP-3", "lastwindow": "0x0" }
                ])
                .to_string(),
                "j/clients" => json!([
                    { "address": "0x9", "title": "a terminal", "class": "foot", "pid": 4265,
                      "workspace": { "id": 5 }, "floating": false, "urgent": false }
                ])
                .to_string(),
                "j/activewindow" => json!({ "address": "0x9" }).to_string(),
                "j/getoption input:kb_layout" => json!({ "str": "pl,ru" }).to_string(),
                "j/devices" => json!({ "keyboards": [
                    { "main": false, "active_keymap": "English (US)" },
                    { "main": true, "active_keymap": "Russian" }
                ] })
                .to_string(),
                other => panic!("the fake was asked for {other}"),
            },
            Vec::new(),
        )
    }

    #[tokio::test]
    async fn a_snapshot_reads_every_part_of_the_model() {
        let server = snapshot_server();
        let snapshot = Hyprland::at(&server.dir)
            .snapshot()
            .await
            .expect("snapshot");

        assert_eq!(snapshot.focused_output.as_deref(), Some("eDP-1"));
        assert_eq!(snapshot.focused_window, Some(WindowId(9)));
        assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
        assert_eq!(snapshot.windows[0].workspace_id, Some(WorkspaceId(5)));
        assert_eq!(snapshot.windows[0].layout_order, None);
        assert_eq!(snapshot.workspaces[0].name.as_deref(), Some("5"));
        assert!(snapshot.workspaces[0].is_active && snapshot.workspaces[0].is_focused);
        assert!(!snapshot.workspaces[1].is_active);
        assert_eq!(snapshot.workspaces[0].active_window_id, Some(WindowId(9)));
        // Hyprland writes `0x0` for an empty workspace, which is not a window at address zero.
        assert_eq!(snapshot.workspaces[1].active_window_id, None);
    }

    /// Hyprland reports focus once for the session rather than on each client, so without filling
    /// it in `is_focused` would be false on every window while niri sets it on one.
    #[tokio::test]
    async fn the_focused_window_is_marked_the_way_niri_marks_it() {
        let server = snapshot_server();
        let snapshot = Hyprland::at(&server.dir)
            .snapshot()
            .await
            .expect("snapshot");

        assert_eq!(snapshot.focused_window, Some(WindowId(9)));
        assert_eq!(
            snapshot
                .windows
                .iter()
                .filter(|window| window.is_focused)
                .map(|window| window.id)
                .collect::<Vec<_>>(),
            [WindowId(9)],
        );
    }

    /// Hyprland sends an absent title or class as `""` where niri sends `null`. Both have to reach
    /// a caller as `None`, or a panel renders an empty label under one compositor and nothing
    /// under the other.
    #[tokio::test]
    async fn an_empty_title_is_absent_rather_than_an_empty_string() {
        let server = FakeHyprland::spawn(
            |command| match command {
                "j/clients" => json!([{ "address": "0x1", "title": "", "class": "" }]).to_string(),
                "j/monitors" | "j/workspaces" => "[]".to_owned(),
                "j/activewindow" => "{}".to_owned(),
                "j/getoption input:kb_layout" => json!({ "str": "" }).to_string(),
                "j/devices" => json!({ "keyboards": [] }).to_string(),
                other => panic!("the fake was asked for {other}"),
            },
            Vec::new(),
        );

        let snapshot = Hyprland::at(&server.dir)
            .snapshot()
            .await
            .expect("snapshot");

        assert_eq!(snapshot.windows[0].title, None);
        assert_eq!(snapshot.windows[0].app_id, None);
    }

    /// The same question niri answers with a null mode, so both have to land on `enabled: false`.
    #[tokio::test]
    async fn a_disabled_monitor_reports_the_same_shape_niri_does() {
        let server = snapshot_server();
        let snapshot = Hyprland::at(&server.dir)
            .snapshot()
            .await
            .expect("snapshot");

        let built_in = &snapshot.outputs[0];
        let external = &snapshot.outputs[1];

        assert!(built_in.enabled && built_in.built_in);
        assert_eq!(
            built_in.current_mode,
            Some(Mode {
                width: 2880,
                height: 1800,
                refresh_mhz: 120_001
            })
        );
        assert_eq!(built_in.description.as_deref(), Some("Samsung 14\""));
        // Hyprland reports the mode's pixels; niri reports the scaled logical size. 2880 / 1.25.
        let logical = built_in
            .logical
            .as_ref()
            .expect("an enabled output has a logical size");
        assert_eq!((logical.width, logical.height), (2304, 1440));
        assert_eq!(logical.scale, 1.25);
        assert!(!external.enabled);
        assert_eq!(external.current_mode, None);
        assert_eq!(external.logical, None);
    }

    /// Hyprland gives xkb codes and a display name from opposite ends, so both halves have to be
    /// filled — otherwise the same layout renders as "Russian" here and "ru" under niri.
    #[tokio::test]
    async fn both_names_and_codes_are_populated() {
        let server = snapshot_server();
        let snapshot = Hyprland::at(&server.dir)
            .snapshot()
            .await
            .expect("snapshot");

        assert_eq!(snapshot.keyboard.codes, ["pl", "ru"]);
        assert_eq!(snapshot.keyboard.names, ["pl", "Russian"]);
        assert_eq!(snapshot.keyboard.current, Some(1));
    }

    #[tokio::test]
    async fn a_refusal_carries_the_reason_hyprland_gave() {
        let server = FakeHyprland::spawn(|_| "Invalid dispatcher".to_owned(), Vec::new());

        let failure = Hyprland::at(&server.dir)
            .focus_window(WindowTarget::Next)
            .await
            .expect_err("refused");

        assert!(matches!(failure, CompositorError::Refused(reason) if reason.contains("Invalid")));
    }

    #[tokio::test]
    async fn an_empty_reply_is_a_closed_connection() {
        let server = FakeHyprland::spawn(|_| String::new(), Vec::new());

        let failure = Hyprland::at(&server.dir)
            .focus_window(WindowTarget::Next)
            .await
            .expect_err("closed");

        assert!(matches!(failure, CompositorError::Closed));
    }

    #[tokio::test]
    async fn a_missing_socket_names_the_path_it_tried() {
        let failure = Hyprland::at("/nonexistent/hypr")
            .focus_window(WindowTarget::Next)
            .await
            .expect_err("no socket");

        assert!(
            failure
                .to_string()
                .contains("/nonexistent/hypr/.socket.sock"),
            "expected the path in {failure}"
        );
    }

    #[test]
    fn every_target_serializes_to_the_command_hyprland_expects() {
        assert_eq!(
            layout_command(LayoutTarget::Index(2)),
            "dispatch switchxkblayout all 2"
        );
        assert_eq!(
            layout_command(LayoutTarget::Next),
            "dispatch switchxkblayout all next"
        );
        assert_eq!(
            layout_command(LayoutTarget::Prev),
            "dispatch switchxkblayout all prev"
        );

        assert_eq!(
            workspace_command(&WorkspaceTarget::Id(WorkspaceId(5))),
            "dispatch workspace 5"
        );
        assert_eq!(
            workspace_command(&WorkspaceTarget::Index(3)),
            "dispatch workspace 3"
        );
        assert_eq!(
            workspace_command(&WorkspaceTarget::Name("chat".to_owned())),
            "dispatch workspace name:chat"
        );
        assert_eq!(
            workspace_command(&WorkspaceTarget::Next),
            "dispatch workspace +1"
        );
        assert_eq!(
            workspace_command(&WorkspaceTarget::Prev),
            "dispatch workspace -1"
        );

        assert_eq!(
            window_command(WindowTarget::Id(WindowId(0x5591_e8b2_f5a0))),
            "dispatch focuswindow address:0x5591e8b2f5a0"
        );
        assert_eq!(window_command(WindowTarget::Next), "dispatch cyclenext");
        assert_eq!(
            window_command(WindowTarget::Prev),
            "dispatch cyclenext prev"
        );

        assert_eq!(
            rename_command(WorkspaceId(3), Some("chat")),
            "dispatch renameworkspace 3 chat"
        );
        assert_eq!(
            rename_command(WorkspaceId(3), None),
            "dispatch renameworkspace 3"
        );
    }

    /// Disabling a monitor discards its mode, so the only way back is the remembered config. The
    /// zero-size fallback covers a monitor that was already off when it was first seen.
    #[test]
    fn a_remembered_monitor_config_restores_its_mode() {
        let config = MonitorConfig {
            width: 2880,
            height: 1800,
            refresh_hz: 120.001,
            x: 100,
            y: 0,
            scale: 1.25,
            transform: 0,
        };
        assert_eq!(
            config.keyword_args("eDP-1"),
            "eDP-1,2880x1800@120.001,100x0,1.25"
        );

        let rotated = MonitorConfig {
            transform: 3,
            ..config
        };
        assert_eq!(
            rotated.keyword_args("eDP-1"),
            "eDP-1,2880x1800@120.001,100x0,1.25,transform,3"
        );

        assert_eq!(
            MonitorConfig::default().keyword_args("DP-3"),
            "DP-3,preferred,auto,1"
        );
    }

    async fn events_from(lines: &[&str]) -> Vec<Event> {
        let server = FakeHyprland::spawn(
            |command| match command {
                "j/getoption input:kb_layout" => json!({ "str": "pl,ru" }).to_string(),
                "j/devices" => json!({ "keyboards": [] }).to_string(),
                other => panic!("the fake was asked for {other}"),
            },
            lines.iter().map(|line| (*line).to_owned()).collect(),
        );

        Hyprland::at(&server.dir)
            .events()
            .await
            .expect("subscribed")
            .collect()
            .await
    }

    #[tokio::test]
    async fn addresses_arrive_bare_in_events_and_prefixed_in_json() {
        let events = events_from(&["activewindowv2>>5591e8b2f5a0", "closewindow>>0x9"]).await;

        assert_eq!(
            events,
            [
                Event::WindowFocusChanged(Some(WindowId(0x5591_e8b2_f5a0))),
                Event::WindowClosed(WindowId(9)),
            ]
        );
    }

    /// Hyprland's fine-grained events carry an address and nothing else, so the honest answer is a
    /// resync rather than a half-built record.
    #[tokio::test]
    async fn structural_events_ask_for_a_resync() {
        let events = events_from(&[
            "openwindow>>9,1,foot,a terminal",
            "monitorremovedv2>>1,DP-3,Dell",
            "configreloaded>>",
        ])
        .await;

        assert_eq!(
            events,
            [
                Event::Resync(Resync::Structure),
                Event::Resync(Resync::Outputs),
                Event::Resync(Resync::Keyboard),
            ]
        );
    }

    #[tokio::test]
    async fn an_unrecognized_line_is_skipped_and_the_stream_survives() {
        let events = events_from(&[
            "somethingHyprlandAddedLater>>1,2,3",
            "no separator at all",
            "closewindow>>0x4",
        ])
        .await;

        assert_eq!(events, [Event::WindowClosed(WindowId(4))]);
    }

    /// `activelayout` reports a display name against a configured list of xkb codes, and a keyboard
    /// name may itself contain a comma.
    #[tokio::test]
    async fn a_layout_switch_resolves_a_display_name_to_a_configured_code() {
        let events = events_from(&[
            "activelayout>>AT Translated Set 2, keyboard,Russian",
            "activelayout>>kbd,Klingon",
        ])
        .await;

        assert_eq!(
            events,
            [Event::KeyboardLayoutSwitched {
                idx: 1,
                name: Some("Russian".to_owned()),
            }]
        );
    }

    /// A window title is attacker-controlled and Hyprland packs it into a comma-separated line, so
    /// the split must not let a crafted title impersonate another field.
    #[tokio::test]
    async fn a_title_containing_a_comma_does_not_shift_the_other_fields() {
        let server = FakeHyprland::spawn(
            |command| match command {
                "j/clients" => json!([{ "address": "0x1",
                                        "title": "a, b, c\u{2068}spoof\u{2069}",
                                        "class": "foot" }])
                .to_string(),
                "j/monitors" | "j/workspaces" => "[]".to_owned(),
                "j/activewindow" => "{}".to_owned(),
                "j/getoption input:kb_layout" => json!({ "str": "" }).to_string(),
                "j/devices" => json!({ "keyboards": [] }).to_string(),
                other => panic!("the fake was asked for {other}"),
            },
            Vec::new(),
        );

        let snapshot = Hyprland::at(&server.dir)
            .snapshot()
            .await
            .expect("snapshot");

        assert_eq!(snapshot.windows[0].title.as_deref(), Some("a, b, cspoof"));
        assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
    }
}
