use futures_util::{
    FutureExt, StreamExt,
    future::{BoxFuture, ready},
    stream,
};
use glimpse_compositors::{
    Capabilities, Cast, CastKind, CastTarget, Compositor as Backend, CompositorError,
    Event as Change, Output, Resync, Snapshot, WindowId, WindowTarget, Workspace, WorkspaceId,
    WorkspaceTarget, detect_compositor,
};
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint},
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
    pub output_power: bool,
    #[serde(default)]
    pub blank_screens: bool,
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
    FetchFailed(String),
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
    RenameWorkspace {
        id: u64,
        name: Option<String>,
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

pub type CompositorHandle = ServiceEndpoint<Compositor>;

impl ServiceEndpoint<Compositor> {
    pub async fn focus_workspace(&self, target: WorkspaceRef) -> Result<(), CommandError> {
        self.call("compositor service stopped", |reply| {
            Command::FocusWorkspace { target, reply }
        })
        .await?
    }

    pub async fn focus_window(&self, target: WindowRef) -> Result<(), CommandError> {
        self.call("compositor service stopped", |reply| Command::FocusWindow {
            target,
            reply,
        })
        .await?
    }

    pub async fn focus_app(&self, pid: Option<i32>, names: &[&str]) -> Result<(), CommandError> {
        let by_pid = match pid {
            Some(pid) => self.focus_window(WindowRef::Pid { pid }).await,
            None => Err(CommandError::InvalidArgument(
                "the application has no known process".to_owned(),
            )),
        };
        if by_pid.is_ok() {
            return by_pid;
        }
        let windows = self.snapshot().windows.map(|windows| windows.windows);
        match app_window(windows.as_deref().unwrap_or_default(), names) {
            Some(id) => self.focus_window(WindowRef::Id { id }).await,
            None => {
                tracing::debug!(?pid, ?names, "no window to focus");
                Ok(())
            }
        }
    }

    pub async fn rename_workspace(
        &self,
        id: u64,
        name: Option<String>,
    ) -> Result<(), CommandError> {
        self.call("compositor service stopped", |reply| {
            Command::RenameWorkspace { id, name, reply }
        })
        .await?
    }

    pub async fn set_output_enabled(
        &self,
        connector: String,
        enabled: bool,
    ) -> Result<(), CommandError> {
        self.call("compositor service stopped", |reply| {
            Command::SetOutputEnabled {
                connector,
                enabled,
                reply,
            }
        })
        .await?
    }

    pub async fn power_off_monitors(&self) -> Result<(), CommandError> {
        self.call("compositor service stopped", |reply| {
            Command::PowerOffMonitors { reply }
        })
        .await?
    }

    pub async fn stop_screencast(&self, session_id: u64) -> Result<(), CommandError> {
        self.call("compositor service stopped", |reply| {
            Command::StopScreencast { session_id, reply }
        })
        .await?
    }
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
    fetch_failed: bool,
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
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

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
                            Err(error) => Event::FetchFailed(error.to_string()),
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
    ) -> Self {
        let backend = detect_compositor();
        tracing::debug!(compositor = backend.name(), "starting compositor service");

        Self::with_backend(ctx, backend)
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
                    if std::mem::take(&mut self.fetch_failed) {
                        self.attempt += 1;
                    }
                    return;
                };
                if apply(state, change) {
                    self.attempt += 1;
                }
                self.publish();
            }
            Input::Event(Event::FetchFailed(reason)) => {
                self.fetch_failed = true;
                ctx.degraded(reason);
            }
            Input::Event(Event::Failed(reason)) => ctx.degraded(reason),
            Input::Command(command) => self.dispatch(ctx, command),
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
            fetch_failed: false,
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

    fn dispatch(&self, ctx: &Ctx<Self>, command: Command) {
        let backend = self.backend.clone();
        let (call, reply): (BoxFuture<'static, Result<(), CompositorError>>, _) = match command {
            Command::FocusWorkspace { target, reply } => {
                let target = workspace_target(target);
                (
                    async move { backend.focus_workspace(target).await }.boxed(),
                    reply,
                )
            }
            Command::FocusWindow { target, reply } => (
                match window_target(target, self.state.as_ref()) {
                    Some(target) => async move { backend.focus_window(target).await }.boxed(),
                    None => ready(Err(CompositorError::Refused(
                        "no window matches the requested reference".to_owned(),
                    )))
                    .boxed(),
                },
                reply,
            ),
            Command::RenameWorkspace { id, name, reply } => (
                async move {
                    backend
                        .rename_workspace(WorkspaceId(id), name.as_deref())
                        .await
                }
                .boxed(),
                reply,
            ),
            Command::SetOutputEnabled {
                connector,
                enabled,
                reply,
            } => (
                match self.check_output_enabled(&connector, enabled) {
                    Ok(()) => {
                        async move { backend.set_output_enabled(&connector, enabled).await }.boxed()
                    }
                    Err(error) => ready(Err(error)).boxed(),
                },
                reply,
            ),
            Command::PowerOffMonitors { reply } => (
                async move { backend.power_off_monitors().await }.boxed(),
                reply,
            ),
            Command::StopScreencast { session_id, reply } => (
                async move { backend.stop_screencast(session_id).await }.boxed(),
                reply,
            ),
        };

        ctx.spawn_detached(move |_| async move {
            let _ = reply.send(call.await.map_err(|error| command_error(&error)));
        });
    }

    fn check_output_enabled(&self, connector: &str, enabled: bool) -> Result<(), CompositorError> {
        if !self.capabilities.output_power {
            return Err(CompositorError::Unsupported(
                "this compositor cannot turn displays on or off",
            ));
        }
        let Some(state) = self.state.as_ref() else {
            return Err(CompositorError::Refused(
                "the compositor state has not arrived yet".to_owned(),
            ));
        };
        let Some(output) = state
            .outputs
            .iter()
            .find(|output| output.connector == connector)
        else {
            return Err(CompositorError::Refused(format!(
                "unknown output {connector}"
            )));
        };
        if !enabled && output.enabled && enabled_output_count(state) <= 1 {
            return Err(CompositorError::Refused(
                "cannot turn off the last display that is still on".to_owned(),
            ));
        }
        Ok(())
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
        | Change::KeyboardLayoutSwitched(_)
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
        output_power: capabilities.output_power,
        blank_screens: capabilities.blank_screens,
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

fn app_window(windows: &[WindowInfo], names: &[&str]) -> Option<u64> {
    let names: Vec<String> = names
        .iter()
        .map(|name| app_key(name))
        .filter(|name| !name.is_empty())
        .collect();
    windows
        .iter()
        .filter(|window| {
            window
                .app_id
                .as_deref()
                .is_some_and(|app_id| names.contains(&app_key(app_id)))
        })
        .min_by_key(|window| window.id)
        .map(|window| window.id)
}

fn app_key(name: &str) -> String {
    name.trim().trim_end_matches(".desktop").to_lowercase()
}

pub(crate) fn command_error(error: &CompositorError) -> CommandError {
    match error {
        CompositorError::Unsupported(reason) | CompositorError::Unavailable(reason) => {
            CommandError::Unsupported(reason.to_string())
        }
        CompositorError::Connect { .. } | CompositorError::Closed => {
            CommandError::Unavailable(error.to_string())
        }
        CompositorError::Refused(reason) => CommandError::InvalidArgument(reason.to_string()),
        CompositorError::Protocol(reason) => CommandError::Internal(reason.to_string()),
        CompositorError::NoFile(_)
        | CompositorError::InvalidPath(_)
        | CompositorError::Watch { .. } => CommandError::Internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests;
