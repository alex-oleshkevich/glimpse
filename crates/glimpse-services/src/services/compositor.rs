use futures_util::{StreamExt, stream};
use glimpse_compositors::{
    Capabilities, Compositor as Backend, CompositorError, Event as Change, Output, Resync,
    Snapshot, WindowId, WindowTarget, Workspace, WorkspaceId, WorkspaceTarget, detect_compositor,
};
use glimpse_contracts::{
    CloseWindow, Command as _, CompositorCapabilities, CompositorOutputs, CompositorStatus,
    CompositorWindows, CompositorWorkspaces, FocusOutput, FocusWindow, FocusWorkspace, Message,
    MoveWindowToWorkspace, MoveWorkspaceToOutput, OutputInfo, RenameWorkspace, ReorderWorkspace,
    WindowInfo, WindowRef, WorkspaceInfo, WorkspaceRef,
};
use glimpse_ipc::{CallError, ErrorCode};
use serde_json::Value;

use crate::{
    broker::Responder,
    context::Ctx,
    publisher::Publisher,
    service::{Input, NoConfig, Service, ServiceError, decode_args, unknown_command},
    subscription::Sub,
};

pub enum Event {
    Snapshot(Box<Snapshot>),
    Changed(Change),
    Failed(String),
}

#[derive(Debug)]
pub enum Command {
    FocusWorkspace(WorkspaceRef),
    FocusWindow(WindowRef),
    FocusOutput(String),
    RenameWorkspace {
        id: u64,
        name: Option<String>,
    },
    MoveWorkspaceToOutput {
        id: u64,
        connector: String,
    },
    ReorderWorkspace {
        id: u64,
        index: u8,
    },
    MoveWindowToWorkspace {
        window: u64,
        workspace: WorkspaceRef,
    },
    CloseWindow {
        id: u64,
    },
}

