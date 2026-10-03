mod event;
#[cfg(test)]
mod testing;

use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::stream::{self, BoxStream, StreamExt};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::OwnedFd;
use rustix::fs::inotify;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;

use crate::error::CompositorError;
use crate::event::Event;
use crate::model::{
    LayoutTarget, Logical, Mode, Output, Snapshot, Window, WindowId, WindowTarget, Workspace,
    WorkspaceId, WorkspaceTarget, capped_edid_str, fill_window_outputs, is_built_in,
};
use event::{EventState, WireCast, WireLayouts, into_casts};

const SCREENSHOT_DEADLINE: Duration = Duration::from_secs(2);

pub(crate) const CAPABILITIES: crate::Capabilities = crate::Capabilities {
    floating: false,
    workspace_reorder: true,
    output_power: true,
};

/// Niri answers one request per connection and then closes it, so there is no connection to hold —
/// only the path to open the next one at. `EventStream` is the single exception, and it owns its
/// own connection for as long as the stream lives.
#[derive(Debug, Clone)]
pub struct Niri {
    socket: PathBuf,
}

impl Niri {
    pub fn at(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub(crate) async fn snapshot(&self) -> Result<Snapshot, CompositorError> {
        let (outputs, workspaces, mut windows, keyboard, focused_output, casts) = tokio::try_join!(
            self.fetch::<BTreeMap<String, WireOutput>>("Outputs"),
            self.fetch::<Vec<Workspace>>("Workspaces"),
            self.fetch::<Vec<Window>>("Windows"),
            self.fetch::<WireLayouts>("KeyboardLayouts"),
            self.fetch::<Option<WireFocusedOutput>>("FocusedOutput"),
            self.fetch::<Vec<WireCast>>("Casts"),
        )?;
        fill_window_outputs(&mut windows, &workspaces);

        Ok(Snapshot {
            outputs: outputs.into_values().map(WireOutput::into_model).collect(),
            focused_window: windows
                .iter()
                .find(|window| window.is_focused)
                .map(|window| window.id),
            workspaces,
            windows,
            keyboard: keyboard.into(),
            focused_output: focused_output.map(|output| output.name),
            casts: into_casts(casts),
        })
    }

    pub(crate) async fn events(&self) -> Result<BoxStream<'static, Event>, CompositorError> {
        let mut lines = self.open(&json!("EventStream")).await?;
        // Niri acknowledges the subscription before it sends anything, and a refusal here has to
        // surface as an error rather than as a stream that silently yields nothing.
        let reply = next_line(&mut lines).await?;
        unwrap_reply(&reply)?;

        let start = (lines, EventState::default(), VecDeque::new());
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
        self.act(&layout_action(to)).await
    }

    pub(crate) async fn focus_workspace(&self, to: WorkspaceTarget) -> Result<(), CompositorError> {
        self.act(&workspace_action(to)).await
    }

    pub(crate) async fn rename_workspace(
        &self,
        id: WorkspaceId,
        name: Option<&str>,
    ) -> Result<(), CompositorError> {
        self.act(&rename_action(id, name)).await
    }

    pub(crate) async fn focus_window(&self, to: WindowTarget) -> Result<(), CompositorError> {
        self.act(&window_action(to)).await
    }

    pub(crate) async fn move_workspace_to_output(
        &self,
        id: WorkspaceId,
        connector: &str,
    ) -> Result<(), CompositorError> {
        self.act(&move_to_output_action(id, connector)).await
    }

    pub(crate) async fn reorder_workspace(
        &self,
        id: WorkspaceId,
        index: u8,
    ) -> Result<(), CompositorError> {
        self.act(&reorder_action(id, index)).await
    }

    pub(crate) async fn move_window_to_workspace(
        &self,
        window: WindowId,
        to: &WorkspaceTarget,
    ) -> Result<(), CompositorError> {
        self.act(&move_window_action(window, to)?).await
    }

    pub(crate) async fn close_window(&self, id: WindowId) -> Result<(), CompositorError> {
        self.act(&close_action(id)).await
    }

    pub(crate) async fn focus_output(&self, connector: &str) -> Result<(), CompositorError> {
        self.act(&focus_output_action(connector)).await
    }

    pub(crate) async fn set_output_enabled(
        &self,
        connector: &str,
        on: bool,
    ) -> Result<(), CompositorError> {
        self.request(&output_request(connector, on)).await.map(drop)
    }

    pub(crate) async fn power_off_monitors(&self) -> Result<(), CompositorError> {
        self.act(&power_off_monitors_action()).await
    }

