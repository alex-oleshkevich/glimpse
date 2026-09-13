use futures_util::{StreamExt, stream};
use glimpse_compositors::{
    Capabilities, Compositor as Backend, CompositorError, Event as Change, Output, Resync,
    Snapshot, WindowId, WindowTarget, Workspace, WorkspaceId, WorkspaceTarget, detect_compositor,
};
use glimpse_contracts::{
    CompositorCapabilities, CompositorOutputs, CompositorPrivacy, CompositorStatus,
    CompositorWindows, CompositorWorkspaces, OutputInfo, WindowInfo, WindowRef, WorkspaceInfo,
    WorkspaceRef,
};
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};

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

pub fn initial_state() -> CompositorState {
    CompositorState::default()
}

pub struct Compositor {
    backend: Backend,
    state_publisher: Publisher<CompositorState>,
    state: Option<Snapshot>,
    attempt: u64,
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

        let service = Self {
            state_publisher: ctx.publisher(),
            state: None,
            attempt: 0,
            backend,
        };

        service.state_publisher.update(|state| {
            state.status = Some(CompositorStatus {
                name: service.backend.name().to_owned(),
                capabilities: capabilities(service.backend.capabilities()),
            });
        });

        Ok(service)
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
    fn publish(&mut self) {
        let Some(state) = self.state.as_ref() else {
            return;
        };

        let workspaces = workspaces_of(state);
        let windows = windows_of(state);
        let outputs = outputs_of(state);

        self.state_publisher.update(|published| {
            published.workspaces = Some(CompositorWorkspaces { workspaces });
            published.windows = Some(CompositorWindows { windows });
            published.outputs = Some(CompositorOutputs { outputs });
            published.privacy = Some(CompositorPrivacy {
                active: !state.active_casts.is_empty(),
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
        };

        let outcome = outcome.map_err(|error| command_error(&error));
        let _ = reply.send(outcome);
    }
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
        Change::CastsChanged(casts) => state.active_casts = casts,
        Change::CastStartedOrChanged { id, active } => {
            if active {
                state.active_casts.insert(id);
            } else {
                state.active_casts.remove(&id);
            }
        }
        Change::CastStopped(id) => {
            state.active_casts.remove(&id);
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
        .filter(|output| output.enabled)
        .map(|output| OutputInfo {
            connector: output.connector.clone(),
            label: label_of(output),
            built_in: output.built_in,
            focused: state.focused_output.as_ref() == Some(&output.connector),
        })
        .collect()
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
    use glimpse_compositors::Window;

    use super::*;

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

    #[test]
    fn casts_keep_privacy_active_until_the_last_stream_stops() {
        let mut state = Snapshot::default();

        apply(&mut state, Change::CastsChanged([3_u64, 7_u64].into()));
        apply(&mut state, Change::CastStopped(3));
        assert_eq!(state.active_casts, [7].into());

        apply(
            &mut state,
            Change::CastStartedOrChanged {
                id: 7,
                active: false,
            },
        );
        assert!(state.active_casts.is_empty());
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
    fn a_disabled_output_is_not_published() {
        let state = Snapshot {
            outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
            focused_output: Some("DP-1".to_owned()),
            ..Snapshot::default()
        };

        let published = outputs_of(&state);

        assert_eq!(published.len(), 1);
        assert!(published[0].focused);
        assert_eq!(
            published[0].label.as_deref(),
            Some("Samsung ATNA60CL10-0"),
            "niri fills make and model, leaves description null, and pads the model with a \
             trailing space; a popover offering `Move to display` has nothing to render otherwise"
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
}
