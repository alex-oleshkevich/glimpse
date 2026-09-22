use futures_util::{StreamExt, stream};
use glimpse_compositors::{
    Capabilities, Cast, CastKind, CastTarget, Compositor as Backend, CompositorError,
    Event as Change, Output, Resync, Snapshot, WindowId, WindowTarget, Workspace, WorkspaceId,
    WorkspaceTarget, detect_compositor,
};
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub index: Option<u8>,
    pub name: Option<String>,
    pub output: Option<String>,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
    pub windows: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub workspace: Option<u64>,
    pub focused: bool,
    pub floating: bool,
    pub urgent: bool,
    pub order: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputMode {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputLogical {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub connector: String,
    pub label: Option<String>,
    pub built_in: bool,
    pub focused: bool,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub current_mode: Option<OutputMode>,
    pub logical: Option<OutputLogical>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositorCapabilities {
    pub floating: bool,
    pub workspace_reorder: bool,
    pub output_power: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum WorkspaceRef {
    Id { id: u64 },
    Index { index: u8 },
    Name { name: String },
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum WindowRef {
    Id { id: u64 },
    Pid { pid: i32 },
    Next,
    Prev,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositorStatus {
    pub name: String,
    pub capabilities: CompositorCapabilities,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositorWorkspaces {
    pub workspaces: Vec<WorkspaceInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositorWindows {
    pub windows: Vec<WindowInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositorOutputs {
    pub outputs: Vec<OutputInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastKindInfo {
    PipeWire,
    Screencopy,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastTargetInfo {
    Output(String),
    Window(u64),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastInfo {
    pub stream_id: u64,
    pub session_id: Option<u64>,
    pub kind: CastKindInfo,
    pub target: CastTargetInfo,
    pub pw_node_id: Option<u32>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositorPrivacy {
    pub active: bool,
    pub casts: Vec<CastInfo>,
}

pub enum Event {
    Snapshot(Box<Snapshot>),
    Changed(Change),
    Failed(String),
}

#[derive(Debug)]
pub enum Command {
    FocusWorkspace {
        target: WorkspaceRef,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    FocusWindow {
        target: WindowRef,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    FocusOutput {
        connector: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    RenameWorkspace {
        id: u64,
        name: Option<String>,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    MoveWorkspaceToOutput {
        id: u64,
        connector: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    ReorderWorkspace {
        id: u64,
        index: u8,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    MoveWindowToWorkspace {
        window: u64,
        workspace: WorkspaceRef,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    CloseWindow {
        id: u64,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    SetOutputEnabled {
        connector: String,
        enabled: bool,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    PowerOffMonitors {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    StopScreencast {
        session_id: u64,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(Clone)]
pub struct CompositorHandle(ServiceEndpoint<Compositor>);

impl CompositorHandle {
    pub fn snapshot(&self) -> CompositorState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<CompositorState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn focus_workspace(&self, target: WorkspaceRef) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::FocusWorkspace { target, reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn focus_window(&self, target: WindowRef) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::FocusWindow { target, reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn focus_output(&self, connector: String) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::FocusOutput { connector, reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn rename_workspace(
        &self,
        id: u64,
        name: Option<String>,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0
            .command(Command::RenameWorkspace { id, name, reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn move_workspace_to_output(
        &self,
        id: u64,
        connector: String,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::MoveWorkspaceToOutput {
            id,
            connector,
            reply,
        })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn reorder_workspace(&self, id: u64, index: u8) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0
            .command(Command::ReorderWorkspace { id, index, reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn move_window_to_workspace(
        &self,
        window: u64,
        workspace: WorkspaceRef,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::MoveWindowToWorkspace {
            window,
            workspace,
            reply,
        })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn close_window(&self, id: u64) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::CloseWindow { id, reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn set_output_enabled(
        &self,
        connector: String,
        enabled: bool,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::SetOutputEnabled {
            connector,
            enabled,
            reply,
        })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn power_off_monitors(&self) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::PowerOffMonitors { reply })?;
        result.await.map_err(|_| stopped())?
    }

    pub async fn stop_screencast(&self, session_id: u64) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0
            .command(Command::StopScreencast { session_id, reply })?;
        result.await.map_err(|_| stopped())?
    }
}

fn stopped() -> CommandError {
    CommandError::Unavailable("compositor service stopped".to_owned())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompositorState {
    pub status: Option<CompositorStatus>,
    pub workspaces: Option<CompositorWorkspaces>,
    pub windows: Option<CompositorWindows>,
    pub outputs: Option<CompositorOutputs>,
    pub privacy: Option<CompositorPrivacy>,
}

pub struct Compositor {
    backend: Backend,
    state_publisher: Publisher<CompositorState>,
    state: Option<Snapshot>,
    attempt: u64,
    capabilities: CompositorCapabilities,
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Events,
    Fetch { attempt: u64 },
}

impl Service for Compositor {
    const NAME: &'static str = "compositor";

    type Config = NoConfig;
    type State = CompositorState;
    type Handle = CompositorHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        CompositorHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let follow = self.backend.clone();
        let read = self.backend.clone();

        vec![
            Sub::stream(Watch::Events, move |_ctx| async move {
                match follow.events().await {
                    Ok(events) => events
                        .map(Event::Changed)
                        .chain(stream::once(async {
                            Event::Failed("the compositor closed its event stream".to_owned())
                        }))
                        .boxed(),
                    Err(error) => {
                        stream::once(async move { Event::Failed(error.to_string()) }).boxed()
                    }
                }
            }),
            Sub::stream(
                Watch::Fetch {
                    attempt: self.attempt,
                },
                move |_ctx| async move {
                    stream::once(async move {
                        match read.snapshot().await {
                            Ok(snapshot) => Event::Snapshot(Box::new(snapshot)),
                            Err(error) => Event::Failed(error.to_string()),
                        }
                    })
                },
            ),
        ]
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        let backend = detect_compositor();
        tracing::debug!(compositor = backend.name(), "starting compositor service");

        Ok(Self::with_backend(ctx, backend))
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Snapshot(snapshot)) => {
                self.state = Some(*snapshot);
                if ctx.is_degraded() {
                    ctx.running();
                }
                self.publish();
            }
            Input::Event(Event::Changed(change)) => {
                let Some(state) = self.state.as_mut() else {
                    return;
                };
                if apply(state, change) {
                    self.attempt += 1;
                }
                self.publish();
            }
            Input::Event(Event::Failed(reason)) => ctx.degraded(reason),
            Input::Command(command) => self.dispatch(command).await,
            Input::Config(NoConfig) => {}
        }
    }
}

impl Compositor {
    fn with_backend(ctx: &Ctx<Self>, backend: Backend) -> Self {
        let capabilities = capabilities(backend.capabilities());
        let service = Self {
            state_publisher: ctx.publisher(),
            state: None,
            attempt: 0,
            capabilities,
            backend,
        };

        service.state_publisher.update(|state| {
            state.status = Some(CompositorStatus {
                name: service.backend.name().to_owned(),
                capabilities: service.capabilities,
            });
        });

        service
    }

    fn publish(&mut self) {
        let Some(state) = self.state.as_ref() else {
            return;
        };

        let workspaces = workspaces_of(state);
        let windows = windows_of(state);
        let outputs = outputs_of(state);
        let casts = casts_of(state);

        self.state_publisher.update(|published| {
            published.workspaces = Some(CompositorWorkspaces { workspaces });
            published.windows = Some(CompositorWindows { windows });
            published.outputs = Some(CompositorOutputs { outputs });
            published.privacy = Some(CompositorPrivacy {
                active: !state.casts.is_empty(),
                casts,
            });
        });
    }

    async fn dispatch(&self, command: Command) {
        let (outcome, reply) = match command {
            Command::FocusWorkspace { target, reply } => (
                self.backend.focus_workspace(workspace_target(target)).await,
                reply,
            ),
            Command::FocusWindow { target, reply } => (
                match window_target(target, self.state.as_ref()) {
                    Some(target) => self.backend.focus_window(target).await,
                    None => Err(CompositorError::Refused(
                        "no window matches the requested reference".to_owned(),
                    )),
                },
                reply,
            ),
            Command::FocusOutput { connector, reply } => {
                (self.backend.focus_output(&connector).await, reply)
            }
            Command::RenameWorkspace { id, name, reply } => (
                self.backend
                    .rename_workspace(WorkspaceId(id), name.as_deref())
                    .await,
                reply,
            ),
            Command::MoveWorkspaceToOutput {
                id,
                connector,
                reply,
            } => (
                self.backend
                    .move_workspace_to_output(WorkspaceId(id), &connector)
                    .await,
                reply,
            ),
            Command::ReorderWorkspace { id, index, reply } => (
                self.backend.reorder_workspace(WorkspaceId(id), index).await,
                reply,
            ),
            Command::MoveWindowToWorkspace {
                window,
                workspace,
                reply,
            } => (
                self.backend
                    .move_window_to_workspace(WindowId(window), workspace_target(workspace))
                    .await,
                reply,
            ),
            Command::CloseWindow { id, reply } => {
                (self.backend.close_window(WindowId(id)).await, reply)
            }
            Command::SetOutputEnabled {
                connector,
                enabled,
                reply,
            } => {
                let outcome = if !self.capabilities.output_power {
                    Err(CompositorError::Unsupported(
                        "this compositor cannot turn displays on or off",
                    ))
                } else {
                    match self.state.as_ref() {
                        None => Err(CompositorError::Refused(
                            "the compositor state has not arrived yet".to_owned(),
                        )),
                        Some(state) => match state
                            .outputs
                            .iter()
                            .find(|output| output.connector == connector)
                        {
                            None => Err(CompositorError::Refused(format!(
                                "unknown output {connector}"
                            ))),
                            Some(output)
                                if !enabled
                                    && output.enabled
                                    && enabled_output_count(state) <= 1 =>
                            {
                                Err(CompositorError::Refused(
                                    "cannot turn off the last display that is still on".to_owned(),
                                ))
                            }
                            Some(_) => self.backend.set_output_enabled(&connector, enabled).await,
                        },
                    }
                };
                (outcome, reply)
            }
            Command::PowerOffMonitors { reply } => {
                let outcome = if !self.capabilities.output_power {
                    Err(CompositorError::Unsupported(
                        "this compositor cannot blank the displays",
                    ))
                } else {
                    self.backend.power_off_monitors().await
                };
                (outcome, reply)
            }
            Command::StopScreencast { session_id, reply } => {
                (self.backend.stop_screencast(session_id).await, reply)
            }
        };

        let outcome = outcome.map_err(|error| command_error(&error));
        let _ = reply.send(outcome);
    }
}

fn enabled_output_count(state: &Snapshot) -> usize {
    state.outputs.iter().filter(|output| output.enabled).count()
}

fn apply(state: &mut Snapshot, change: Change) -> bool {
    match change {
        Change::WorkspacesChanged(workspaces) => state.workspaces = workspaces,
        Change::WorkspaceActivated { id, focused } => {
            let Some(activated) = state.workspaces.iter().find(|workspace| workspace.id == id)
            else {
                return true;
            };
            let output = activated.output.clone();
            if focused {
                state.focused_output = output.clone();
            }
            for workspace in &mut state.workspaces {
                if workspace.output == output {
                    workspace.is_active = workspace.id == id;
                }
                if focused {
                    workspace.is_focused = workspace.id == id;
                }
            }
        }
        Change::WorkspaceActiveWindowChanged { workspace, window } => {
            if let Some(found) = workspace_mut(state, workspace) {
                found.active_window_id = window;
            }
        }
        Change::WorkspaceUrgencyChanged { id, urgent } => {
            if let Some(found) = workspace_mut(state, id) {
                found.is_urgent = urgent;
            }
        }
        Change::WindowsChanged(windows) => state.windows = windows,
        Change::WindowOpenedOrChanged(window) => {
            let (id, focused) = (window.id, window.is_focused);
            match state.windows.iter_mut().find(|it| it.id == id) {
                Some(found) => *found = window,
                None => state.windows.push(window),
            }
            if focused {
                state.focused_window = Some(id);
                for other in &mut state.windows {
                    other.is_focused = other.id == id;
                }
            }
        }
        Change::WindowClosed(id) => state.windows.retain(|window| window.id != id),
        Change::WindowFocusChanged(id) => {
            state.focused_window = id;
            for window in &mut state.windows {
                window.is_focused = Some(window.id) == id;
                if window.is_focused {
                    window.is_urgent = false;
                }
            }
        }
        Change::WindowUrgencyChanged { id, urgent } => {
            if let Some(found) = state.windows.iter_mut().find(|it| it.id == id) {
                found.is_urgent = urgent;
            }
        }
        Change::WindowLayoutsChanged(orders) => {
            for (id, order) in orders {
                if let Some(found) = state.windows.iter_mut().find(|it| it.id == id) {
                    found.layout_order = order;
                }
            }
        }
        Change::KeyboardLayoutsChanged(_)
        | Change::KeyboardLayoutSwitched { .. }
        | Change::Resync(Resync::Keyboard) => {}
        Change::CastsChanged(casts) => state.casts = casts,
        Change::CastStartedOrChanged { cast } => {
            match state
                .casts
                .iter_mut()
                .find(|existing| existing.stream_id == cast.stream_id)
            {
                Some(existing) => *existing = cast,
                None => state.casts.push(cast),
            }
        }
        Change::CastStopped(id) => {
            state.casts.retain(|cast| cast.stream_id != id);
        }
        Change::Resync(Resync::Structure | Resync::Outputs) => return true,
    }
    false
}

fn workspace_mut(state: &mut Snapshot, id: WorkspaceId) -> Option<&mut Workspace> {
    state
        .workspaces
        .iter_mut()
        .find(|workspace| workspace.id == id)
}

fn workspaces_of(state: &Snapshot) -> Vec<WorkspaceInfo> {
    let mut workspaces: Vec<WorkspaceInfo> = state
        .workspaces
        .iter()
        .map(|workspace| WorkspaceInfo {
            id: workspace.id.0,
            index: workspace.idx,
            name: workspace.name.clone(),
            output: workspace.output.clone(),
            active: workspace.is_active,
            focused: workspace.is_focused,
            urgent: workspace.is_urgent
                || state
                    .windows
                    .iter()
                    .any(|window| window.is_urgent && window.workspace_id == Some(workspace.id)),
            windows: state
                .windows
                .iter()
                .filter(|window| window.workspace_id == Some(workspace.id))
                .count() as u32,
        })
        .collect();

    workspaces.sort_by(|left, right| {
        left.output
            .cmp(&right.output)
            .then(position(left).cmp(&position(right)))
    });
    workspaces
}

fn windows_of(state: &Snapshot) -> Vec<WindowInfo> {
    let mut windows: Vec<WindowInfo> = state
        .windows
        .iter()
        .map(|window| WindowInfo {
            id: window.id.0,
            title: window.title.clone(),
            app_id: window.app_id.clone(),
            workspace: window.workspace_id.map(|id| id.0),
            focused: window.is_focused,
            floating: window.is_floating,
            urgent: window.is_urgent,
            order: window.layout_order,
        })
        .collect();

    windows.sort_by_key(|window| (window.workspace, window.order, window.id));
    windows
}

fn outputs_of(state: &Snapshot) -> Vec<OutputInfo> {
    state
        .outputs
        .iter()
        .map(|output| OutputInfo {
            connector: output.connector.clone(),
            label: label_of(output),
            built_in: output.built_in,
            focused: state.focused_output.as_ref() == Some(&output.connector),
            make: output.make.clone(),
            model: output.model.clone(),
            serial: output.serial.clone(),
            current_mode: output.current_mode.map(|mode| OutputMode {
                width: mode.width,
                height: mode.height,
                refresh_mhz: mode.refresh_mhz,
            }),
            logical: output.logical.as_ref().map(|logical| OutputLogical {
                x: logical.x,
                y: logical.y,
                width: logical.width,
                height: logical.height,
                scale: logical.scale,
            }),
            enabled: output.enabled,
        })
        .collect()
}

fn casts_of(state: &Snapshot) -> Vec<CastInfo> {
    state.casts.iter().map(cast_info).collect()
}

fn cast_info(cast: &Cast) -> CastInfo {
    CastInfo {
        stream_id: cast.stream_id,
        session_id: cast.session_id,
        kind: match cast.kind {
            CastKind::PipeWire => CastKindInfo::PipeWire,
            CastKind::Screencopy => CastKindInfo::Screencopy,
            CastKind::Unknown => CastKindInfo::Unknown,
        },
        target: match &cast.target {
            CastTarget::Output(name) => CastTargetInfo::Output(name.clone()),
            CastTarget::Window(id) => CastTargetInfo::Window(id.0),
            CastTarget::Unknown => CastTargetInfo::Unknown,
        },
        pw_node_id: cast.pw_node_id,
        active: cast.active,
    }
}

fn label_of(output: &Output) -> Option<String> {
    let composed = match output.description.as_deref() {
        Some(description) => description.to_owned(),
        None => match (output.make.as_deref(), output.model.as_deref()) {
            (Some(make), Some(model)) => format!("{make} {model}"),
            (Some(only), None) | (None, Some(only)) => only.to_owned(),
            (None, None) => return None,
        },
    };

    let trimmed = composed.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn position(workspace: &WorkspaceInfo) -> u64 {
    workspace.index.map_or(workspace.id, u64::from)
}

fn capabilities(capabilities: Capabilities) -> CompositorCapabilities {
    CompositorCapabilities {
        floating: capabilities.floating,
        workspace_reorder: capabilities.workspace_reorder,
        output_power: capabilities.output_power,
    }
}

fn workspace_target(reference: WorkspaceRef) -> WorkspaceTarget {
    match reference {
        WorkspaceRef::Id { id } => WorkspaceTarget::Id(WorkspaceId(id)),
        WorkspaceRef::Index { index } => WorkspaceTarget::Index(index),
        WorkspaceRef::Name { name } => WorkspaceTarget::Name(name),
        WorkspaceRef::Next => WorkspaceTarget::Next,
        WorkspaceRef::Prev => WorkspaceTarget::Prev,
    }
}

fn window_target(reference: WindowRef, state: Option<&Snapshot>) -> Option<WindowTarget> {
    match reference {
        WindowRef::Id { id } => Some(WindowTarget::Id(WindowId(id))),
        WindowRef::Pid { pid } => state?
            .windows
            .iter()
            .filter(|window| window.pid == Some(pid))
            .min_by_key(|window| window.id.0)
            .map(|window| WindowTarget::Id(window.id)),
        WindowRef::Next => Some(WindowTarget::Next),
        WindowRef::Prev => Some(WindowTarget::Prev),
    }
}

fn command_error(error: &CompositorError) -> CommandError {
    match error {
        CompositorError::Unsupported(reason) | CompositorError::Unavailable(reason) => {
            CommandError::Unsupported(reason.to_string())
        }
        CompositorError::Connect { .. } | CompositorError::Closed => {
            CommandError::Unavailable(error.to_string())
        }
        CompositorError::Refused(reason) => CommandError::InvalidArgument(reason.to_string()),
        CompositorError::Protocol(reason) => CommandError::Internal(reason.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use glimpse_compositors::{Cast, CastKind, CastTarget, Niri, Window};
    use glimpse_dbus::Buses;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;
    use tokio::sync::{mpsc, watch};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::service::ServiceState;

    struct FakeNiri {
        _dir: tempfile::TempDir,
        socket: PathBuf,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl FakeNiri {
        fn spawn() -> Self {
            let dir = tempfile::tempdir().expect("a temporary directory");
            let socket = dir.path().join("niri.sock");
            let listener = UnixListener::bind(&socket).expect("bind the fake niri socket");
            let requests = Arc::new(Mutex::new(Vec::new()));
            let recorded = requests.clone();

            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let recorded = recorded.clone();
                    tokio::spawn(async move {
                        let mut reader = BufReader::new(stream);
                        let mut request = String::new();
                        if reader.read_line(&mut request).await.is_err() {
                            return;
                        }
                        recorded
                            .lock()
                            .expect("not poisoned")
                            .push(request.trim().to_owned());
                        let _ = reader.get_mut().write_all(b"{\"Ok\":null}\n").await;
                    });
                }
            });

            Self {
                _dir: dir,
                socket,
                requests,
            }
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().expect("not poisoned").clone()
        }
    }

    struct Harness {
        service: Compositor,
        ctx: Ctx<Compositor>,
        state: watch::Receiver<CompositorState>,
        _inbox: mpsc::Receiver<Input<Compositor>>,
        _cancel: CancellationToken,
    }

    fn harness(backend: Backend) -> Harness {
        let (events, inbox) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let (health, _health_rx) = watch::channel(ServiceState::Starting);
        let (published, state) = watch::channel(CompositorState::default());
        let ctx = Ctx::<Compositor>::new(
            events,
            &cancel,
            published,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Compositor::with_backend(&ctx, backend);

        Harness {
            service,
            ctx,
            state,
            _inbox: inbox,
            _cancel: cancel,
        }
    }

    fn snapshot_of(workspaces: Vec<Workspace>, windows: Vec<Window>) -> Snapshot {
        Snapshot {
            workspaces,
            windows,
            ..Snapshot::default()
        }
    }

    /// The backend is read from the environment by `start`; every assertion here supplies it
    /// instead, so the result does not depend on which compositor the test machine runs.
    #[tokio::test]
    async fn a_status_is_published_before_any_snapshot_arrives() {
        let harness = harness(Backend::Unsupported);

        let published = harness.state.borrow().clone();
        assert_eq!(
            published.status.map(|status| status.name),
            Some(Backend::Unsupported.name().to_owned()),
            "the bar needs a compositor name before the first snapshot resolves"
        );
        assert!(
            published.workspaces.is_none(),
            "nothing is known about workspaces until a snapshot arrives"
        );
    }

    /// `Changed` before the first `Snapshot` has no state to apply to, and must not publish an
    /// empty set of workspaces over the nothing that is already there.
    #[tokio::test]
    async fn a_change_before_the_first_snapshot_publishes_nothing() {
        let mut harness = harness(Backend::Unsupported);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Changed(Change::WorkspacesChanged(Vec::new()))),
            )
            .await;

        assert!(
            harness.state.borrow().workspaces.is_none(),
            "a change with no snapshot behind it is dropped rather than published as empty"
        );
    }

    #[tokio::test]
    async fn the_first_snapshot_publishes_and_clears_a_degraded_health() {
        let mut harness = harness(Backend::Unsupported);
        harness.ctx.degraded("no compositor yet".to_owned());

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Snapshot(Box::new(snapshot_of(
                    vec![workspace(1, Some(1), "DP-2")],
                    Vec::new(),
                )))),
            )
            .await;

        assert!(!harness.ctx.is_degraded(), "a snapshot clears degraded");
        let published = harness.state.borrow().clone();
        assert_eq!(
            published.workspaces.map(|shown| shown.workspaces.len()),
            Some(1)
        );
    }

    fn workspace(id: u64, idx: Option<u8>, output: &str) -> Workspace {
        Workspace {
            id: WorkspaceId(id),
            idx,
            name: None,
            output: Some(output.to_owned()),
            is_active: false,
            is_focused: false,
            is_urgent: false,
            active_window_id: None,
        }
    }

    fn window(id: u64, workspace: u64) -> Window {
        Window {
            id: WindowId(id),
            title: None,
            app_id: None,
            pid: None,
            workspace_id: Some(WorkspaceId(workspace)),
            is_focused: false,
            is_floating: false,
            is_urgent: false,
            layout_order: None,
        }
    }

    #[test]
    fn a_window_that_opens_focused_takes_focus_from_whatever_had_it() {
        let mut had_focus = window(1, 4);
        had_focus.is_focused = true;
        let mut opening = window(3, 4);
        opening.is_focused = true;

        let mut state = Snapshot {
            windows: vec![had_focus, window(2, 4)],
            focused_window: Some(WindowId(1)),
            ..Snapshot::default()
        };

        apply(&mut state, Change::WindowOpenedOrChanged(opening));

        assert_eq!(
            state
                .windows
                .iter()
                .filter(|window| window.is_focused)
                .map(|window| window.id)
                .collect::<Vec<WindowId>>(),
            [WindowId(3)],
            "a session focuses one window, so a popover listing them must never tick two rows"
        );
        assert_eq!(state.focused_window, Some(WindowId(3)));
    }

    #[test]
    fn a_window_changing_without_focus_leaves_focus_where_it_is() {
        let mut had_focus = window(1, 4);
        had_focus.is_focused = true;
        let mut retitled = window(2, 4);
        retitled.title = Some("vim".to_owned());

        let mut state = Snapshot {
            windows: vec![had_focus, window(2, 4)],
            focused_window: Some(WindowId(1)),
            ..Snapshot::default()
        };

        apply(&mut state, Change::WindowOpenedOrChanged(retitled));

        assert_eq!(state.focused_window, Some(WindowId(1)));
        assert!(
            state.windows[0].is_focused,
            "a title arriving on an unfocused window must not move focus onto nothing"
        );
    }

    fn output(connector: &str, enabled: bool) -> Output {
        Output {
            connector: connector.to_owned(),
            make: Some("Samsung".to_owned()),
            model: Some("ATNA60CL10-0 ".to_owned()),
            serial: None,
            description: None,
            logical: None,
            current_mode: None,
            enabled,
            built_in: false,
        }
    }

    #[test]
    fn a_window_is_focusable_by_the_pid_of_the_process_that_owns_it() {
        let mut owned = window(9, 1);
        owned.pid = Some(4265);
        let state = Snapshot {
            windows: vec![window(2, 1), owned],
            ..Snapshot::default()
        };

        assert_eq!(
            window_target(WindowRef::Pid { pid: 4265 }, Some(&state)),
            Some(WindowTarget::Id(WindowId(9)))
        );
    }

    /// `Snapshot.windows` is edited in place as windows come and go, so its order drifts over a
    /// session. Taking the first match would raise a different window of the same application at
    /// different moments; breaking the tie on the id is what makes it the same one every time.
    #[test]
    fn a_process_owning_several_windows_always_resolves_to_the_same_one() {
        let mut later = window(9, 1);
        later.pid = Some(4265);
        let mut earlier = window(3, 1);
        earlier.pid = Some(4265);
        let state = Snapshot {
            windows: vec![later, earlier],
            ..Snapshot::default()
        };

        assert_eq!(
            window_target(WindowRef::Pid { pid: 4265 }, Some(&state)),
            Some(WindowTarget::Id(WindowId(3)))
        );
    }

    /// Both refusals reach the caller as `InvalidArgs`, which does not invite a retry: there is
    /// nothing to retry when no window belongs to the process.
    #[test]
    fn a_pid_with_no_window_is_refused_and_so_is_one_asked_before_the_first_snapshot() {
        let state = Snapshot {
            windows: vec![window(2, 1)],
            ..Snapshot::default()
        };

        assert_eq!(
            window_target(WindowRef::Pid { pid: 4265 }, Some(&state)),
            None
        );
        assert_eq!(window_target(WindowRef::Pid { pid: 4265 }, None), None);
        assert_eq!(
            window_target(WindowRef::Id { id: 2 }, None),
            Some(WindowTarget::Id(WindowId(2))),
            "a reference that needs no window list still resolves without one"
        );
    }

    #[test]
    fn a_workspace_is_urgent_when_a_window_on_it_is() {
        let mut urgent = window(1, 7);
        urgent.is_urgent = true;
        let state = Snapshot {
            workspaces: vec![workspace(7, Some(1), "DP-1"), workspace(8, Some(2), "DP-1")],
            windows: vec![urgent, window(2, 8)],
            ..Snapshot::default()
        };

        let published = workspaces_of(&state);

        assert!(
            published[0].urgent,
            "Hyprland never marks a workspace urgent, so deriving it from the windows is the \
             only thing that makes it work there at all"
        );
        assert!(!published[1].urgent);
    }

    #[test]
    fn focusing_a_window_clears_the_urgency_hyprland_never_clears() {
        let mut urgent = window(1, 7);
        urgent.is_urgent = true;
        let mut state = Snapshot {
            workspaces: vec![workspace(7, Some(1), "DP-1")],
            windows: vec![urgent],
            ..Snapshot::default()
        };

        apply(&mut state, Change::WindowFocusChanged(Some(WindowId(1))));

        assert!(!state.windows[0].is_urgent);
        assert!(state.windows[0].is_focused);
        assert!(
            !workspaces_of(&state)[0].urgent,
            "the workspace stops being urgent with the window, since it is derived rather than \
             cached"
        );
    }

    fn cast(stream_id: u64, active: bool) -> Cast {
        Cast {
            stream_id,
            session_id: None,
            kind: CastKind::Unknown,
            target: CastTarget::Unknown,
            pw_node_id: None,
            active,
        }
    }

    #[test]
    fn casts_keep_privacy_active_until_the_last_stream_stops() {
        let mut state = Snapshot::default();

        apply(
            &mut state,
            Change::CastsChanged(vec![cast(3, true), cast(7, true)]),
        );
        apply(&mut state, Change::CastStopped(3));
        assert_eq!(
            state
                .casts
                .iter()
                .map(|cast| cast.stream_id)
                .collect::<Vec<_>>(),
            [7]
        );

        apply(
            &mut state,
            Change::CastStartedOrChanged {
                cast: cast(7, false),
            },
        );
        assert_eq!(state.casts.len(), 1);
        assert!(!state.casts[0].active);
    }

    #[tokio::test]
    async fn a_snapshot_publishes_privacy_as_active_when_any_cast_is() {
        let mut harness = harness(Backend::Unsupported);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Snapshot(Box::new(Snapshot {
                    casts: vec![cast(2, false), cast(7, true)],
                    ..Snapshot::default()
                }))),
            )
            .await;

        let published = harness
            .state
            .borrow()
            .privacy
            .clone()
            .expect("privacy published");
        assert!(published.active);
        assert_eq!(published.casts.len(), 2);
    }

    #[tokio::test]
    async fn a_paused_cast_still_publishes_privacy_as_active() {
        let mut harness = harness(Backend::Unsupported);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Snapshot(Box::new(Snapshot {
                    casts: vec![cast(2, false), cast(7, false)],
                    ..Snapshot::default()
                }))),
            )
            .await;

        let published = harness
            .state
            .borrow()
            .privacy
            .clone()
            .expect("privacy published");
        assert!(
            published.active,
            "OBS pauses every stream on a scene switch without ending the session, so a session \
             with no currently-streaming cast is still one a portal will not prompt for again"
        );
    }

    #[tokio::test]
    async fn no_casts_publishes_privacy_as_inactive() {
        let mut harness = harness(Backend::Unsupported);

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::Snapshot(Box::default())))
            .await;

        let published = harness
            .state
            .borrow()
            .privacy
            .clone()
            .expect("privacy published");
        assert!(!published.active);
        assert!(published.casts.is_empty());
    }

    #[test]
    fn activating_a_workspace_leaves_the_other_output_alone() {
        let mut here = workspace(1, Some(1), "DP-1");
        here.is_active = true;
        let mut there = workspace(9, Some(1), "HDMI-A-1");
        there.is_active = true;
        let mut state = Snapshot {
            workspaces: vec![here, workspace(2, Some(2), "DP-1"), there],
            ..Snapshot::default()
        };

        apply(
            &mut state,
            Change::WorkspaceActivated {
                id: WorkspaceId(2),
                focused: true,
            },
        );

        assert!(!state.workspaces[0].is_active);
        assert!(state.workspaces[1].is_active);
        assert!(
            state.workspaces[2].is_active,
            "each output has its own current workspace; activating one must not clear the other"
        );
        assert!(state.workspaces[1].is_focused);
        assert!(!state.workspaces[2].is_focused);
        assert_eq!(state.focused_output.as_deref(), Some("DP-1"));
    }

    #[test]
    fn workspaces_are_ordered_by_output_then_by_position() {
        let state = Snapshot {
            workspaces: vec![
                workspace(30, Some(2), "HDMI-A-1"),
                workspace(20, Some(2), "DP-1"),
                workspace(10, Some(1), "DP-1"),
                workspace(40, Some(1), "HDMI-A-1"),
            ],
            ..Snapshot::default()
        };

        let published = workspaces_of(&state);

        assert_eq!(
            published.iter().map(|it| it.id).collect::<Vec<_>>(),
            [10, 20, 40, 30]
        );
    }

    #[test]
    fn a_workspace_without_an_index_is_ordered_by_its_id() {
        let state = Snapshot {
            workspaces: vec![workspace(9, None, "DP-1"), workspace(3, None, "DP-1")],
            ..Snapshot::default()
        };

        assert_eq!(
            workspaces_of(&state)
                .iter()
                .map(|it| it.id)
                .collect::<Vec<_>>(),
            [3, 9],
            "Hyprland fills no idx, and its id is the ordering"
        );
    }

    #[test]
    fn an_activation_naming_a_workspace_we_have_not_seen_asks_for_a_resync() {
        let mut focused = workspace(1, Some(1), "DP-1");
        focused.is_focused = true;
        focused.is_active = true;
        let mut state = Snapshot {
            workspaces: vec![focused],
            ..Snapshot::default()
        };

        assert!(
            apply(
                &mut state,
                Change::WorkspaceActivated {
                    id: WorkspaceId(99),
                    focused: true,
                }
            ),
            "the list is behind the compositor, and re-reading it is the only way to catch up"
        );
        assert!(
            state.workspaces[0].is_focused,
            "clearing focus everywhere and setting it nowhere would leave the strip with no \
             current workspace at all"
        );
    }

    #[test]
    fn a_structural_resync_asks_for_a_new_snapshot_and_a_keyboard_one_does_not() {
        let mut state = Snapshot::default();

        assert!(apply(&mut state, Change::Resync(Resync::Structure)));
        assert!(apply(&mut state, Change::Resync(Resync::Outputs)));
        assert!(
            !apply(&mut state, Change::Resync(Resync::Keyboard)),
            "the keyboard belongs to another service, and re-reading the whole snapshot for it \
             would be a round trip per layout switch"
        );
    }

    #[test]
    fn a_disabled_output_still_publishes_with_enabled_false() {
        let state = Snapshot {
            outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
            focused_output: Some("DP-1".to_owned()),
            ..Snapshot::default()
        };

        let published = outputs_of(&state);

        assert_eq!(published.len(), 2, "a disabled output must stay switchable");
        let disabled = published
            .iter()
            .find(|output| output.connector == "HDMI-A-1")
            .expect("HDMI-A-1");
        assert!(!disabled.enabled);
        let enabled = published
            .iter()
            .find(|output| output.connector == "DP-1")
            .expect("DP-1");
        assert!(enabled.focused);
        assert_eq!(
            enabled.label.as_deref(),
            Some("Samsung ATNA60CL10-0"),
            "niri fills make and model, leaves description null, and pads the model with a \
             trailing space; a popover offering `Move to display` has nothing to render otherwise"
        );
    }

    #[test]
    fn a_snapshot_populates_every_output_field() {
        let mut display = output("DP-1", true);
        display.model = Some("ATNA60CL10-0".to_owned());
        display.serial = Some("69QC174".to_owned());
        display.current_mode = Some(glimpse_compositors::Mode {
            width: 3840,
            height: 2160,
            refresh_mhz: 239_991,
        });
        display.logical = Some(glimpse_compositors::Logical {
            x: -3072,
            y: 0,
            width: 3072,
            height: 1728,
            scale: 1.25,
        });
        let state = Snapshot {
            outputs: vec![display],
            focused_output: Some("DP-1".to_owned()),
            ..Snapshot::default()
        };

        let published = outputs_of(&state);

        assert_eq!(published[0].make.as_deref(), Some("Samsung"));
        assert_eq!(published[0].model.as_deref(), Some("ATNA60CL10-0"));
        assert_eq!(published[0].serial.as_deref(), Some("69QC174"));
        assert_eq!(
            published[0].current_mode,
            Some(OutputMode {
                width: 3840,
                height: 2160,
                refresh_mhz: 239_991,
            })
        );
        assert_eq!(
            published[0].logical,
            Some(OutputLogical {
                x: -3072,
                y: 0,
                width: 3072,
                height: 1728,
                scale: 1.25,
            })
        );
        assert!(published[0].enabled);
    }

    #[test]
    fn output_power_is_mirrored_into_compositor_capabilities() {
        let mapped = capabilities(Capabilities {
            floating: false,
            workspace_reorder: true,
            output_power: true,
        });

        assert!(mapped.output_power);
    }

    #[tokio::test]
    async fn an_identical_snapshot_does_not_republish() {
        let mut harness = harness(Backend::Unsupported);
        let snapshot = || Snapshot {
            outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
            focused_output: Some("DP-1".to_owned()),
            ..Snapshot::default()
        };

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Snapshot(Box::new(snapshot()))),
            )
            .await;
        harness.state.borrow_and_update();

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Snapshot(Box::new(snapshot()))),
            )
            .await;

        assert!(
            !harness.state.has_changed().expect("sender is alive"),
            "two identical snapshots must not rebuild every consumer's widget list"
        );
    }

    #[test]
    fn a_description_is_preferred_to_a_make_and_model() {
        let mut described = output("DP-1", true);
        described.description = Some("Samsung 14\"".to_owned());

        assert_eq!(
            label_of(&described).as_deref(),
            Some("Samsung 14\""),
            "Hyprland supplies the description, which is the better label of the two"
        );
    }

    #[test]
    fn a_capability_a_compositor_lacks_is_refused_without_inviting_a_retry() {
        assert!(matches!(
            command_error(&CompositorError::Unavailable("reorder a workspace")),
            CommandError::Unsupported(_)
        ));
        assert!(matches!(
            command_error(&CompositorError::Closed),
            CommandError::Unavailable(_)
        ));
    }

    async fn snapshot_of_outputs(harness: &mut Harness, outputs: Vec<Output>) {
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Snapshot(Box::new(Snapshot {
                    outputs,
                    ..Snapshot::default()
                }))),
            )
            .await;
    }

    async fn set_output_enabled(
        harness: &mut Harness,
        connector: &str,
        enabled: bool,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Command(Command::SetOutputEnabled {
                    connector: connector.to_owned(),
                    enabled,
                    reply,
                }),
            )
            .await;
        result.await.expect("service is running")
    }

    async fn power_off_monitors(harness: &mut Harness) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Command(Command::PowerOffMonitors { reply }),
            )
            .await;
        result.await.expect("service is running")
    }

    async fn stop_screencast(harness: &mut Harness, session_id: u64) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Command(Command::StopScreencast { session_id, reply }),
            )
            .await;
        result.await.expect("service is running")
    }

    #[tokio::test]
    async fn disabling_the_only_enabled_output_is_refused_without_reaching_the_backend() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
        snapshot_of_outputs(&mut harness, vec![output("DP-1", true)]).await;

        let result = set_output_enabled(&mut harness, "DP-1", false).await;

        assert!(matches!(result, Err(CommandError::InvalidArgument(_))));
        assert!(
            fake.requests().is_empty(),
            "no call may reach the compositor backend"
        );
    }

    #[tokio::test]
    async fn disabling_one_of_two_enabled_outputs_reaches_the_backend_and_succeeds() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
        snapshot_of_outputs(
            &mut harness,
            vec![output("DP-1", true), output("HDMI-A-1", true)],
        )
        .await;

        let result = set_output_enabled(&mut harness, "DP-1", false).await;

        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.requests(),
            vec![r#"{"Output":{"action":"Off","output":"DP-1"}}"#]
        );
    }

    #[tokio::test]
    async fn enabling_a_disabled_output_is_never_blocked_by_the_guard() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
        snapshot_of_outputs(
            &mut harness,
            vec![output("DP-1", true), output("HDMI-A-1", false)],
        )
        .await;

        let result = set_output_enabled(&mut harness, "HDMI-A-1", true).await;

        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.requests(),
            vec![r#"{"Output":{"action":"On","output":"HDMI-A-1"}}"#]
        );
    }

    #[tokio::test]
    async fn disabling_an_already_disabled_output_is_not_mistaken_for_the_last_one() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
        snapshot_of_outputs(
            &mut harness,
            vec![output("DP-1", true), output("HDMI-A-1", false)],
        )
        .await;

        let result = set_output_enabled(&mut harness, "HDMI-A-1", false).await;

        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.requests(),
            vec![r#"{"Output":{"action":"Off","output":"HDMI-A-1"}}"#]
        );
    }

    #[tokio::test]
    async fn an_unknown_connector_is_refused_without_reaching_the_backend() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
        snapshot_of_outputs(&mut harness, vec![output("DP-1", true)]).await;

        let result = set_output_enabled(&mut harness, "HDMI-A-1", false).await;

        assert!(matches!(result, Err(CommandError::InvalidArgument(_))));
        assert!(fake.requests().is_empty());
    }

    #[tokio::test]
    async fn power_off_monitors_reaches_the_backend_with_no_last_output_guard() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
        snapshot_of_outputs(&mut harness, vec![output("DP-1", true)]).await;

        let result = power_off_monitors(&mut harness).await;

        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.requests(),
            vec![r#"{"Action":{"PowerOffMonitors":{}}}"#]
        );
    }

    #[tokio::test]
    async fn stop_screencast_reaches_the_backend_with_the_session_id() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));

        let result = stop_screencast(&mut harness, 2).await;

        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.requests(),
            vec![r#"{"Action":{"StopCast":{"session_id":2}}}"#]
        );
    }

    #[tokio::test]
    async fn stop_screencast_is_unavailable_under_hyprland() {
        let mut harness = harness(Backend::Hyprland(glimpse_compositors::Hyprland::at(
            "/nonexistent",
        )));

        assert!(matches!(
            stop_screencast(&mut harness, 2).await,
            Err(CommandError::Unsupported(_))
        ));
    }

    #[tokio::test]
    async fn output_commands_are_unsupported_without_the_capability() {
        let mut harness = harness(Backend::Unsupported);
        snapshot_of_outputs(
            &mut harness,
            vec![output("DP-1", true), output("HDMI-A-1", true)],
        )
        .await;

        assert!(matches!(
            set_output_enabled(&mut harness, "DP-1", false).await,
            Err(CommandError::Unsupported(_))
        ));
        assert!(matches!(
            power_off_monitors(&mut harness).await,
            Err(CommandError::Unsupported(_))
        ));
    }

    #[tokio::test]
    async fn set_output_enabled_before_any_snapshot_is_refused_rather_than_reaching_the_backend() {
        let fake = FakeNiri::spawn();
        let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));

        let result = set_output_enabled(&mut harness, "DP-1", false).await;

        assert_eq!(
            result,
            Err(CommandError::InvalidArgument(
                "the compositor state has not arrived yet".to_owned()
            ))
        );
        assert!(fake.requests().is_empty());
    }
}
