use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use futures_util::stream::BoxStream;
use futures_util::{FutureExt, StreamExt};
use glimpse_dbus::Exported;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Duration, Instant};
use zbus::Connection;
use zbus::object_server::InterfaceRef;
use zbus::zvariant::{OwnedValue, Value};

use crate::backend::Backend;
use crate::contract::{BUS_NAME, Error, Item, OBJECT_PATH, key};
use crate::event::{Event, Resync};
use crate::merge;
use crate::model::{
    Cast, Focus, KeyboardLayouts, Output, Snapshot, Window, WindowId, Workspace, WorkspaceId,
};
use crate::sanitize::NAME_CAP;

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("another provider owns {BUS_NAME}")]
    NameTaken,
    #[error("session bus: {0}")]
    Bus(zbus::Error),
    #[error("compositor: {0}")]
    Backend(Error),
    #[error("the compositor event stream ended")]
    Ended,
}

impl ServeError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NameTaken => 4,
            _ => 1,
        }
    }
}

pub async fn run<B: Backend>(probe: bool, backend: Option<B>, missing: &'static str) -> ExitCode {
    if probe {
        return match backend {
            Some(backend) if backend.probe().await => ExitCode::SUCCESS,
            _ => ExitCode::FAILURE,
        };
    }
    let Some(backend) = backend else {
        tracing::error!("{missing}");
        return ExitCode::FAILURE;
    };
    let connection = match Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            tracing::error!(%error, "cannot reach the session bus");
            return ExitCode::FAILURE;
        }
    };
    let ended = serve(connection, backend).await;
    tracing::error!(%ended, "provider ended");
    ExitCode::from(ended.exit_code())
}

pub async fn serve<B: Backend>(connection: Connection, backend: B) -> ServeError {
    let backend = Arc::new(backend);
    let (events, served) = match start(&*backend).await {
        Ok(started) => started,
        Err(error) => return ServeError::Backend(error),
    };
    let (updates, state) = watch::channel(served);
    let (keyboard, commands) = mpsc::channel(16);
    let object = Object {
        backend: backend.clone(),
        state,
        keyboard,
    };
    let exported = match Exported::start(
        connection.clone(),
        BUS_NAME,
        OBJECT_PATH,
        object,
        std::future::ready(()),
    )
    .await
    {
        Ok(exported) => exported,
        Err(zbus::Error::NameTaken) => return ServeError::NameTaken,
        Err(error) => return ServeError::Bus(error),
    };
    let object = match connection
        .object_server()
        .interface::<_, Object<B>>(OBJECT_PATH)
        .await
    {
        Ok(object) => object,
        Err(error) => return ServeError::Bus(error),
    };
    follow(&object, &*backend, events, &updates, commands).await;
    exported.shutdown().await;
    ServeError::Ended
}

#[derive(Debug, Clone, PartialEq)]
struct Served {
    snapshot: Snapshot,
    session_lock_restore: Option<bool>,
}

async fn start<B: Backend>(backend: &B) -> Result<(BoxStream<'static, Event>, Served), Error> {
    let events = backend.events().await?;
    let served = Served {
        snapshot: backend.snapshot().await?,
        session_lock_restore: backend.session_lock_restore().await?,
    };
    Ok((events, served))
}

async fn follow<B: Backend>(
    object: &InterfaceRef<Object<B>>,
    backend: &B,
    mut events: BoxStream<'static, Event>,
    updates: &watch::Sender<Served>,
    mut commands: mpsc::Receiver<KeyboardCommand>,
) {
    let mut pending = (false, false);
    let mut signals = Signals::default();
    loop {
        let due = signals.due();
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { break };
                update(object, backend, event, updates, &mut pending, &mut signals).await;
            }
            () = tokio::time::sleep_until(due.unwrap_or_else(Instant::now)), if due.is_some() => {
                let changed = signals.release(&updates.borrow());
                if let Err(error) = publish(object, changed).await {
                    tracing::warn!(%error, "compositor change signal failed");
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { break };
                for _ in 0..64 {
                    match events.next().now_or_never() {
                        Some(Some(event)) => update(object, backend, event, updates, &mut pending, &mut signals).await,
                        Some(None) => return,
                        None => break,
                    }
                }
                pending = (true, true);
                let result = keyboard_command(object, backend, command.action, &mut events, updates, &mut signals).await;
                if result.is_ok() {
                    pending = (false, false);
                }
                let _ = command.reply.send(result);
            }
        }
    }
    tracing::info!("compositor event stream ended");
}

