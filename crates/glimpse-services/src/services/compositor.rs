use futures_util::{FutureExt, future::BoxFuture};
use glimpse_compositors::{
    client::{CompositorState as Mirrored, Handle as ProviderHandle},
    contract::{Capability, Error as ProviderError},
    model::{Output, WindowId, WorkspaceId},
};
use glimpse_dbus::PENDING;
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint},
    subscription::Sub,
};
use serde::{Deserialize, Serialize};

pub(crate) const NO_PROVIDER: &str = "no compositor provider is running";
const UNSUPPORTED: &str = "unsupported";

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
    pub current_mode: Option<OutputMode>,
    pub logical: Option<OutputLogical>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum WorkspaceRef {
    Id { id: u64 },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositorStatus {
    pub name: String,
    pub capabilities: Vec<Capability>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastInfo {
    pub stream_id: u64,
    pub session_id: Option<u64>,
    pub output: Option<String>,
    pub pw_node_id: Option<u32>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositorPrivacy {
    pub active: bool,
    pub casts: Vec<CastInfo>,
}

pub enum Event {
    Provider(Box<Mirrored>),
    Closed,
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
    pub fn supports(&self, capability: Capability) -> bool {
        self.subscribe()
            .borrow()
            .status
            .as_ref()
            .is_some_and(|status| status.capabilities.contains(&capability))
    }

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
        let Some(windows) = self.snapshot().windows else {
            return Err(CommandError::Unsupported(NO_PROVIDER.to_owned()));
        };
        if let Some(pid) = pid {
            match self.focus_window(WindowRef::Pid { pid }).await {
                Ok(()) => return Ok(()),
                Err(CommandError::InvalidArgument(_)) => {}
                Err(error) => return Err(error),
            }
        }
        match app_window(&windows.windows, names) {
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

pub struct Dependencies {
    pub provider: ProviderHandle,
}

pub struct Compositor {
    state_publisher: Publisher<CompositorState>,
    provider: ProviderHandle,
    served: Option<Mirrored>,
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Provider,
}

impl Service for Compositor {
    const NAME: &'static str = "compositor";

    type Config = NoConfig;
    type State = CompositorState;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![Sub::watch(
            Watch::Provider,
            self.provider.subscribe(),
            |state| Event::Provider(Box::new(state)),
            Event::Closed,
        )]
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        tracing::debug!("starting compositor service");
        Self {
            state_publisher: ctx.publisher(),
            provider: dependencies.provider,
            served: None,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Provider(state)) => match state.unavailable.clone() {
                None => {
                    if ctx.is_degraded() {
                        ctx.running();
                    }
                    self.publish(&state);
                    self.served = Some(*state);
                }
                Some(reason) if reason == PENDING => {}
                Some(reason) => self.lost(ctx, reason),
            },
            Input::Event(Event::Closed) => {
                self.lost(ctx, "the compositor provider closed".to_owned());
            }
            Input::Command(command) => self.dispatch(ctx, command),
            Input::Config(NoConfig) => {}
        }
    }
}

impl Compositor {
    fn lost(&self, ctx: &Ctx<Self>, reason: String) {
        if self.served.is_some() {
            ctx.degraded(reason);
            return;
        }
        self.state_publisher.set(CompositorState {
            status: Some(CompositorStatus {
                name: UNSUPPORTED.to_owned(),
                capabilities: Vec::new(),
            }),
            privacy: Some(CompositorPrivacy {
                active: false,
                casts: Vec::new(),
            }),
            ..CompositorState::default()
        });
    }

    fn publish(&self, state: &Mirrored) {
        self.state_publisher.set(CompositorState {
            status: Some(CompositorStatus {
                name: state.name.clone(),
                capabilities: state.capabilities.clone(),
            }),
            workspaces: Some(CompositorWorkspaces {
                workspaces: workspaces_of(state),
            }),
            windows: Some(CompositorWindows {
                windows: windows_of(state),
            }),
            outputs: Some(CompositorOutputs {
                outputs: outputs_of(state),
            }),
            privacy: Some(CompositorPrivacy {
                active: !state.casts.is_empty(),
                casts: casts_of(state),
            }),
        });
    }

    fn gate(&self, needs: Option<Capability>) -> Result<&Mirrored, ProviderError> {
        let served = self
            .served
            .as_ref()
            .ok_or_else(|| ProviderError::Unsupported(NO_PROVIDER.to_owned()))?;
        match needs.filter(|needed| !served.capabilities.contains(needed)) {
            Some(missing) => Err(ProviderError::Unsupported(format!(
                "this compositor does not support {}",
                missing.as_str()
            ))),
            None => Ok(served),
        }
    }

    fn dispatch(&self, ctx: &Ctx<Self>, command: Command) {
        let provider = self.provider.clone();
        let (call, reply): (Result<BoxFuture<'static, Result<(), ProviderError>>, _>, _) =
            match command {
                Command::FocusWorkspace { target, reply } => (
                    self.gate(None).map(|_| match target {
                        WorkspaceRef::Id { id } => {
                            async move { provider.focus_workspace(WorkspaceId(id)).await }.boxed()
                        }
                        WorkspaceRef::Next => {
                            async move { provider.step_workspace(true).await }.boxed()
                        }
                        WorkspaceRef::Prev => {
                            async move { provider.step_workspace(false).await }.boxed()
                        }
                    }),
                    reply,
                ),
                Command::FocusWindow { target, reply } => (
                    self.gate(None).and_then(|served| match target {
                        WindowRef::Id { id } => {
                            Ok(async move { provider.focus_window(WindowId(id)).await }.boxed())
                        }
                        WindowRef::Pid { pid } => served
                            .windows
                            .iter()
                            .filter(|window| window.pid == Some(pid))
                            .min_by_key(|window| window.id.0)
                            .map(|window| window.id)
                            .ok_or_else(|| {
                                ProviderError::Refused(
                                    "no window matches the requested reference".to_owned(),
                                )
                            })
                            .map(|id| async move { provider.focus_window(id).await }.boxed()),
                        WindowRef::Next => {
                            Ok(async move { provider.step_window(true).await }.boxed())
                        }
                        WindowRef::Prev => {
                            Ok(async move { provider.step_window(false).await }.boxed())
                        }
                    }),
                    reply,
                ),
                Command::RenameWorkspace { id, name, reply } => (
                    self.gate(Some(Capability::RenameWorkspace)).map(|_| {
                        async move {
                            provider
                                .rename_workspace(
                                    WorkspaceId(id),
                                    name.as_deref().unwrap_or_default(),
                                )
                                .await
                        }
                        .boxed()
                    }),
                    reply,
                ),
                Command::SetOutputEnabled {
                    connector,
                    enabled,
                    reply,
                } => (
                    self.gate(Some(Capability::OutputPower))
                        .and_then(|served| last_display_guard(served, &connector, enabled))
                        .map(|()| {
                            async move { provider.set_output_enabled(&connector, enabled).await }
                                .boxed()
                        }),
                    reply,
                ),
                Command::PowerOffMonitors { reply } => (
                    self.gate(Some(Capability::BlankScreens))
                        .map(|_| async move { provider.power_off_monitors().await }.boxed()),
                    reply,
                ),
                Command::StopScreencast { session_id, reply } => (
                    self.gate(None)
                        .map(|_| async move { provider.stop_screencast(session_id).await }.boxed()),
                    reply,
                ),
            };

        ctx.spawn_detached(move |_| async move {
            let result = match call {
                Ok(call) => call.await,
                Err(error) => Err(error),
            };
            let _ = reply.send(result.map_err(|error| command_error(&error)));
        });
    }
}

fn last_display_guard(
    state: &Mirrored,
    connector: &str,
    enabled: bool,
) -> Result<(), ProviderError> {
    let enabled_outputs = state.outputs.iter().filter(|output| output.enabled).count();
    let turning_off_the_last = !enabled
        && enabled_outputs <= 1
        && state
            .outputs
            .iter()
            .any(|output| output.connector == connector && output.enabled);
    match turning_off_the_last {
        true => Err(ProviderError::Refused(
            "cannot turn off the last display that is still on".to_owned(),
        )),
        false => Ok(()),
    }
}

fn workspaces_of(state: &Mirrored) -> Vec<WorkspaceInfo> {
    let mut workspaces: Vec<WorkspaceInfo> = state
        .workspaces
        .iter()
        .map(|workspace| WorkspaceInfo {
            id: workspace.id.0,
            index: workspace.index,
            name: workspace.name.clone(),
            output: workspace.output.clone(),
            active: workspace.active,
            focused: workspace.active
                && workspace.output.is_some()
                && workspace.output == state.focus.output,
            urgent: state
                .windows
                .iter()
                .any(|window| window.urgent && window.workspace == Some(workspace.id)),
            windows: state
                .windows
                .iter()
                .filter(|window| window.workspace == Some(workspace.id))
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

fn windows_of(state: &Mirrored) -> Vec<WindowInfo> {
    let mut windows: Vec<WindowInfo> = state
        .windows
        .iter()
        .map(|window| WindowInfo {
            id: window.id.0,
            title: window.title.clone(),
            app_id: window.app_id.clone(),
            workspace: window.workspace.map(|id| id.0),
            focused: state.focus.window == Some(window.id),
            floating: window.floating,
            urgent: window.urgent,
            order: window.order,
        })
        .collect();

    windows.sort_by_key(|window| (window.workspace, window.order, window.id));
    windows
}

fn outputs_of(state: &Mirrored) -> Vec<OutputInfo> {
    state
        .outputs
        .iter()
        .map(|output| OutputInfo {
            connector: output.connector.clone(),
            label: label_of(output),
            built_in: built_in(&output.connector),
            focused: state.focus.output.as_ref() == Some(&output.connector),
            make: output.make.clone(),
            model: output.model.clone(),
            current_mode: output.mode.map(|mode| OutputMode {
                width: mode.width,
                height: mode.height,
                refresh_mhz: mode.refresh_mhz,
            }),
            logical: output.logical.map(|logical| OutputLogical {
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

fn casts_of(state: &Mirrored) -> Vec<CastInfo> {
    state
        .casts
        .iter()
        .map(|cast| CastInfo {
            stream_id: cast.id,
            session_id: cast.session,
            output: cast.output.clone(),
            pw_node_id: cast.pw_node,
            active: cast.active,
        })
        .collect()
}

fn built_in(connector: &str) -> bool {
    let connector = connector.to_ascii_lowercase();
    connector.starts_with("edp-") || connector.starts_with("lvds-")
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

pub(crate) fn command_error(error: &ProviderError) -> CommandError {
    match error {
        ProviderError::Unsupported(reason) => CommandError::Unsupported(reason.clone()),
        ProviderError::Refused(reason) => CommandError::InvalidArgument(reason.clone()),
        ProviderError::Unavailable(reason) => CommandError::Unavailable(reason.clone()),
        ProviderError::Failed(reason) => CommandError::Internal(reason.clone()),
    }
}

#[cfg(test)]
pub(crate) mod tests;
