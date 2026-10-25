use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::stream::{self, BoxStream, StreamExt};
use glimpse_compositors::{
    backend::Backend,
    contract::{BUS_NAME, Capability, Error as ProviderError},
    event::Event as BackendEvent,
    model::{
        Cast, Focus, KeyboardLayouts, Logical, Mode, Rect, Snapshot, Window, WindowId, Workspace,
        WorkspaceId,
    },
    provider::{ServeError, serve},
};
use glimpse_dbus::{Buses, testing::PrivateBus};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::*;
use crate::service::{Running, ServiceState};

pub(crate) trait Caps: Send + Sync + 'static {
    const SET: &'static [Capability];
}

pub(crate) struct Everything;
pub(crate) struct Nothing;
pub(crate) struct RenameOnly;

impl Caps for Everything {
    const SET: &'static [Capability] = &[
        Capability::RenameWorkspace,
        Capability::OutputPower,
        Capability::BlankScreens,
    ];
}

impl Caps for Nothing {
    const SET: &'static [Capability] = &[];
}

impl Caps for RenameOnly {
    const SET: &'static [Capability] = &[Capability::RenameWorkspace];
}

#[derive(Default)]
pub(crate) struct Shared {
    snapshot: Mutex<Snapshot>,
    calls: Mutex<Vec<String>>,
    recorded: watch::Sender<usize>,
    snapshots: Mutex<usize>,
}

impl Shared {
    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }

    pub(crate) fn recorded(&self) -> watch::Receiver<usize> {
        self.recorded.subscribe()
    }

    pub(crate) fn snapshots(&self) -> usize {
        *self.snapshots.lock().expect("snapshots")
    }
}

pub(crate) struct Fake<C: Caps> {
    shared: Arc<Shared>,
    events: tokio::sync::broadcast::WeakSender<BackendEvent>,
    caps: PhantomData<fn() -> C>,
}

impl<C: Caps> Fake<C> {
    fn record(&self, call: String) -> Result<(), ProviderError> {
        let mut calls = self.shared.calls.lock().expect("calls");
        calls.push(call);
        self.shared.recorded.send_replace(calls.len());
        Ok(())
    }
}

impl<C: Caps> Backend for Fake<C> {
    const NAME: &'static str = "fake";
    const CAPABILITIES: &'static [Capability] = C::SET;

    async fn snapshot(&self) -> Result<Snapshot, ProviderError> {
        *self.shared.snapshots.lock().expect("snapshots") += 1;
        Ok(self.shared.snapshot.lock().expect("snapshot").clone())
    }

    async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, ProviderError> {
        Ok(self
            .shared
            .snapshot
            .lock()
            .expect("snapshot")
            .keyboard
            .clone())
    }

    async fn session_lock_restore(&self) -> Result<Option<bool>, ProviderError> {
        Ok(None)
    }

    async fn events(&self) -> Result<BoxStream<'static, BackendEvent>, ProviderError> {
        let events = self
            .events
            .upgrade()
            .ok_or_else(|| ProviderError::Failed("events closed".to_owned()))?
            .subscribe();
        Ok(stream::unfold(events, |mut events| async move {
            Some((events.recv().await.ok()?, events))
        })
        .boxed())
    }

    async fn focus_workspace(&self, id: WorkspaceId) -> Result<(), ProviderError> {
        self.record(format!("focus_workspace {}", id.0))
    }

    async fn step_workspace(&self, forward: bool) -> Result<(), ProviderError> {
        self.record(format!("step_workspace {forward}"))
    }

    async fn rename_workspace(&self, id: WorkspaceId, name: String) -> Result<(), ProviderError> {
        self.record(format!("rename_workspace {} {name}", id.0))
    }

    async fn focus_window(&self, id: WindowId) -> Result<(), ProviderError> {
        self.record(format!("focus_window {}", id.0))
    }

    async fn step_window(&self, forward: bool) -> Result<(), ProviderError> {
        self.record(format!("step_window {forward}"))
    }

    async fn switch_keyboard_layout(&self, index: u32) -> Result<(), ProviderError> {
        self.record(format!("switch_keyboard_layout {index}"))?;
        self.shared
            .snapshot
            .lock()
            .expect("snapshot")
            .keyboard
            .current = Some(index);
        Ok(())
    }

    async fn step_keyboard_layout(&self, forward: bool) -> Result<(), ProviderError> {
        self.record(format!("step_keyboard_layout {forward}"))?;
        let mut snapshot = self.shared.snapshot.lock().expect("snapshot");
        let keyboard = &mut snapshot.keyboard;
        let count = keyboard.names.len().max(keyboard.codes.len()) as u32;
        keyboard.current = keyboard.current.filter(|_| count > 0).map(|index| {
            if forward {
                (index + 1) % count
            } else {
                (index + count - 1) % count
            }
        });
        Ok(())
    }

    async fn set_output_enabled(
        &self,
        connector: String,
        enabled: bool,
    ) -> Result<(), ProviderError> {
        self.record(format!("set_output_enabled {connector} {enabled}"))
    }

    async fn power_off_monitors(&self) -> Result<(), ProviderError> {
        self.record("power_off_monitors".to_owned())
    }

    async fn stop_screencast(&self, session: u64) -> Result<(), ProviderError> {
        self.record(format!("stop_screencast {session}"))
    }

    async fn screenshot_window(&self, id: WindowId, path: PathBuf) -> Result<(), ProviderError> {
        self.record(format!("screenshot_window {} {}", id.0, path.display()))
    }

    async fn window_geometries(&self) -> Result<HashMap<WindowId, Rect>, ProviderError> {
        Ok(HashMap::new())
    }

    async fn probe(&self) -> bool {
        true
    }
}