enum KeyboardAction {
    Switch(u32),
    Step(bool),
}

struct KeyboardCommand {
    action: KeyboardAction,
    reply: oneshot::Sender<Result<Option<u32>, Error>>,
}

async fn keyboard_command<B: Backend>(
    object: &InterfaceRef<Object<B>>,
    backend: &B,
    action: KeyboardAction,
    events: &mut BoxStream<'static, Event>,
    updates: &watch::Sender<Served>,
    signals: &mut Signals,
) -> Result<Option<u32>, Error> {
    match action {
        KeyboardAction::Switch(index) => backend.switch_keyboard_layout(index).await?,
        KeyboardAction::Step(forward) => backend.step_keyboard_layout(forward).await?,
    }
    *events = backend.events().await?;
    let selected = match action {
        KeyboardAction::Switch(_) => None,
        KeyboardAction::Step(_) => {
            Some(backend.keyboard_layouts().await?.current.ok_or_else(|| {
                Error::Failed("the compositor did not report the selected layout".to_owned())
            })?)
        }
    };
    let mut next = updates.borrow().clone();
    next.snapshot = backend.snapshot().await?;
    if let Ok(restore) = backend.session_lock_restore().await {
        next.session_lock_restore = restore.or(next.session_lock_restore);
    }
    let previous = updates.send_replace(next);
    let mut changed = signals.settle(previous, &updates.borrow());
    changed.insert(
        "Keyboard",
        Value::from(keyboard(&updates.borrow().snapshot.keyboard)),
    );
    publish(object, changed)
        .await
        .map_err(|error| Error::Failed(error.to_string()))?;
    Ok(selected)
}

async fn update<B: Backend>(
    object: &InterfaceRef<Object<B>>,
    backend: &B,
    event: Event,
    updates: &watch::Sender<Served>,
    pending: &mut (bool, bool),
    signals: &mut Signals,
) {
    let mut next = updates.borrow().clone();
    match merge::apply(&mut next.snapshot, event) {
        Some(Resync::Keyboard) => pending.1 = true,
        Some(Resync::Structure | Resync::Outputs) => pending.0 = true,
        None => {}
    }
    if pending.0 {
        match backend.snapshot().await {
            Ok(snapshot) => {
                next.snapshot = snapshot;
                pending.0 = false;
            }
            Err(error) => tracing::warn!(%error, "compositor snapshot refetch failed"),
        }
    }
    if pending.1 {
        pending.1 = !refetch_keyboard(backend, &mut next).await;
    }
    let previous = updates.send_replace(next);
    let Some(changed) = signals.next(previous, &updates.borrow()) else {
        return;
    };
    if let Err(error) = publish(object, changed).await {
        tracing::warn!(%error, "compositor change signal failed");
    }
}

const TITLE_HOLD: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Signals {
    held: Option<(Served, Instant)>,
    windows_sent: Option<Instant>,
}

impl Signals {
    fn next(
        &mut self,
        previous: Served,
        current: &Served,
    ) -> Option<HashMap<&'static str, Value<'static>>> {
        let (base, due) = match self.held.take() {
            Some((base, due)) => (base, Some(due)),
            None => (previous, None),
        };
        if title_only(&base, current) {
            let ready = self
                .windows_sent
                .map(|sent| sent + TITLE_HOLD)
                .filter(|ready| *ready > Instant::now());
            if let Some(due) = due.or(ready) {
                self.held = Some((base, due));
                return None;
            }
        }
        Some(self.signalled(&base, current))
    }

    fn settle(
        &mut self,
        previous: Served,
        current: &Served,
    ) -> HashMap<&'static str, Value<'static>> {
        let base = self.held.take().map_or(previous, |(base, _)| base);
        self.signalled(&base, current)
    }

    fn release(&mut self, current: &Served) -> HashMap<&'static str, Value<'static>> {
        match self.held.take() {
            Some((base, _)) => self.signalled(&base, current),
            None => HashMap::new(),
        }
    }

    fn due(&self) -> Option<Instant> {
        self.held.as_ref().map(|(_, due)| *due)
    }