    pub(crate) async fn stop_screencast(&self, session_id: u64) -> Result<(), CompositorError> {
        self.act(&stop_cast_action(session_id)).await
    }

    pub(crate) async fn screenshot_window(
        &self,
        id: WindowId,
        path: &Path,
    ) -> Result<(), CompositorError> {
        if !path.is_absolute() {
            return Err(CompositorError::InvalidPath(path.to_owned()));
        }
        let path_str = path
            .to_str()
            .ok_or_else(|| CompositorError::InvalidPath(path.to_owned()))?
            .to_owned();
        let file_name = path
            .file_name()
            .ok_or_else(|| CompositorError::InvalidPath(path.to_owned()))?
            .to_owned();
        let dir = path
            .parent()
            .ok_or_else(|| CompositorError::InvalidPath(path.to_owned()))?
            .to_owned();

        let watch = arm_close_write_watch(&dir)?;

        self.act(&screenshot_action(id, &path_str)).await?;

        tokio::task::spawn_blocking(move || {
            wait_for_close_write(watch, &file_name, SCREENSHOT_DEADLINE)
        })
        .await
        .map_err(CompositorError::protocol)?
    }

    async fn fetch<T: for<'de> Deserialize<'de>>(
        &self,
        name: &'static str,
    ) -> Result<T, CompositorError> {
        let reply = self.request(&json!(name)).await?;
        let payload = reply
            .get(name)
            .ok_or_else(|| CompositorError::Protocol(format!("reply carried no `{name}`")))?;
        T::deserialize(payload).map_err(CompositorError::protocol)
    }

    async fn act(&self, action: &Value) -> Result<(), CompositorError> {
        self.request(&json!({ "Action": action })).await.map(drop)
    }

    async fn request(&self, request: &Value) -> Result<Value, CompositorError> {
        let mut lines = self.open(request).await?;
        let reply = next_line(&mut lines).await?;
        unwrap_reply(&reply)
    }

    async fn open(&self, request: &Value) -> Result<Lines<BufReader<UnixStream>>, CompositorError> {
        let mut stream = UnixStream::connect(&self.socket)
            .await
            .map_err(|error| CompositorError::connect(&self.socket, error))?;

        let mut line = serde_json::to_vec(request).map_err(CompositorError::protocol)?;
        line.push(b'\n');
        stream
            .write_all(&line)
            .await
            .map_err(|error| CompositorError::connect(&self.socket, error))?;
        stream.flush().await.map_err(CompositorError::protocol)?;

        Ok(BufReader::new(stream).lines())
    }
}

pub(crate) fn from_env() -> Option<Niri> {
    let socket = std::env::var_os("NIRI_SOCKET")?;
    match socket.is_empty() {
        true => None,
        false => Some(Niri::at(PathBuf::from(socket))),
    }
}

async fn next_line(lines: &mut Lines<BufReader<UnixStream>>) -> Result<String, CompositorError> {
    lines
        .next_line()
        .await
        .map_err(CompositorError::protocol)?
        .ok_or(CompositorError::Closed)
}