pub struct Compositor {
    backend: Backend,
    status: Publisher<CompositorStatus>,
    workspaces: Publisher<CompositorWorkspaces>,
    windows: Publisher<CompositorWindows>,
    outputs: Publisher<CompositorOutputs>,
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
    const TOPICS: &'static [&'static str] = &[
        CompositorStatus::NAME,
        CompositorWorkspaces::NAME,
        CompositorWindows::NAME,
        CompositorOutputs::NAME,
    ];
    const METHODS: &'static [&'static str] = &[
        FocusWorkspace::NAME,
        FocusWindow::NAME,
        FocusOutput::NAME,
        RenameWorkspace::NAME,
        MoveWorkspaceToOutput::NAME,
        ReorderWorkspace::NAME,
        MoveWindowToWorkspace::NAME,
        CloseWindow::NAME,
    ];

    type Config = NoConfig;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

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

    fn decode(method: &str, args: Value) -> Result<Self::Command, CallError> {
        match method {
            FocusWorkspace::NAME => {
                let FocusWorkspace { target } = decode_args(args)?;
                Ok(Command::FocusWorkspace(target))
            }
            FocusWindow::NAME => {
                let FocusWindow { target } = decode_args(args)?;
                Ok(Command::FocusWindow(target))
            }
            FocusOutput::NAME => {
                let FocusOutput { connector } = decode_args(args)?;
                Ok(Command::FocusOutput(connector))
            }
            RenameWorkspace::NAME => {
                let RenameWorkspace { id, name } = decode_args(args)?;
                Ok(Command::RenameWorkspace { id, name })
            }
            MoveWorkspaceToOutput::NAME => {
                let MoveWorkspaceToOutput { id, connector } = decode_args(args)?;
                Ok(Command::MoveWorkspaceToOutput { id, connector })
            }
            ReorderWorkspace::NAME => {
                let ReorderWorkspace { id, index } = decode_args(args)?;
                Ok(Command::ReorderWorkspace { id, index })
            }
            MoveWindowToWorkspace::NAME => {
                let MoveWindowToWorkspace { window, workspace } = decode_args(args)?;
                Ok(Command::MoveWindowToWorkspace { window, workspace })
            }
            CloseWindow::NAME => {
                let CloseWindow { id } = decode_args(args)?;
                Ok(Command::CloseWindow { id })
            }
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        let backend = detect_compositor();
        tracing::debug!(compositor = backend.name(), "starting compositor service");

        let mut service = Self {
            status: ctx.publisher::<CompositorStatus>(),
            workspaces: ctx.publisher::<CompositorWorkspaces>(),
            windows: ctx.publisher::<CompositorWindows>(),
            outputs: ctx.publisher::<CompositorOutputs>(),
            state: None,
            attempt: 0,
            backend,
        };

        service.status.set(CompositorStatus {
            name: service.backend.name().to_owned(),
            capabilities: capabilities(service.backend.capabilities()),
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
            Input::Command(command, responder) => self.dispatch(command, responder).await,
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

        self.workspaces.set(CompositorWorkspaces { workspaces });
        self.windows.set(CompositorWindows { windows });
        self.outputs.set(CompositorOutputs { outputs });
    }

    async fn dispatch(&self, command: Command, responder: Responder) {
        let outcome = match command {
            Command::FocusWorkspace(target) => {
                self.backend.focus_workspace(workspace_target(target)).await
            }
            Command::FocusWindow(target) => self.backend.focus_window(window_target(target)).await,
            Command::FocusOutput(connector) => self.backend.focus_output(&connector).await,
            Command::RenameWorkspace { id, name } => {
                self.backend
                    .rename_workspace(WorkspaceId(id), name.as_deref())
                    .await
            }
            Command::MoveWorkspaceToOutput { id, connector } => {
                self.backend
                    .move_workspace_to_output(WorkspaceId(id), &connector)
                    .await
            }
            Command::ReorderWorkspace { id, index } => {
                self.backend.reorder_workspace(WorkspaceId(id), index).await
            }
            Command::MoveWindowToWorkspace { window, workspace } => {
                self.backend
                    .move_window_to_workspace(WindowId(window), workspace_target(workspace))
                    .await
            }
            Command::CloseWindow { id } => self.backend.close_window(WindowId(id)).await,
        };

        match outcome {
            Ok(()) => responder.ok(()),
            Err(error) => responder.fail(CallError::new(code(&error), error.to_string())),
        }
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
            match state.windows.iter_mut().find(|it| it.id == window.id) {
                Some(found) => *found = window,
                None => state.windows.push(window),
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

fn window_target(reference: WindowRef) -> WindowTarget {
    match reference {
        WindowRef::Id { id } => WindowTarget::Id(WindowId(id)),
        WindowRef::Next => WindowTarget::Next,
        WindowRef::Prev => WindowTarget::Prev,
    }
}

fn code(error: &CompositorError) -> ErrorCode {
    match error {
        CompositorError::Unsupported(_) | CompositorError::Unavailable(_) => ErrorCode::Unsupported,
        CompositorError::Connect { .. } | CompositorError::Closed => ErrorCode::Unavailable,
        CompositorError::Refused(_) => ErrorCode::InvalidArgs,
        CompositorError::Protocol(_) => ErrorCode::Internal,
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
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Compositor>();
    }

    #[test]
    fn a_name_the_service_does_not_declare_is_refused() {
        let error = Compositor::decode("compositor.explode", Value::Null).expect_err("refused");
        assert_eq!(error.code, ErrorCode::UnknownCommand);
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
        let refusal = CallError::new(
            code(&CompositorError::Unavailable("reorder a workspace")),
            "x",
        );
        assert_eq!(refusal.code, ErrorCode::Unsupported);
        assert!(
            !refusal.retryable,
            "retrying cannot make a compositor grow the feature"
        );

        let gone = CallError::new(code(&CompositorError::Closed), "x");
        assert!(gone.retryable);
    }
}