    fn signalled(
        &mut self,
        base: &Served,
        current: &Served,
    ) -> HashMap<&'static str, Value<'static>> {
        let changed = changed(base, current);
        if changed.contains_key("Windows") {
            self.windows_sent = Some(Instant::now());
        }
        changed
    }
}

fn title_only(before: &Served, after: &Served) -> bool {
    let (old, new) = (&before.snapshot, &after.snapshot);
    old.windows != new.windows
        && old.windows.len() == new.windows.len()
        && old.windows.iter().zip(&new.windows).all(|(old, new)| {
            let Window {
                id,
                title: _,
                app_id,
                pid,
                workspace,
                floating,
                urgent,
                order,
            } = old;
            *id == new.id
                && *app_id == new.app_id
                && *pid == new.pid
                && *workspace == new.workspace
                && *floating == new.floating
                && *urgent == new.urgent
                && *order == new.order
        })
        && old.outputs == new.outputs
        && old.workspaces == new.workspaces
        && old.focus == new.focus
        && old.keyboard == new.keyboard
        && old.casts == new.casts
        && before.session_lock_restore == after.session_lock_restore
}

async fn refetch_keyboard<B: Backend>(backend: &B, next: &mut Served) -> bool {
    let layouts = match backend.keyboard_layouts().await {
        Ok(layouts) => {
            next.snapshot.keyboard = layouts;
            true
        }
        Err(error) => {
            tracing::warn!(%error, "keyboard layout refetch failed");
            false
        }
    };
    if next.session_lock_restore.is_none() {
        return layouts;
    }
    match backend.session_lock_restore().await {
        Ok(restore) => {
            next.session_lock_restore = restore.or(next.session_lock_restore);
            layouts
        }
        Err(error) => {
            tracing::warn!(%error, "session lock restore refetch failed");
            false
        }
    }
}

fn changed(before: &Served, after: &Served) -> HashMap<&'static str, Value<'static>> {
    let (old, new) = (&before.snapshot, &after.snapshot);
    let mut changed = HashMap::new();
    if old.outputs != new.outputs {
        changed.insert(
            "Outputs",
            Value::from(new.outputs.iter().map(output).collect::<Vec<_>>()),
        );
    }
    if old.workspaces != new.workspaces {
        changed.insert(
            "Workspaces",
            Value::from(new.workspaces.iter().map(workspace).collect::<Vec<_>>()),
        );
    }
    if old.windows != new.windows {
        changed.insert(
            "Windows",
            Value::from(new.windows.iter().map(window).collect::<Vec<_>>()),
        );
    }
    if old.focus != new.focus {
        changed.insert("Focus", Value::from(focus(&new.focus)));
    }
    if old.keyboard != new.keyboard {
        changed.insert("Keyboard", Value::from(keyboard(&new.keyboard)));
    }
    if old.casts != new.casts {
        changed.insert(
            "Casts",
            Value::from(new.casts.iter().map(cast).collect::<Vec<_>>()),
        );
    }
    if before.session_lock_restore != after.session_lock_restore
        && let Some(restore) = after.session_lock_restore
    {
        changed.insert("SessionLockRestore", Value::from(restore));
    }
    changed
}

async fn publish<B: Backend>(
    object: &InterfaceRef<Object<B>>,
    changed: HashMap<&str, Value<'_>>,
) -> zbus::Result<()> {
    if changed.is_empty() {
        return Ok(());
    }
    zbus::fdo::Properties::properties_changed(
        object.signal_emitter(),
        <Object<B> as zbus::object_server::Interface>::name(),
        changed,
        [].as_slice().into(),
    )
    .await
}

struct Object<B> {
    backend: Arc<B>,
    state: watch::Receiver<Served>,
    keyboard: mpsc::Sender<KeyboardCommand>,
}

