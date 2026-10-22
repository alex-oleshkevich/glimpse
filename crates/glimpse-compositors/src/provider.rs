use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use glimpse_dbus::Exported;
use tokio::sync::watch;
use zbus::Connection;
use zbus::object_server::InterfaceRef;
use zbus::zvariant::{OwnedValue, Value};

use crate::backend::Backend;
use crate::contract::{BUS_NAME, Error, Item, OBJECT_PATH, key};
use crate::event::{Event, Resync};
use crate::merge;
use crate::model::{Cast, Output, Snapshot, Window, WindowId, Workspace, WorkspaceId};
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
    let object = Object {
        backend: backend.clone(),
        state,
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
    follow(&object, &*backend, events, &updates).await;
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
) {
    let (mut structure, mut keyboard) = (false, false);
    while let Some(event) = events.next().await {
        let mut next = updates.borrow().clone();
        match merge::apply(&mut next.snapshot, event) {
            Some(Resync::Keyboard) => keyboard = true,
            Some(Resync::Structure | Resync::Outputs) => structure = true,
            None => {}
        }
        if structure {
            match backend.snapshot().await {
                Ok(snapshot) => {
                    next.snapshot = snapshot;
                    structure = false;
                }
                Err(error) => tracing::warn!(%error, "compositor snapshot refetch failed"),
            }
        }
        if keyboard {
            keyboard = !refetch_keyboard(backend, &mut next).await;
        }
        let changed = changed(&updates.borrow(), &next);
        updates.send_replace(next);
        if let Err(error) = publish(object, changed).await {
            tracing::warn!(%error, "compositor change signal failed");
        }
    }
    tracing::info!("compositor event stream ended");
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

struct Changed {
    outputs: bool,
    workspaces: bool,
    windows: bool,
    focus: bool,
    keyboard: bool,
    casts: bool,
    lock: bool,
}

fn changed(before: &Served, after: &Served) -> Changed {
    let (old, new) = (&before.snapshot, &after.snapshot);
    Changed {
        outputs: old.outputs != new.outputs,
        workspaces: old.workspaces != new.workspaces,
        windows: old.windows != new.windows,
        focus: old.focus != new.focus,
        keyboard: old.keyboard != new.keyboard,
        casts: old.casts != new.casts,
        lock: before.session_lock_restore != after.session_lock_restore,
    }
}

async fn publish<B: Backend>(
    object: &InterfaceRef<Object<B>>,
    changed: Changed,
) -> zbus::Result<()> {
    let emitter = object.signal_emitter();
    let object = object.get().await;
    if changed.outputs {
        object.outputs_changed(emitter).await?;
    }
    if changed.workspaces {
        object.workspaces_changed(emitter).await?;
    }
    if changed.windows {
        object.windows_changed(emitter).await?;
    }
    if changed.focus {
        object.focus_changed(emitter).await?;
    }
    if changed.keyboard {
        object.keyboard_changed(emitter).await?;
    }
    if changed.casts {
        object.casts_changed(emitter).await?;
    }
    if changed.lock {
        object.session_lock_restore_changed(emitter).await?;
    }
    Ok(())
}

struct Object<B> {
    backend: Arc<B>,
    state: watch::Receiver<Served>,
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
        let state = self.state.borrow();
        let focus = &state.snapshot.focus;
        item([
            (key::OUTPUT, maybe(focus.output.clone())),
            (key::WINDOW, maybe(focus.window.map(|id| id.0))),
        ])
    }

    #[zbus(property)]
    fn keyboard(&self) -> Item {
        let state = self.state.borrow();
        let keyboard = &state.snapshot.keyboard;
        let list = |names: &Vec<String>| (!names.is_empty()).then(|| Value::from(names.clone()));
        item([
            (key::NAMES, list(&keyboard.names)),
            (key::CODES, list(&keyboard.codes)),
            (key::CURRENT, maybe(keyboard.current)),
        ])
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
    fn session_lock_restore(&self) -> zbus::fdo::Result<bool> {
        self.state.borrow().session_lock_restore.ok_or_else(|| {
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
        self.backend.switch_keyboard_layout(index).await
    }

    async fn step_keyboard_layout(&self, forward: bool) -> Result<(), Error> {
        self.backend.step_keyboard_layout(forward).await
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
