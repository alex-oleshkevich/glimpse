mod event;
#[cfg(test)]
mod testing;

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::stream::BoxStream;
use glimpse_compositors::backend::Backend;
use glimpse_compositors::contract::{Capability, Error};
use glimpse_compositors::event::{Event, line_events};
use glimpse_compositors::model::{
    Focus, KeyboardLayouts, Logical, Mode, Output, Rect, Snapshot, WindowId, WorkspaceId,
};
use glimpse_compositors::sanitize::{
    NAME_CAP, TITLE_CAP, capped_edid_str, capped_name_str, sanitize,
};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::OwnedFd;
use rustix::fs::inotify;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;

use event::{
    EventState, WireCast, WireLayouts, WireWindow, WireWorkspace, into_casts, into_workspaces,
};

const SCREENSHOT_DEADLINE: Duration = Duration::from_secs(2);
const PROBE_DEADLINE: Duration = Duration::from_secs(2);

pub struct Niri {
    socket: PathBuf,
}

impl Niri {
    pub fn at(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub fn from_env() -> Option<Self> {
        let socket = std::env::var_os("NIRI_SOCKET").filter(|socket| !socket.is_empty())?;
        Some(Self::at(PathBuf::from(socket)))
    }

    async fn fetch<T: for<'de> Deserialize<'de>>(&self, name: &'static str) -> Result<T, Error> {
        let reply = self.request(&json!(name)).await?;
        let payload = reply
            .get(name)
            .ok_or_else(|| Error::Failed(format!("reply carried no `{name}`")))?;
        T::deserialize(payload).map_err(failed)
    }

    async fn act(&self, action: &Value) -> Result<(), Error> {
        self.request(&json!({ "Action": action })).await.map(drop)
    }

    async fn request(&self, request: &Value) -> Result<Value, Error> {
        let mut lines = self.open(request).await?;
        let reply = next_line(&mut lines).await?;
        unwrap_reply(&reply)
    }

    async fn open(&self, request: &Value) -> Result<Lines<BufReader<UnixStream>>, Error> {
        let unreachable = |error: std::io::Error| {
            Error::Unavailable(format!("niri socket {}: {error}", self.socket.display()))
        };
        let mut stream = UnixStream::connect(&self.socket)
            .await
            .map_err(unreachable)?;

        let mut line = serde_json::to_vec(request).map_err(failed)?;
        line.push(b'\n');
        stream.write_all(&line).await.map_err(unreachable)?;

        Ok(BufReader::new(stream).lines())
    }
}

impl Backend for Niri {
    const NAME: &'static str = "niri";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::RenameWorkspace,
        Capability::OutputPower,
        Capability::BlankScreens,
        Capability::WindowScreenshot,
        Capability::WindowGeometry,
        Capability::OverviewBackdrop,
    ];

    async fn snapshot(&self) -> Result<Snapshot, Error> {
        let (outputs, workspaces, windows, keyboard, focused_output, casts) = tokio::try_join!(
            self.fetch::<BTreeMap<String, WireOutput>>("Outputs"),
            self.fetch::<Vec<WireWorkspace>>("Workspaces"),
            self.fetch::<Vec<WireWindow>>("Windows"),
            self.fetch::<WireLayouts>("KeyboardLayouts"),
            self.fetch::<Option<WireFocusedOutput>>("FocusedOutput"),
            self.fetch::<Vec<WireCast>>("Casts"),
        )?;

        Ok(Snapshot {
            outputs: outputs.into_values().map(WireOutput::into_model).collect(),
            workspaces: into_workspaces(workspaces).0,
            focus: Focus {
                output: focused_output.and_then(|output| capped_name_str(&output.name)),
                window: windows
                    .iter()
                    .find(|window| window.is_focused())
                    .map(WireWindow::id),
            },
            windows: windows.into_iter().map(WireWindow::into_model).collect(),
            keyboard: keyboard.into(),
            casts: into_casts(casts),
        })
    }

    async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, Error> {
        Ok(self.fetch::<WireLayouts>("KeyboardLayouts").await?.into())
    }

    async fn session_lock_restore(&self) -> Result<Option<bool>, Error> {
        Ok(None)
    }

