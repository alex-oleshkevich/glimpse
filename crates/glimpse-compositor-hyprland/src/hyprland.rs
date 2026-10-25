mod event;
mod outputs;
#[cfg(test)]
mod testing;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use glimpse_compositors::backend::Backend;
use glimpse_compositors::contract::{Capability, Error};
use glimpse_compositors::event::{Event, LineDecoder, line_events};
use glimpse_compositors::model::{
    Focus, KeyboardLayouts, Logical, Mode, Output, Rect, Snapshot, Window, WindowId, Workspace,
    WorkspaceId,
};
use glimpse_compositors::sanitize::{
    capped_app_id_str, capped_edid_str, capped_name_str, capped_title_str,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::OnceCell;

use event::{Shared, address};

const CONTROL_SOCKET: &str = ".socket.sock";
const EVENT_SOCKET: &str = ".socket2.sock";
const DIALECT_PROBE: &str = "eval return 1";
const PROBE_DEADLINE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Legacy,
    Lua,
}

#[derive(Clone)]
pub struct Hyprland {
    dir: PathBuf,
    shared: Arc<Shared>,
    dialect: Arc<OnceCell<Dialect>>,
    outputs: Arc<tokio::sync::Mutex<Option<outputs::Manager>>>,
}

impl Hyprland {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            shared: Arc::default(),
            dialect: Arc::default(),
            outputs: Arc::default(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        match signature.is_empty() {
            true => None,
            false => Some(Self::at(dirs::runtime_dir()?.join("hypr").join(signature))),
        }
    }

    async fn resolve_id_selector(&self, id: WorkspaceId) -> Result<String, Error> {
        let signed = id.0 as i64;
        if signed >= 0 {
            return Ok(signed.to_string());
        }
        let workspaces = self.json::<Vec<WireWorkspace>>("j/workspaces").await?;
        let name = workspaces
            .iter()
            .find(|workspace| workspace.id == signed)
            .map(|workspace| workspace.name.clone())
            .ok_or_else(|| Error::Refused(format!("unknown workspace {signed}")))?;
        Ok(format!("name:{name}"))
    }

    async fn json<T: for<'de> Deserialize<'de>>(&self, command: &str) -> Result<T, Error> {
        let reply = self.control(command).await?;
        serde_json::from_str(&reply)
            .map_err(|error| Error::Failed(format!("undecodable reply: {error}")))
    }

    async fn dialect(&self) -> Result<Dialect, Error> {
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

    async fn dispatch(&self, command: &str) -> Result<(), Error> {
        let reply = self.control(command).await?;
        match reply.trim() {
            "ok" => Ok(()),
            refusal => Err(Error::Refused(refusal.to_owned())),
        }
    }

    async fn restore_output(
        &self,
        mut outputs: tokio::sync::OwnedMutexGuard<Option<outputs::Manager>>,
        connector: String,
        enabled: bool,
    ) -> Result<(), Error> {
        let (mut outputs, restored) = tokio::task::spawn_blocking(move || {
            let restored = outputs
                .as_mut()
                .ok_or_else(|| outputs::failed("output manager is unavailable"))
                .and_then(|manager| manager.restore_enabled(&connector, enabled));
            (outputs, restored)
        })
        .await
        .map_err(outputs::failed)?;
        restored?;
        if !outputs
            .as_ref()
            .is_some_and(outputs::Manager::has_committed)
        {
            *outputs = None;
            return Ok(());
        }
        let socket = outputs::socket(&self.dir).await?;
        tokio::task::spawn_blocking(move || {
            let previous = outputs
                .as_ref()
                .ok_or_else(|| outputs::failed("output manager is unavailable"))?;
            let replacement = outputs::Manager::replacement(socket, previous)?;
            *outputs = Some(replacement);
            Ok(())
        })
        .await
        .map_err(outputs::failed)?
    }

    async fn control(&self, command: &str) -> Result<String, Error> {
        let path = self.dir.join(CONTROL_SOCKET);
        let mut stream = connect(&path).await?;

        stream
            .write_all(command.as_bytes())
            .await
            .map_err(|error| unreachable_socket(&path, error))?;
        stream
            .flush()
            .await
            .map_err(|error| unreachable_socket(&path, error))?;

        let mut bytes = Vec::new();
        stream
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| unreachable_socket(&path, error))?;
        let reply = String::from_utf8_lossy(&bytes).into_owned();

        match reply.is_empty() {
            true => Err(Error::Unavailable(
                "the compositor closed the connection before replying".to_owned(),
            )),
            false => Ok(reply),
        }
    }
}