fn unwrap_reply(line: &str) -> Result<Value, CompositorError> {
    match serde_json::from_str::<Reply>(line) {
        Ok(Reply::Ok(payload)) => Ok(payload),
        Ok(Reply::Err(reason)) => Err(CompositorError::Refused(reason)),
        Err(error) => Err(CompositorError::protocol(error)),
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
struct WireOutput {
    name: String,
    #[serde(default)]
    make: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial: Option<String>,
    #[serde(default)]
    modes: Vec<WireMode>,
    /// An index into `modes`, and `null` for an output that is switched off — niri has no
    /// `enabled` field, so this is the only thing that says so.
    #[serde(default)]
    current_mode: Option<usize>,
    #[serde(default)]
    logical: Option<Logical>,
}

impl WireOutput {
    fn into_model(self) -> Output {
        let current_mode = self
            .current_mode
            .and_then(|index| self.modes.get(index))
            .map(|mode| Mode {
                width: mode.width,
                height: mode.height,
                refresh_mhz: mode.refresh_rate,
            });

        Output {
            built_in: is_built_in(&self.name),
            connector: self.name,
            make: self.make.as_deref().and_then(capped_edid_str),
            model: self.model.as_deref().and_then(capped_edid_str),
            serial: self.serial.as_deref().and_then(capped_edid_str),
            description: None,
            logical: self.logical,
            enabled: current_mode.is_some(),
            current_mode,
        }
    }
}

fn layout_action(to: LayoutTarget) -> Value {
    let layout = match to {
        LayoutTarget::Next => json!("Next"),
        LayoutTarget::Prev => json!("Prev"),
        LayoutTarget::Index(index) => json!({ "Index": index }),
    };
    json!({ "SwitchLayout": { "layout": layout } })
}

fn workspace_action(to: WorkspaceTarget) -> Value {
    // `Next`/`Prev` are separate actions, not a reference: niri's directional focus follows the
    // monitor layout, which an index cannot express.
    let reference = match to {
        WorkspaceTarget::Next => return json!({ "FocusWorkspaceDown": {} }),
        WorkspaceTarget::Prev => return json!({ "FocusWorkspaceUp": {} }),
        WorkspaceTarget::Id(id) => json!({ "Id": id.0 }),
        WorkspaceTarget::Index(index) => json!({ "Index": index }),
        WorkspaceTarget::Name(name) => json!({ "Name": name }),
    };
    json!({ "FocusWorkspace": { "reference": reference } })
}

fn window_action(to: WindowTarget) -> Value {
    match to {
        WindowTarget::Next => json!({ "FocusWindowDownOrColumnRight": {} }),
        WindowTarget::Prev => json!({ "FocusWindowUpOrColumnLeft": {} }),
        WindowTarget::Id(id) => json!({ "FocusWindow": { "id": id.0 } }),
    }
}

fn rename_action(id: WorkspaceId, name: Option<&str>) -> Value {
    let reference = json!({ "Id": id.0 });
    match name {
        Some(name) => json!({ "SetWorkspaceName": { "name": name, "workspace": reference } }),
        None => json!({ "UnsetWorkspaceName": { "reference": reference } }),
    }
}

fn workspace_reference(to: &WorkspaceTarget) -> Result<Value, CompositorError> {
    match to {
        WorkspaceTarget::Id(id) => Ok(json!({ "Id": id.0 })),
        WorkspaceTarget::Index(index) => Ok(json!({ "Index": index })),
        WorkspaceTarget::Name(name) => Ok(json!({ "Name": name })),
        WorkspaceTarget::Next | WorkspaceTarget::Prev => Err(CompositorError::Unavailable(
            "move a window to a workspace named only as next or previous",
        )),
    }
}

fn move_to_output_action(id: WorkspaceId, connector: &str) -> Value {
    json!({ "MoveWorkspaceToMonitor": { "output": connector, "reference": { "Id": id.0 } } })
}

fn reorder_action(id: WorkspaceId, index: u8) -> Value {
    json!({ "MoveWorkspaceToIndex": { "index": index, "reference": { "Id": id.0 } } })
}

fn move_window_action(window: WindowId, to: &WorkspaceTarget) -> Result<Value, CompositorError> {
    let reference = workspace_reference(to)?;
    Ok(json!({
        "MoveWindowToWorkspace": { "reference": reference, "window_id": window.0, "focus": false }
    }))
}

fn close_action(id: WindowId) -> Value {
    json!({ "CloseWindow": { "id": id.0 } })
}

fn focus_output_action(connector: &str) -> Value {
    json!({ "FocusMonitor": { "output": connector } })
}

fn output_request(connector: &str, on: bool) -> Value {
    let action = match on {
        true => "On",
        false => "Off",
    };
    json!({ "Output": { "output": connector, "action": action } })
}

fn power_off_monitors_action() -> Value {
    json!({ "PowerOffMonitors": {} })
}

fn stop_cast_action(session_id: u64) -> Value {
    json!({ "StopCast": { "session_id": session_id } })
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

fn arm_close_write_watch(dir: &Path) -> Result<OwnedFd, CompositorError> {
    let flags = inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK;
    let watch = inotify::init(flags).map_err(|error| CompositorError::Watch {
        path: dir.to_owned(),
        error: error.into(),
    })?;
    inotify::add_watch(
        &watch,
        dir,
        inotify::WatchFlags::CLOSE_WRITE | inotify::WatchFlags::MOVED_TO,
    )
    .map_err(|error| CompositorError::Watch {
        path: dir.to_owned(),
        error: error.into(),
    })?;
    Ok(watch)
}

fn wait_for_close_write(
    watch: OwnedFd,
    file_name: &OsStr,
    deadline: Duration,
) -> Result<(), CompositorError> {
    use std::os::unix::ffi::OsStrExt;

    let started = Instant::now();
    let mut buffer = [std::mem::MaybeUninit::uninit(); 512];
    loop {
        let remaining = deadline
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(CompositorError::NoFile(deadline))?;
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        let mut fds = [PollFd::new(&watch, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err(CompositorError::NoFile(deadline)),
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(CompositorError::protocol(error)),
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
                Err(error) => return Err(CompositorError::protocol(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests;