    async fn events(&self) -> Result<BoxStream<'static, Event>, Error> {
        let mut lines = self.open(&json!("EventStream")).await?;
        let reply = next_line(&mut lines).await?;
        unwrap_reply(&reply)?;

        Ok(line_events(lines.into_inner(), EventState::default()))
    }

    async fn focus_workspace(&self, id: WorkspaceId) -> Result<(), Error> {
        self.act(&json!({ "FocusWorkspace": { "reference": { "Id": id.0 } } }))
            .await
    }

    async fn step_workspace(&self, forward: bool) -> Result<(), Error> {
        self.act(&step_workspace_action(forward)).await
    }

    async fn rename_workspace(&self, id: WorkspaceId, name: String) -> Result<(), Error> {
        self.act(&rename_action(id, &name)).await
    }

    async fn focus_window(&self, id: WindowId) -> Result<(), Error> {
        self.act(&json!({ "FocusWindow": { "id": id.0 } })).await
    }

    async fn step_window(&self, forward: bool) -> Result<(), Error> {
        self.act(&step_window_action(forward)).await
    }

    async fn switch_keyboard_layout(&self, index: u32) -> Result<(), Error> {
        self.act(&layout_action(json!({ "Index": index }))).await
    }

    async fn step_keyboard_layout(&self, forward: bool) -> Result<(), Error> {
        let layout = match forward {
            true => json!("Next"),
            false => json!("Prev"),
        };
        self.act(&layout_action(layout)).await
    }

    async fn set_output_enabled(&self, connector: String, enabled: bool) -> Result<(), Error> {
        let reply = self.request(&output_request(&connector, enabled)).await?;
        match reply.get("OutputConfigChanged").and_then(Value::as_str) {
            Some("OutputWasMissing") => Err(Error::Refused(format!("unknown output {connector}"))),
            _ => Ok(()),
        }
    }

    async fn power_off_monitors(&self) -> Result<(), Error> {
        self.act(&json!({ "PowerOffMonitors": {} })).await
    }

    async fn stop_screencast(&self, session: u64) -> Result<(), Error> {
        self.act(&json!({ "StopCast": { "session_id": session } }))
            .await
    }

    async fn screenshot_window(&self, id: WindowId, path: PathBuf) -> Result<(), Error> {
        let refused = || Error::Refused(format!("not a UTF-8 file path: {path:?}"));
        let path_str = path.to_str().ok_or_else(refused)?.to_owned();
        let file_name = path.file_name().ok_or_else(refused)?.to_owned();
        let dir = path.parent().ok_or_else(refused)?.to_owned();

        let watch = arm_close_write_watch(&dir)?;

        self.act(&screenshot_action(id, &path_str)).await?;

        tokio::task::spawn_blocking(move || {
            wait_for_close_write(watch, &file_name, SCREENSHOT_DEADLINE)
        })
        .await
        .map_err(failed)?
    }

    async fn window_geometries(&self) -> Result<HashMap<WindowId, Rect>, Error> {
        let windows = self.fetch::<Vec<WireWindow>>("Windows").await?;
        Ok(windows
            .iter()
            .filter_map(|window| Some((window.id(), window.rect()?)))
            .collect())
    }

    async fn probe(&self) -> bool {
        let version = tokio::time::timeout(PROBE_DEADLINE, self.fetch::<String>("Version")).await;
        matches!(version, Ok(Ok(_)))
    }
}

fn failed(context: impl std::fmt::Display) -> Error {
    Error::Failed(context.to_string())
}

async fn next_line(lines: &mut Lines<BufReader<UnixStream>>) -> Result<String, Error> {
    lines
        .next_line()
        .await
        .map_err(failed)?
        .ok_or_else(|| Error::Unavailable("niri closed the connection before replying".to_owned()))
}

fn unwrap_reply(line: &str) -> Result<Value, Error> {
    match serde_json::from_str::<Reply>(line) {
        Ok(Reply::Ok(payload)) => Ok(payload),
        Ok(Reply::Err(reason)) => Err(Error::Refused(sanitize(&reason, TITLE_CAP))),
        Err(error) => Err(failed(error)),
    }
}