pub(crate) struct Served {
    pub(crate) shared: Arc<Shared>,
    pub(crate) events: tokio::sync::broadcast::Sender<BackendEvent>,
    pub(crate) task: JoinHandle<ServeError>,
}

pub(crate) async fn serve_on<C: Caps>(bus: &PrivateBus, snapshot: Snapshot) -> Served {
    let (events, _) = tokio::sync::broadcast::channel(128);
    let shared = Arc::new(Shared {
        snapshot: Mutex::new(snapshot),
        ..Shared::default()
    });
    let fake = Fake::<C> {
        shared: shared.clone(),
        events: events.downgrade(),
        caps: PhantomData,
    };
    let task = tokio::spawn(serve(bus.connection().await, fake));
    let connection = bus.connection().await;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.expect("dbus");
    let name = zbus::names::BusName::try_from(BUS_NAME).expect("a bus name");
    let mut owned = false;
    for _ in 0..500 {
        if dbus
            .name_has_owner(name.clone())
            .await
            .expect("an owner query")
        {
            owned = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(owned, "the fake provider never took the bus name");
    Served {
        shared,
        events,
        task,
    }
}

pub(crate) struct Mirror {
    pub(crate) handle: CompositorHandle,
    pub(crate) provider: glimpse_compositors::client::CompositorProvider,
    pub(crate) _running: Running<Compositor>,
}

pub(crate) async fn mirror_on(bus: &PrivateBus) -> Mirror {
    let provider = glimpse_compositors::client::CompositorProvider::start(bus.connection().await);
    let (running, handle) = Running::<Compositor>::spawn(
        &glimpse_config::Config::default(),
        Buses::unavailable("no bus in tests"),
        Dependencies {
            provider: ProviderHandle(provider.handle()),
        },
    );
    Mirror {
        handle,
        provider,
        _running: running,
    }
}

pub(crate) async fn until<T: Clone>(
    receiver: &mut watch::Receiver<T>,
    what: &str,
    mut done: impl FnMut(&T) -> bool,
) -> T {
    match tokio::time::timeout(
        Duration::from_secs(10),
        receiver.wait_for(|value| done(value)),
    )
    .await
    {
        Ok(Ok(value)) => value.clone(),
        _ => panic!("timed out waiting for {what}"),
    }
}

pub(crate) struct Rig {
    pub(crate) bus: PrivateBus,
    pub(crate) served: Served,
    pub(crate) mirror: Mirror,
    pub(crate) state: watch::Receiver<CompositorState>,
}

impl Rig {
    pub(crate) async fn start(snapshot: Snapshot) -> Self {
        Self::with::<Everything>(snapshot).await
    }

    pub(crate) async fn with<C: Caps>(snapshot: Snapshot) -> Self {
        let bus = PrivateBus::start();
        let served = serve_on::<C>(&bus, snapshot).await;
        let mirror = mirror_on(&bus).await;
        let mut state = mirror.handle.subscribe();
        until(&mut state, "the first served state", |state| {
            state.workspaces.is_some()
        })
        .await;
        Self {
            bus,
            served,
            mirror,
            state,
        }
    }

    pub(crate) fn send(&self, event: BackendEvent) {
        glimpse_compositors::merge::apply(
            &mut self.served.shared.snapshot.lock().expect("snapshot"),
            event.clone(),
        );
        self.served.events.send(event).expect("the fake is alive");
    }

    pub(crate) async fn until(&mut self, what: &str, done: impl FnMut(&CompositorState) -> bool) {
        until(&mut self.state, what, done).await;
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        self.served.shared.calls()
    }

    pub(crate) fn snapshots(&self) -> usize {
        self.served.shared.snapshots()
    }
}

pub(crate) fn snapshot_of(workspaces: Vec<Workspace>, windows: Vec<Window>) -> Snapshot {
    Snapshot {
        workspaces,
        windows,
        ..Snapshot::default()
    }
}

pub(crate) fn workspace(id: u64, index: Option<u8>, output: &str) -> Workspace {
    Workspace {
        id: WorkspaceId(id),
        index,
        name: None,
        output: Some(output.to_owned()),
        active: false,
    }
}

pub(crate) fn window(id: u64, workspace: u64) -> Window {
    Window {
        id: WindowId(id),
        title: None,
        app_id: None,
        pid: None,
        workspace: Some(WorkspaceId(workspace)),
        floating: false,
        urgent: false,
        order: None,
    }
}

pub(crate) fn output(connector: &str, enabled: bool) -> Output {
    Output {
        connector: connector.to_owned(),
        enabled,
        description: None,
        make: Some("Samsung".to_owned()),
        model: Some("ATNA60CL10-0 ".to_owned()),
        mode: None,
        logical: None,
    }
}

pub(crate) fn cast(id: u64, active: bool) -> Cast {
    Cast {
        id,
        active,
        output: None,
        session: None,
        pw_node: None,
    }
}

pub(crate) fn focus(output: Option<&str>, window: Option<u64>) -> Focus {
    Focus {
        output: output.map(str::to_owned),
        window: window.map(WindowId),
    }
}

pub(crate) fn health_of(rig: &Mirror) -> ServiceState {
    rig.handle.health().borrow().clone()
}

mod casts;
mod commands;
mod focus;
mod outputs;
mod snapshot;
mod workspaces;
