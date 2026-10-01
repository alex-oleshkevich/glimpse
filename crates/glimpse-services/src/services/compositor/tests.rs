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
        size: None,
        rect: None,
        output: None,
    }
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

mod casts;
mod commands;
mod focus;
mod outputs;
mod snapshot;
mod workspaces;
