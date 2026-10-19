mod event;
#[cfg(test)]
mod testing;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::stream::BoxStream;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::OnceCell;

use crate::error::CompositorError;
use crate::event::{Event, LineDecoder, line_events};
use crate::model::{
    KeyboardLayouts, LayoutTarget, Logical, Mode, Output, Rect, Snapshot, Window, WindowId,
    WindowTarget, Workspace, WorkspaceId, WorkspaceTarget, capped_app_id_str, capped_edid_str,
    capped_name_str, capped_title_str, fill_window_outputs, is_built_in,
};
use event::{EventState, Shared, address};

pub(crate) const CAPABILITIES: crate::Capabilities = crate::Capabilities {
    output_power: true,
    blank_screens: false,
    overview_backdrop: false,
};

const CONTROL_SOCKET: &str = ".socket.sock";
const EVENT_SOCKET: &str = ".socket2.sock";
const DIALECT_PROBE: &str = "eval return 1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Legacy,
    Lua,
}

#[derive(Debug, Clone)]
pub struct Hyprland {
    dir: PathBuf,
    shared: Arc<Shared>,
    dialect: Arc<OnceCell<Dialect>>,
}

impl Hyprland {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            shared: Arc::default(),
            dialect: Arc::default(),
        }
    }

    pub(crate) fn from_env() -> Option<Self> {
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        match signature.is_empty() {
            true => None,
            false => Some(Self::at(dirs::runtime_dir()?.join("hypr").join(signature))),
        }
    }

    pub(crate) async fn allow_session_lock_restore(&self) -> Result<bool, CompositorError> {
        #[derive(Deserialize)]
        struct Reading {
            int: i64,
        }
        let option = self
            .json::<Reading>("j/getoption misc:allow_session_lock_restore")
            .await?;
        Ok(option.int != 0)
    }

    pub(crate) async fn snapshot(&self) -> Result<Snapshot, CompositorError> {
        let (monitors, workspaces, clients, active, layouts) = tokio::try_join!(
            self.json::<Vec<WireMonitor>>("j/monitors all"),
            self.json::<Vec<WireWorkspace>>("j/workspaces"),
            self.json::<Vec<WireClient>>("j/clients"),
            self.json::<Value>("j/activewindow"),
            self.keyboard_layouts(),
        )?;

        let focused_output = monitors
            .iter()
            .find(|monitor| monitor.focused)
            .map(|monitor| monitor.name.clone());
        let active_workspaces: HashMap<i64, bool> = monitors
            .iter()
            .filter_map(|monitor| {
                let workspace = monitor.active_workspace.as_ref()?;
                Some((workspace.id, monitor.focused))
            })
            .collect();
        let monitor_origins: HashMap<i64, (f64, f64)> = monitors
            .iter()
            .map(|monitor| (monitor.id, (f64::from(monitor.x), f64::from(monitor.y))))
            .collect();

        // Hyprland reports focus once, per session, rather than on each client — so unlike niri
        // the flag has to be filled in here or `is_focused` would be false on every window.
        let focused_window = active
            .get("address")
            .and_then(Value::as_str)
            .and_then(address);

        let workspaces: Vec<Workspace> = workspaces
            .into_iter()
            .filter(|workspace| !workspace.name.starts_with("special:"))
            .map(|workspace| workspace.into_model(&active_workspaces))
            .collect();
        let mut windows: Vec<Window> = clients
            .into_iter()
            .filter_map(|client| client.into_model(&monitor_origins))
            .map(|window| Window {
                is_focused: Some(window.id) == focused_window,
                is_urgent: self.shared.is_urgent(window.id),
                ..window
            })
            .collect();
        fill_window_outputs(&mut windows, &workspaces);

        Ok(Snapshot {
            workspaces,
            windows,
            outputs: monitors.into_iter().map(WireMonitor::into_model).collect(),
            keyboard: layouts,
            focused_window,
            focused_output,
            casts: self.shared.casts(),
        })
    }

    pub(crate) async fn events(&self) -> Result<BoxStream<'static, Event>, CompositorError> {
        let path = self.dir.join(EVENT_SOCKET);
        let stream = connect(&path).await?;
        self.shared.reset();

        Ok(line_events(
            BufReader::new(stream),
            EventDecoder {
                state: EventState::new(self.shared.clone()),
                hyprland: self.clone(),
            },
        ))
    }

    pub(crate) async fn switch_keyboard_layout(
        &self,
        to: LayoutTarget,
    ) -> Result<(), CompositorError> {
        let reply = self.control(&layout_command(to)).await?;
        match reply.trim() {
            "ok" | "" => Ok(()),
            refusal => Err(CompositorError::Refused(refusal.to_owned())),
        }
    }

    pub(crate) async fn focus_workspace(&self, to: WorkspaceTarget) -> Result<(), CompositorError> {
        let selector = self.resolve_selector(&to).await?;
        let dialect = self.dialect().await?;
        self.dispatch(&workspace_command(dialect, &selector)).await
    }

    pub(crate) async fn rename_workspace(
        &self,
        id: WorkspaceId,
        name: Option<&str>,
    ) -> Result<(), CompositorError> {
        let selector = self.resolve_id_selector(id).await?;
        let dialect = self.dialect().await?;
        self.dispatch(&rename_command(dialect, &selector, name))
            .await
    }

    pub(crate) async fn focus_window(&self, to: WindowTarget) -> Result<(), CompositorError> {
        let dialect = self.dialect().await?;
        self.dispatch(&window_command(dialect, to)).await
    }

    async fn resolve_selector(&self, to: &WorkspaceTarget) -> Result<String, CompositorError> {
        match plain_selector(to) {
            Some(selector) => Ok(selector),
            None => match to {
                WorkspaceTarget::Id(id) => self.resolve_id_selector(*id).await,
                _ => unreachable!("plain_selector only defers a negative id"),
            },
        }
    }

    async fn resolve_id_selector(&self, id: WorkspaceId) -> Result<String, CompositorError> {
        let signed = id.0 as i64;
        if signed >= 0 {
            return Ok(signed.to_string());
        }
        let workspaces = self.json::<Vec<WireWorkspace>>("j/workspaces").await?;
        let name = workspaces
            .iter()
            .find(|workspace| workspace.id == signed)
            .map(|workspace| workspace.name.clone())
            .ok_or_else(|| CompositorError::Refused(format!("unknown workspace {signed}")))?;
        Ok(format!("name:{name}"))
    }

    pub(crate) async fn set_output_enabled(
        &self,
        connector: &str,
        on: bool,
    ) -> Result<(), CompositorError> {
        match on {
            true => self.dispatch("reload").await,
            false => {
                self.dispatch_monitor(
                    &format!("keyword monitor {connector},disable"),
                    &disable_eval(connector),
                )
                .await
            }
        }
    }

    async fn dispatch_monitor(&self, keyword: &str, eval: &str) -> Result<(), CompositorError> {
        match self.dispatch(keyword).await {
            Ok(()) => Ok(()),
            Err(CompositorError::Refused(reason)) if reason.contains("non-legacy") => {
                self.dispatch(eval).await
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) async fn power_off_monitors(&self) -> Result<(), CompositorError> {
        Err(CompositorError::Unavailable("blank the screens"))
    }

    pub(crate) async fn stop_screencast(&self, _session_id: u64) -> Result<(), CompositorError> {
        Err(CompositorError::Unavailable("stop a screencast"))
    }

    pub(crate) async fn screenshot_window(
        &self,
        _id: WindowId,
        _path: &Path,
    ) -> Result<(), CompositorError> {
        Err(CompositorError::Unavailable("screenshot a window"))
    }

    pub(crate) async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, CompositorError> {
        let devices = self.json::<WireDevices>("j/devices").await?;
        let Some(keyboard) = devices.main_keyboard() else {
            return Ok(KeyboardLayouts::default());
        };

        let codes: Vec<String> = keyboard
            .layout
            .split(',')
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let current = keyboard
            .active_layout_index
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < codes.len());

        let mut names = codes.clone();
        if let (Some(index), Some(keymap)) = (current, keyboard.active_keymap.clone()) {
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

    async fn dialect(&self) -> Result<Dialect, CompositorError> {
        self.dialect
            .get_or_try_init(|| async {
                let reply = self.control(DIALECT_PROBE).await?;
                Ok(match reply.trim() {
                    "ok" => Dialect::Lua,
                    _ => Dialect::Legacy,
                })
            })
            .await
            .copied()
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
        let mut stream = connect(&path).await?;

        stream
            .write_all(command.as_bytes())
            .await
            .map_err(|error| CompositorError::connect(&path, error))?;
        stream.flush().await.map_err(CompositorError::protocol)?;

        let mut bytes = Vec::new();
        stream
            .read_to_end(&mut bytes)
            .await
            .map_err(CompositorError::protocol)?;
        let reply = String::from_utf8_lossy(&bytes).into_owned();

        match reply.is_empty() {
            true => Err(CompositorError::Closed),
            false => Ok(reply),
        }
    }
}

struct EventDecoder {
    state: EventState,
    hyprland: Hyprland,
}

impl LineDecoder for EventDecoder {
    async fn decode(&mut self, line: &str) -> Vec<Event> {
        if EventState::is_layout_event(line) {
            let mut events = Vec::new();
            if let Ok(layouts) = self.hyprland.keyboard_layouts().await
                && let Some(idx) = layouts.current
            {
                events.push(Event::KeyboardLayoutSwitched(idx));
            }
            events
        } else {
            self.state.decode(line)
        }
    }
}

const CONNECT_ATTEMPTS: u32 = 8;
const CONNECT_BACKOFF: tokio::time::Duration = tokio::time::Duration::from_millis(4);

async fn connect(path: &std::path::Path) -> Result<UnixStream, CompositorError> {
    for attempt in 1..CONNECT_ATTEMPTS {
        match UnixStream::connect(path).await {
            Ok(stream) => return Ok(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(CONNECT_BACKOFF * attempt).await;
            }
            Err(error) => return Err(CompositorError::connect(path, error)),
        }
    }

    UnixStream::connect(path)
        .await
        .map_err(|error| CompositorError::connect(path, error))
}

fn lua_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

fn disable_eval(connector: &str) -> String {
    format!(
        "eval hl.monitor({{ output = {}, disabled = true }})",
        lua_string(connector)
    )
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
struct WireDevices {
    #[serde(default)]
    keyboards: Vec<WireKeyboard>,
}

impl WireDevices {
    fn main_keyboard(&self) -> Option<&WireKeyboard> {
        self.keyboards
            .iter()
            .find(|keyboard| keyboard.main)
            .or_else(|| self.keyboards.first())
    }
}

#[derive(Deserialize)]
struct WireKeyboard {
    #[serde(default)]
    main: bool,
    #[serde(default)]
    layout: String,
    #[serde(default)]
    active_layout_index: Option<u32>,
    #[serde(default)]
    active_keymap: Option<String>,
}

#[derive(Deserialize)]
struct WireActiveWorkspace {
    id: i64,
}

#[derive(Deserialize)]
struct WireMonitor {
    id: i64,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    make: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial: Option<String>,
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
            description: self.description.as_deref().and_then(capped_name_str),
            make: self.make.as_deref().and_then(capped_edid_str),
            model: self.model.as_deref().and_then(capped_edid_str),
            serial: self.serial.as_deref().and_then(capped_edid_str),
            enabled,
        }
    }
}

#[derive(Deserialize)]
struct WireWorkspace {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    monitor: Option<String>,
    #[serde(default)]
    lastwindow: String,
}

impl WireWorkspace {
    fn into_model(self, active: &HashMap<i64, bool>) -> Workspace {
        let name = match self.name == self.id.to_string() {
            true => None,
            false => capped_name_str(&self.name),
        };
        Workspace {
            id: WorkspaceId(self.id as u64),
            idx: None,
            name,
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
    id: i64,
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
    monitor: Option<i64>,
    #[serde(default)]
    at: Option<(f64, f64)>,
    #[serde(default)]
    size: Option<(u32, u32)>,
}

impl WireClient {
    fn into_model(self, monitor_origins: &HashMap<i64, (f64, f64)>) -> Option<Window> {
        let origin = self.monitor.and_then(|id| monitor_origins.get(&id));
        let rect = match (self.at, self.size, origin) {
            (Some((x, y)), Some((width, height)), Some(&(origin_x, origin_y))) => Some(Rect {
                x: x - origin_x,
                y: y - origin_y,
                width: f64::from(width),
                height: f64::from(height),
            }),
            _ => None,
        };

        Some(Window {
            id: address(&self.address)?,
            title: capped_title_str(&self.title),
            app_id: capped_app_id_str(&self.class),
            pid: (self.pid > 0).then_some(self.pid),
            workspace_id: self
                .workspace
                .map(|workspace| WorkspaceId(workspace.id as u64)),
            // Hyprland reports focus per monitor, never on the client, so the snapshot fills this
            // in from `j/activewindow` rather than from here.
            is_focused: false,
            is_floating: self.floating,
            is_urgent: false,
            layout_order: None,
            size: self.size,
            rect,
            output: None,
        })
    }
}

fn layout_command(to: LayoutTarget) -> String {
    let target = match to {
        LayoutTarget::Next => "next".to_owned(),
        LayoutTarget::Prev => "prev".to_owned(),
        LayoutTarget::Index(index) => index.to_string(),
    };
    format!("switchxkblayout all {target}")
}

fn plain_selector(to: &WorkspaceTarget) -> Option<String> {
    match to {
        WorkspaceTarget::Next => Some("e+1".to_owned()),
        WorkspaceTarget::Prev => Some("e-1".to_owned()),
        WorkspaceTarget::Id(id) if (id.0 as i64) >= 0 => Some(id.0.to_string()),
        WorkspaceTarget::Id(_) => None,
        WorkspaceTarget::Index(index) => Some(index.to_string()),
        WorkspaceTarget::Name(name) => Some(format!("name:{name}")),
    }
}

fn workspace_command(dialect: Dialect, selector: &str) -> String {
    match dialect {
        Dialect::Legacy => format!("dispatch workspace {selector}"),
        Dialect::Lua => format!(
            "dispatch hl.dsp.focus({{ workspace = {} }})",
            lua_string(selector)
        ),
    }
}

fn window_command(dialect: Dialect, to: WindowTarget) -> String {
    match (dialect, to) {
        (Dialect::Legacy, WindowTarget::Next) => "dispatch cyclenext".to_owned(),
        (Dialect::Legacy, WindowTarget::Prev) => "dispatch cyclenext prev".to_owned(),
        (Dialect::Legacy, WindowTarget::Id(id)) => {
            format!("dispatch focuswindow address:0x{:x}", id.0)
        }
        (Dialect::Lua, WindowTarget::Next) => "dispatch hl.dsp.window.cycle_next()".to_owned(),
        (Dialect::Lua, WindowTarget::Prev) => {
            "dispatch hl.dsp.window.cycle_next({ next = false })".to_owned()
        }
        (Dialect::Lua, WindowTarget::Id(id)) => {
            format!(
                "dispatch hl.dsp.focus({{ window = \"address:0x{:x}\" }})",
                id.0
            )
        }
    }
}

fn rename_command(dialect: Dialect, selector: &str, name: Option<&str>) -> String {
    match (dialect, name) {
        (Dialect::Legacy, Some(name)) => format!("dispatch renameworkspace {selector} {name}"),
        (Dialect::Legacy, None) => format!("dispatch renameworkspace {selector}"),
        (Dialect::Lua, Some(name)) => format!(
            "dispatch hl.dsp.workspace.rename({{ workspace = {}, name = {} }})",
            lua_string(selector),
            lua_string(name)
        ),
        (Dialect::Lua, None) => format!(
            "dispatch hl.dsp.workspace.rename({{ workspace = {} }})",
            lua_string(selector)
        ),
    }
}

#[cfg(test)]
mod tests;