#[zbus::interface(name = "me.aresa.Glimpse.Compositor1")]
impl<B: Backend> Object<B> {
    #[zbus(property(emits_changed_signal = "const"))]
    fn name(&self) -> String {
        B::NAME.to_owned()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn capabilities(&self) -> Vec<String> {
        B::CAPABILITIES
            .iter()
            .map(|capability| capability.as_str().to_owned())
            .collect()
    }

    #[zbus(property)]
    fn outputs(&self) -> Vec<Item> {
        self.state
            .borrow()
            .snapshot
            .outputs
            .iter()
            .map(output)
            .collect()
    }

    #[zbus(property)]
    fn workspaces(&self) -> Vec<Item> {
        let state = self.state.borrow();
        state.snapshot.workspaces.iter().map(workspace).collect()
    }

    #[zbus(property)]
    fn windows(&self) -> Vec<Item> {
        self.state
            .borrow()
            .snapshot
            .windows
            .iter()
            .map(window)
            .collect()
    }

    #[zbus(property)]
    fn focus(&self) -> Item {
        focus(&self.state.borrow().snapshot.focus)
    }

    #[zbus(property)]
    fn keyboard(&self) -> Item {
        keyboard(&self.state.borrow().snapshot.keyboard)
    }

    #[zbus(property)]
    fn casts(&self) -> Vec<Item> {
        self.state
            .borrow()
            .snapshot
            .casts
            .iter()
            .map(cast)
            .collect()
    }

    #[zbus(property)]
    async fn session_lock_restore(&self) -> zbus::fdo::Result<bool> {
        self.backend
            .session_lock_restore()
            .await
            .unwrap_or_else(|_| self.state.borrow().session_lock_restore)
            .ok_or_else(|| {
                zbus::fdo::Error::UnknownProperty("this compositor has no such setting".to_owned())
            })
    }

    async fn focus_workspace(&self, id: u64) -> Result<(), Error> {
        let id = self.workspace(id)?;
        self.backend.focus_workspace(id).await
    }

    async fn step_workspace(&self, forward: bool) -> Result<(), Error> {
        self.backend.step_workspace(forward).await
    }

    async fn rename_workspace(&self, id: u64, name: String) -> Result<(), Error> {
        let id = self.workspace(id)?;
        if name.chars().count() > NAME_CAP || name.chars().any(char::is_control) {
            return Err(Error::Refused(format!(
                "a workspace name is at most {NAME_CAP} characters and has no control characters"
            )));
        }
        self.backend.rename_workspace(id, name).await
    }

    async fn focus_window(&self, id: u64) -> Result<(), Error> {
        let id = self.window(id)?;
        self.backend.focus_window(id).await
    }

    async fn step_window(&self, forward: bool) -> Result<(), Error> {
        self.backend.step_window(forward).await
    }

    async fn switch_keyboard_layout(&self, index: u32) -> Result<(), Error> {
        self.keyboard_command(KeyboardAction::Switch(index))
            .await
            .map(|_| ())
    }

    async fn step_keyboard_layout(&self, forward: bool) -> Result<u32, Error> {
        self.keyboard_command(KeyboardAction::Step(forward))
            .await?
            .ok_or_else(|| {
                Error::Failed("the compositor did not report the selected layout".to_owned())
            })
    }

    async fn set_output_enabled(&self, connector: String, enabled: bool) -> Result<(), Error> {
        self.known(
            |state| {
                state
                    .outputs
                    .iter()
                    .any(|output| output.connector == connector)
            },
            "output",
        )?;
        self.backend.set_output_enabled(connector, enabled).await
    }

    async fn power_off_monitors(&self) -> Result<(), Error> {
        self.backend.power_off_monitors().await
    }

    async fn stop_screencast(&self, session: u64) -> Result<(), Error> {
        self.known(
            |state| state.casts.iter().any(|cast| cast.session == Some(session)),
            "screencast session",
        )?;
        self.backend.stop_screencast(session).await
    }

    async fn screenshot_window(&self, id: u64, path: String) -> Result<(), Error> {
        let id = self.window(id)?;
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(Error::Refused(
                "the screenshot path is not absolute".to_owned(),
            ));
        }
        self.backend.screenshot_window(id, path).await
    }

    async fn window_geometries(&self) -> Result<HashMap<u64, Item>, Error> {
        let geometries = self.backend.window_geometries().await?;
        Ok(geometries
            .into_iter()
            .map(|(id, rect)| {
                let geometry = item([
                    (key::X, some(rect.x)),
                    (key::Y, some(rect.y)),
                    (key::WIDTH, some(rect.width)),
                    (key::HEIGHT, some(rect.height)),
                ]);
                (id.0, geometry)
            })
            .collect())
    }
}