impl Backend for Hyprland {
    const NAME: &'static str = "hyprland";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::RenameWorkspace,
        Capability::OutputPower,
        Capability::WindowGeometry,
    ];

    async fn snapshot(&self) -> Result<Snapshot, Error> {
        let (monitors, workspaces, clients, active, keyboard) = tokio::try_join!(
            self.json::<Vec<WireMonitor>>("j/monitors all"),
            self.json::<Vec<WireWorkspace>>("j/workspaces"),
            self.json::<Vec<WireClient>>("j/clients"),
            self.json::<Value>("j/activewindow"),
            self.keyboard_layouts(),
        )?;

        let output = monitors
            .iter()
            .find(|monitor| monitor.focused)
            .map(|monitor| monitor.name.clone());
        let window = active
            .get("address")
            .and_then(Value::as_str)
            .and_then(address);
        let active_workspaces: Vec<i64> = monitors
            .iter()
            .filter_map(|monitor| Some(monitor.active_workspace.as_ref()?.id))
            .collect();

        Ok(Snapshot {
            workspaces: workspaces
                .into_iter()
                .filter(|workspace| !workspace.name.starts_with("special:"))
                .map(|workspace| workspace.into_model(&active_workspaces))
                .collect(),
            windows: clients
                .into_iter()
                .filter_map(|client| client.into_model())
                .map(|window| Window {
                    urgent: self.shared.is_urgent(window.id),
                    ..window
                })
                .collect(),
            outputs: monitors.into_iter().map(WireMonitor::into_model).collect(),
            focus: Focus { output, window },
            keyboard,
            casts: self.shared.casts(),
        })
    }

    async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, Error> {
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
            .filter(|index| (*index as usize) < codes.len());

        let mut names = codes.clone();
        if let (Some(index), Some(keymap)) = (current, keyboard.active_keymap.clone()) {
            names[index as usize] = keymap;
        }

        Ok(KeyboardLayouts {
            names,
            codes,
            current,
        })
    }

    async fn session_lock_restore(&self) -> Result<Option<bool>, Error> {
        match self
            .json::<Value>("j/getoption misc:allow_session_lock_restore")
            .await
        {
            Ok(option) => Ok(option.get("bool").and_then(Value::as_bool).or_else(|| {
                option
                    .get("int")
                    .and_then(Value::as_i64)
                    .map(|value| value != 0)
            })),
            Err(Error::Failed(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn events(&self) -> Result<BoxStream<'static, Event>, Error> {
        let stream = connect(&self.dir.join(EVENT_SOCKET)).await?;
        let events = line_events(
            BufReader::new(stream),
            EventDecoder {
                shared: self.shared.clone(),
                hyprland: self.clone(),
            },
        );
        Ok(grace_hold(events, self.shared.clone()))
    }

    async fn focus_workspace(&self, id: WorkspaceId) -> Result<(), Error> {
        let selector = self.resolve_id_selector(id).await?;
        let dialect = self.dialect().await?;
        self.dispatch(&workspace_command(dialect, &selector)).await
    }

    async fn step_workspace(&self, forward: bool) -> Result<(), Error> {
        let selector = match forward {
            true => "e+1",
            false => "e-1",
        };
        let dialect = self.dialect().await?;
        self.dispatch(&workspace_command(dialect, selector)).await
    }

    async fn rename_workspace(&self, id: WorkspaceId, name: String) -> Result<(), Error> {
        let selector = self.resolve_id_selector(id).await?;
        let dialect = self.dialect().await?;
        let name = Some(name.as_str()).filter(|name| !name.is_empty());
        self.dispatch(&rename_command(dialect, &selector, name))
            .await
    }

    async fn focus_window(&self, id: WindowId) -> Result<(), Error> {
        let dialect = self.dialect().await?;
        self.dispatch(&window_command(dialect, Some(id))).await
    }

    async fn step_window(&self, forward: bool) -> Result<(), Error> {
        let dialect = self.dialect().await?;
        let command = match forward {
            true => window_command(dialect, None),
            false => window_prev_command(dialect),
        };
        self.dispatch(&command).await
    }

    async fn switch_keyboard_layout(&self, index: u32) -> Result<(), Error> {
        self.dispatch(&layout_command(&index.to_string())).await
    }

    async fn step_keyboard_layout(&self, forward: bool) -> Result<(), Error> {
        let target = match forward {
            true => "next",
            false => "prev",
        };
        self.dispatch(&layout_command(target)).await
    }

    async fn set_output_enabled(&self, connector: String, enabled: bool) -> Result<(), Error> {
        if self.dialect().await? == Dialect::Lua {
            return self.dispatch(&output_eval(&connector, enabled)).await;
        }
        let mut outputs = self.outputs.clone().lock_owned().await;
        if outputs.is_none() {
            let socket = outputs::socket(&self.dir).await?;
            *outputs = Some(
                tokio::task::spawn_blocking(move || outputs::Manager::new(socket))
                    .await
                    .map_err(outputs::failed)??,
            );
        }
        let monitors = self.json::<Vec<WireMonitor>>("j/monitors all").await?;
        let previous = monitors
            .iter()
            .find(|monitor| monitor.name == connector)
            .map(|monitor| !monitor.disabled)
            .ok_or_else(|| Error::Refused(format!("unknown output {connector}")))?;
        let disabled: Vec<String> = monitors
            .into_iter()
            .filter(|monitor| monitor.disabled)
            .map(|monitor| monitor.name)
            .collect();
        let target = connector.clone();
        let pending_disabled = disabled.clone();
        let (mut outputs, result) = tokio::task::spawn_blocking(move || {
            let result = outputs
                .as_mut()
                .ok_or_else(|| outputs::failed("output manager is unavailable"))
                .and_then(|manager| manager.set_enabled(&target, enabled, &pending_disabled));
            (outputs, result)
        })
        .await
        .map_err(outputs::failed)?;
        let result = match result {
            Ok(()) => self.json::<Vec<WireMonitor>>("j/monitors all").await.and_then(|monitors| {
                if monitors.iter().any(|monitor| monitor.name == connector && monitor.disabled != enabled) {
                    Ok(())
                } else {
                    Err(outputs::failed(format!(
                        "output {connector} did not become {}; another output manager may control it",
                        if enabled { "enabled" } else { "disabled" }
                    )))
                }
            }),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            let restored = self.restore_output(outputs, connector, previous).await;
            return Err(match restored {
                Ok(()) => outputs::failed(format!("{error}; prior output overrides restored")),
                Err(restore) => outputs::failed(format!(
                    "{error}; restoring the prior output state was not acknowledged: {restore}"
                )),
            });
        }
        outputs
            .as_mut()
            .ok_or_else(|| outputs::failed("output manager is unavailable"))?
            .confirm(connector, enabled, disabled);
        Ok(())
    }

    async fn power_off_monitors(&self) -> Result<(), Error> {
        Err(Error::Unsupported("blank the screens".to_owned()))
    }

    async fn stop_screencast(&self, _session: u64) -> Result<(), Error> {
        Err(Error::Unsupported("stop a screencast".to_owned()))
    }

    async fn screenshot_window(&self, _id: WindowId, _path: PathBuf) -> Result<(), Error> {
        Err(Error::Unsupported("screenshot a window".to_owned()))
    }

    async fn window_geometries(&self) -> Result<HashMap<WindowId, Rect>, Error> {
        let (monitors, clients) = tokio::try_join!(
            self.json::<Vec<WireMonitor>>("j/monitors all"),
            self.json::<Vec<WireClient>>("j/clients"),
        )?;
        let origins: HashMap<i64, (f64, f64)> = monitors
            .iter()
            .map(|monitor| (monitor.id, (f64::from(monitor.x), f64::from(monitor.y))))
            .collect();
        Ok(clients
            .into_iter()
            .filter_map(|client| Some((address(&client.address)?, client.rect(&origins)?)))
            .collect())
    }

    async fn probe(&self) -> bool {
        let version = tokio::time::timeout(PROBE_DEADLINE, self.control("version")).await;
        matches!(version, Ok(Ok(_)))
    }
}

struct EventDecoder {
    shared: Arc<Shared>,
    hyprland: Hyprland,
}

fn grace_hold(events: BoxStream<'static, Event>, shared: Arc<Shared>) -> BoxStream<'static, Event> {
    stream::unfold((events, shared), |(mut events, shared)| async move {
        loop {
            let deadline = shared.next_deadline();
            let expiry = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = expiry => {
                    if let Some(casts) = shared.expire() {
                        return Some((Event::CastsChanged(casts), (events, shared)));
                    }
                }
                () = shared.stopped.notified() => {}
                event = events.next() => return Some((event?, (events, shared))),
            }
        }
    })
    .boxed()
}

impl LineDecoder for EventDecoder {
    type Event = Event;

    async fn decode(&mut self, line: &str) -> Vec<Event> {
        if line.starts_with("activelayout>>") {
            let mut events = Vec::new();
            if let Ok(layouts) = self.hyprland.keyboard_layouts().await
                && let Some(index) = layouts.current
            {
                events.push(Event::KeyboardLayoutSwitched(index));
            }
            events
        } else {
            event::decode(&self.shared, line)
        }
    }
}

const CONNECT_ATTEMPTS: u32 = 8;
const CONNECT_BACKOFF: tokio::time::Duration = tokio::time::Duration::from_millis(4);

fn unreachable_socket(path: &Path, error: std::io::Error) -> Error {
    Error::Unavailable(format!("compositor socket {}: {error}", path.display()))
}

async fn connect(path: &Path) -> Result<UnixStream, Error> {
    for attempt in 1..CONNECT_ATTEMPTS {
        match UnixStream::connect(path).await {
            Ok(stream) => return Ok(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(CONNECT_BACKOFF * attempt).await;
            }
            Err(error) => return Err(unreachable_socket(path, error)),
        }
    }

    UnixStream::connect(path)
        .await
        .map_err(|error| unreachable_socket(path, error))
}

fn lua_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

fn output_eval(connector: &str, enabled: bool) -> String {
    format!(
        "eval hl.monitor({{ output = {}, disabled = {} }})",
        lua_string(connector),
        !enabled
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
        let mode = enabled.then(|| {
            Some(Mode {
                width: self.width,
                height: self.height,
                refresh_mhz: mhz(self.refresh_rate)?,
            })
        });

        Output {
            logical: enabled.then(|| Logical {
                x: self.x,
                y: self.y,
                width: scaled(self.width, self.scale),
                height: scaled(self.height, self.scale),
                scale: self.scale,
            }),
            mode: mode.flatten(),
            connector: self.name,
            description: self.description.as_deref().and_then(capped_name_str),
            make: self.make.as_deref().and_then(capped_edid_str),
            model: self.model.as_deref().and_then(capped_edid_str),
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
}

impl WireWorkspace {
    fn into_model(self, active: &[i64]) -> Workspace {
        let name = match self.name == self.id.to_string() {
            true => None,
            false => capped_name_str(&self.name),
        };
        Workspace {
            id: WorkspaceId(self.id as u64),
            index: None,
            name,
            output: self.monitor,
            active: active.contains(&self.id),
        }
    }
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
    workspace: Option<WireActiveWorkspace>,
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
    fn rect(&self, monitor_origins: &HashMap<i64, (f64, f64)>) -> Option<Rect> {
        let &(origin_x, origin_y) = monitor_origins.get(&self.monitor?)?;
        let ((x, y), (width, height)) = (self.at?, self.size?);
        Some(Rect {
            x: x - origin_x,
            y: y - origin_y,
            width: f64::from(width),
            height: f64::from(height),
        })
    }

    fn into_model(self) -> Option<Window> {
        Some(Window {
            id: address(&self.address)?,
            title: capped_title_str(&self.title),
            app_id: capped_app_id_str(&self.class),
            pid: (self.pid > 0).then_some(self.pid),
            workspace: self
                .workspace
                .map(|workspace| WorkspaceId(workspace.id as u64)),
            floating: self.floating,
            urgent: false,
            order: None,
        })
    }
}

fn layout_command(target: &str) -> String {
    format!("switchxkblayout all {target}")
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

fn window_command(dialect: Dialect, id: Option<WindowId>) -> String {
    match (dialect, id) {
        (Dialect::Legacy, None) => "dispatch cyclenext".to_owned(),
        (Dialect::Legacy, Some(id)) => format!("dispatch focuswindow address:0x{:x}", id.0),
        (Dialect::Lua, None) => "dispatch hl.dsp.window.cycle_next()".to_owned(),
        (Dialect::Lua, Some(id)) => format!(
            "dispatch hl.dsp.focus({{ window = \"address:0x{:x}\" }})",
            id.0
        ),
    }
}

fn window_prev_command(dialect: Dialect) -> String {
    match dialect {
        Dialect::Legacy => "dispatch cyclenext prev".to_owned(),
        Dialect::Lua => "dispatch hl.dsp.window.cycle_next({ next = false })".to_owned(),
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