#[derive(Deserialize)]
enum Reply {
    Ok(Value),
    Err(String),
}

#[derive(Deserialize)]
struct WireFocusedOutput {
    name: String,
}

#[derive(Deserialize)]
struct WireMode {
    width: u32,
    height: u32,
    refresh_rate: u32,
}

#[derive(Deserialize)]
struct WireLogical {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}

#[derive(Deserialize)]
struct WireOutput {
    name: String,
    #[serde(default)]
    make: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    modes: Vec<WireMode>,
    #[serde(default)]
    current_mode: Option<usize>,
    #[serde(default)]
    logical: Option<WireLogical>,
}

impl WireOutput {
    fn into_model(self) -> Output {
        let mode = self
            .current_mode
            .and_then(|index| self.modes.get(index))
            .map(|mode| Mode {
                width: mode.width,
                height: mode.height,
                refresh_mhz: mode.refresh_rate,
            });

        Output {
            connector: sanitize(&self.name, NAME_CAP),
            enabled: mode.is_some(),
            description: None,
            make: self.make.as_deref().and_then(capped_edid_str),
            model: self.model.as_deref().and_then(capped_edid_str),
            mode,
            logical: self.logical.map(|logical| Logical {
                x: logical.x,
                y: logical.y,
                width: logical.width,
                height: logical.height,
                scale: logical.scale,
            }),
        }
    }
}

fn layout_action(layout: Value) -> Value {
    json!({ "SwitchLayout": { "layout": layout } })
}

fn step_workspace_action(forward: bool) -> Value {
    match forward {
        true => json!({ "FocusWorkspaceDown": {} }),
        false => json!({ "FocusWorkspaceUp": {} }),
    }
}

fn step_window_action(forward: bool) -> Value {
    match forward {
        true => json!({ "FocusWindowDownOrColumnRight": {} }),
        false => json!({ "FocusWindowUpOrColumnLeft": {} }),
    }
}

fn rename_action(id: WorkspaceId, name: &str) -> Value {
    let reference = json!({ "Id": id.0 });
    match name.is_empty() {
        false => json!({ "SetWorkspaceName": { "name": name, "workspace": reference } }),
        true => json!({ "UnsetWorkspaceName": { "reference": reference } }),
    }
}

fn output_request(connector: &str, on: bool) -> Value {
    let action = match on {
        true => "On",
        false => "Off",
    };
    json!({ "Output": { "output": connector, "action": action } })
}

fn screenshot_action(id: WindowId, path: &str) -> Value {
    json!({
        "ScreenshotWindow": {
            "id": id.0,
            "write_to_disk": true,
            "show_pointer": false,
            "path": path,
        }
    })
}

fn arm_close_write_watch(dir: &Path) -> Result<OwnedFd, Error> {
    let watching =
        |error: rustix::io::Errno| Error::Failed(format!("watching {}: {error}", dir.display()));
    let flags = inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK;
    let watch = inotify::init(flags).map_err(watching)?;
    inotify::add_watch(
        &watch,
        dir,
        inotify::WatchFlags::CLOSE_WRITE | inotify::WatchFlags::MOVED_TO,
    )
    .map_err(watching)?;
    Ok(watch)
}

fn wait_for_close_write(
    watch: OwnedFd,
    file_name: &OsStr,
    deadline: Duration,
) -> Result<(), Error> {
    use std::os::unix::ffi::OsStrExt;

    let no_file = || Error::Failed(format!("niri wrote no file within {deadline:?}"));
    let started = Instant::now();
    let mut buffer = [std::mem::MaybeUninit::uninit(); 512];
    loop {
        let remaining = deadline
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(no_file)?;
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        let mut fds = [PollFd::new(&watch, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err(no_file()),
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(failed(error)),
        }

        let mut reader = inotify::Reader::new(&watch, &mut buffer);
        loop {
            match reader.next() {
                Ok(event)
                    if event.file_name().map(|name| name.to_bytes())
                        == Some(file_name.as_bytes()) =>
                {
                    return Ok(());
                }
                Ok(_) => {}
                Err(rustix::io::Errno::WOULDBLOCK) => break,
                Err(error) => return Err(failed(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests;