impl<B: Backend> Object<B> {
    async fn keyboard_command(&self, action: KeyboardAction) -> Result<Option<u32>, Error> {
        let (reply, result) = oneshot::channel();
        self.keyboard
            .send(KeyboardCommand { action, reply })
            .await
            .map_err(|_| Error::Unavailable("the compositor event stream ended".to_owned()))?;
        result
            .await
            .map_err(|_| Error::Unavailable("the compositor event stream ended".to_owned()))?
    }

    fn known(&self, found: impl FnOnce(&Snapshot) -> bool, what: &str) -> Result<(), Error> {
        match found(&self.state.borrow().snapshot) {
            true => Ok(()),
            false => Err(Error::Refused(format!("there is no such {what}"))),
        }
    }

    fn workspace(&self, id: u64) -> Result<WorkspaceId, Error> {
        let id = WorkspaceId(id);
        self.known(
            |state| state.workspaces.iter().any(|it| it.id == id),
            "workspace",
        )?;
        Ok(id)
    }

    fn window(&self, id: u64) -> Result<WindowId, Error> {
        let id = WindowId(id);
        self.known(|state| state.windows.iter().any(|it| it.id == id), "window")?;
        Ok(id)
    }
}

fn some(value: impl Into<Value<'static>>) -> Option<Value<'static>> {
    Some(value.into())
}

fn maybe(value: Option<impl Into<Value<'static>>>) -> Option<Value<'static>> {
    value.map(Into::into)
}

fn item<const N: usize>(entries: [(&str, Option<Value<'static>>); N]) -> Item {
    entries
        .into_iter()
        .filter_map(|(key, value)| Some((key.to_owned(), OwnedValue::try_from(value?).ok()?)))
        .collect()
}

fn focus(focus: &Focus) -> Item {
    item([
        (key::OUTPUT, maybe(focus.output.clone())),
        (key::WINDOW, maybe(focus.window.map(|id| id.0))),
    ])
}

fn keyboard(keyboard: &KeyboardLayouts) -> Item {
    let list = |names: &Vec<String>| (!names.is_empty()).then(|| Value::from(names.clone()));
    item([
        (key::NAMES, list(&keyboard.names)),
        (key::CODES, list(&keyboard.codes)),
        (key::CURRENT, maybe(keyboard.current)),
    ])
}

fn output(output: &Output) -> Item {
    item([
        (key::CONNECTOR, some(output.connector.clone())),
        (key::ENABLED, some(output.enabled)),
        (key::DESCRIPTION, maybe(output.description.clone())),
        (key::MAKE, maybe(output.make.clone())),
        (key::MODEL, maybe(output.model.clone())),
        (
            key::MODE,
            maybe(
                output
                    .mode
                    .map(|mode| (mode.width, mode.height, mode.refresh_mhz)),
            ),
        ),
        (
            key::LOGICAL,
            maybe(output.logical.map(|logical| {
                (
                    logical.x,
                    logical.y,
                    logical.width,
                    logical.height,
                    logical.scale,
                )
            })),
        ),
    ])
}

fn workspace(workspace: &Workspace) -> Item {
    item([
        (key::ID, some(workspace.id.0)),
        (key::INDEX, maybe(workspace.index)),
        (key::NAME, maybe(workspace.name.clone())),
        (key::OUTPUT, maybe(workspace.output.clone())),
        (key::ACTIVE, some(workspace.active)),
    ])
}

fn window(window: &Window) -> Item {
    item([
        (key::ID, some(window.id.0)),
        (key::TITLE, maybe(window.title.clone())),
        (key::APP_ID, maybe(window.app_id.clone())),
        (key::PID, maybe(window.pid)),
        (key::WORKSPACE, maybe(window.workspace.map(|id| id.0))),
        (key::FLOATING, some(window.floating)),
        (key::URGENT, some(window.urgent)),
        (key::ORDER, maybe(window.order)),
    ])
}

fn cast(cast: &Cast) -> Item {
    item([
        (key::ID, some(cast.id)),
        (key::ACTIVE, some(cast.active)),
        (key::OUTPUT, maybe(cast.output.clone())),
        (key::SESSION, maybe(cast.session)),
        (key::PW_NODE, maybe(cast.pw_node)),
    ])
}

#[cfg(test)]
mod tests;
