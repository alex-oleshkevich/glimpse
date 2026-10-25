use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use glimpse_compositors::client::{CompositorProvider, CompositorState, Handle};
use glimpse_compositors::contract::{BUS_NAME, Capability, Error};
use glimpse_compositors::model::{self, WindowId};
use glimpse_dbus::{DEADLINE, PENDING, ProviderError};
use glimpse_shot::ports::{self, Screens, Window};
use glimpse_wayland::capture::{self, CaptureError, Frame};
use zbus::DBusError;

use crate::headless::Layout;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);
const REASON_CAP: usize = 240;

static SESSION: OnceLock<Compositor> = OnceLock::new();

pub struct Compositor {
    provider: CompositorProvider,
    runtime: tokio::runtime::Runtime,
}

impl Compositor {
    pub fn layout(&self) -> anyhow::Result<Option<Layout>> {
        self.runtime.block_on(read_layout(&self.provider))
    }

    pub fn screenshots(&self) -> bool {
        Handle(self.provider.handle()).supports(Capability::WindowScreenshot)
    }

    pub fn screenshot(&self, id: u64, path: &Path) -> Result<(), String> {
        self.runtime
            .block_on(Handle(self.provider.handle()).screenshot_window(WindowId(id), path))
            .map_err(|error| capped(error.description().unwrap_or_default()))
    }
}

pub fn session() -> anyhow::Result<Option<&'static Compositor>> {
    if let Some(link) = SESSION.get() {
        return Ok(Some(link));
    }
    let Some(link) = connect()? else {
        return Ok(None);
    };
    Ok(Some(SESSION.get_or_init(|| link)))
}

pub fn layout() -> anyhow::Result<Option<Layout>> {
    match session()? {
        Some(link) => link.layout(),
        None => Ok(None),
    }
}

fn runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?)
}

fn connect() -> anyhow::Result<Option<Compositor>> {
    let runtime = runtime()?;
    let provider = runtime.block_on(async {
        let Ok(connection) = zbus::Connection::session().await else {
            return None;
        };
        owned(connection).await
    });
    Ok(provider.map(|provider| Compositor { provider, runtime }))
}

#[cfg(test)]
pub async fn read_on(connection: &zbus::Connection) -> anyhow::Result<Option<Layout>> {
    let Some(provider) = owned(connection.clone()).await else {
        return Ok(None);
    };
    read_layout(&provider).await
}

async fn owned(connection: zbus::Connection) -> Option<CompositorProvider> {
    if !has_owner(&connection).await {
        return None;
    }
    Some(CompositorProvider::start(connection))
}

async fn has_owner(connection: &zbus::Connection) -> bool {
    let Ok(dbus) = zbus::fdo::DBusProxy::new(connection).await else {
        return false;
    };
    let Ok(name) = zbus::names::BusName::try_from(BUS_NAME) else {
        return false;
    };
    matches!(dbus.name_has_owner(name).await, Ok(true))
}

async fn read_layout(provider: &CompositorProvider) -> anyhow::Result<Option<Layout>> {
    let handle = Handle(provider.handle());
    let Some(state) = settled(&handle).await? else {
        return Ok(None);
    };
    let Some(rects) = geometries(&handle, &state).await else {
        return Ok(None);
    };
    Ok(Some(project(&state, &rects)))
}

async fn settled(handle: &Handle) -> anyhow::Result<Option<CompositorState>> {
    let mut watch = handle.subscribe();
    let ready = match tokio::time::timeout(
        DEADLINE,
        watch.wait_for(|state| state.unavailable.as_deref() != Some(PENDING)),
    )
    .await
    {
        Ok(ready) => ready,
        Err(_) => {
            return Err(anyhow::anyhow!(
                "{}",
                Error::timed_out().description().unwrap_or_default()
            ));
        }
    };
    let Ok(ready) = ready else {
        return Ok(None);
    };
    let state = ready.clone();
    drop(ready);
    Ok(state.unavailable.is_none().then_some(state))
}

async fn geometries(
    handle: &Handle,
    state: &CompositorState,
) -> Option<HashMap<WindowId, model::Rect>> {
    if !state.capabilities.contains(&Capability::WindowGeometry) {
        return Some(HashMap::new());
    }
    match handle.window_geometries().await {
        Ok(rects) => Some(rects),
        Err(Error::Unavailable(_)) => None,
        Err(_) => Some(HashMap::new()),
    }
}

fn project(state: &CompositorState, rects: &HashMap<WindowId, model::Rect>) -> Layout {
    let focused = state.focus.window;
    Layout {
        outputs: state.outputs.clone(),
        workspaces: state.workspaces.clone(),
        focused_output: state.focus.output.clone(),
        windows: state
            .windows
            .iter()
            .map(|window| {
                let output = window
                    .workspace
                    .and_then(|id| state.workspaces.iter().find(|workspace| workspace.id == id))
                    .and_then(|workspace| workspace.output.clone());
                let origin = output.as_deref().and_then(|name| {
                    state
                        .outputs
                        .iter()
                        .find(|candidate| candidate.connector == name)
                        .and_then(|output| output.logical)
                });
                let rect = rects.get(&window.id).map(|rect| {
                    let (x, y) = origin
                        .map(|logical| (f64::from(logical.x), f64::from(logical.y)))
                        .unwrap_or((0.0, 0.0));
                    ports::Rect {
                        x: rect.x + x,
                        y: rect.y + y,
                        width: rect.width,
                        height: rect.height,
                    }
                });
                Window {
                    id: window.id.0,
                    title: window.title.clone(),
                    app_id: window.app_id.clone(),
                    workspace: window.workspace.map(|id| id.0),
                    output,
                    focused: focused == Some(window.id),
                    floating: window.floating,
                    rect,
                }
            })
            .collect(),
    }
}

fn capped(text: &str) -> String {
    glimpse_utils::text::clean(text, REASON_CAP)
}

pub struct WaylandScreens<'a> {
    pub compositor: Option<&'a Compositor>,
    pub counted: RefCell<Option<Frame>>,
}

impl Screens for WaylandScreens<'_> {
    fn current(&self) -> Result<String, CaptureError> {
        if let Some(compositor) = self.compositor {
            let state = compositor.provider.handle().subscribe();
            let state = state.borrow();
            return state
                .focus
                .output
                .clone()
                .filter(|_| state.unavailable.is_none())
                .ok_or_else(|| CaptureError::NoSuchOutput("current".to_owned()));
        }
        let frames = capture::outputs(Instant::now() + CAPTURE_TIMEOUT, false)?;
        match <[Frame; 1]>::try_from(frames) {
            Ok([only]) => {
                let name = only.connector.clone();
                self.counted.replace(Some(only));
                Ok(name)
            }
            Err(_) => Err(CaptureError::NoSuchOutput("current".to_owned())),
        }
    }

    fn capture(&self, name: &str, cursor: bool) -> Result<Frame, CaptureError> {
        match self.counted.take() {
            Some(frame) if !cursor && frame.connector == name => Ok(frame),
            _ => capture::output(name, Instant::now() + CAPTURE_TIMEOUT, cursor),
        }
    }
}

#[cfg(test)]
mod tests;
