use std::collections::HashMap;
use std::path::Path;

use futures_util::stream::{self, BoxStream};
use futures_util::{FutureExt, StreamExt};
use glimpse_dbus::{Changes, Follow, Provider, ProviderError, ProviderHandle, Snapshots, call};
use tokio::sync::watch;
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedValue;

use crate::contract::{BUS_NAME, Capability, Error, Item, key};
use crate::model::{
    Cast, Focus, KeyboardLayouts, Logical, Mode, Output, Rect, Window, WindowId, Workspace,
    WorkspaceId,
};
use crate::sanitize::{
    NAME_CAP, capped_app_id_str, capped_edid_str, capped_name_str, capped_title_str, sanitize,
};

mod keyboard;
pub use keyboard::{KeyboardBarrier, KeyboardUpdate};

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Compositor1",
    default_service = "me.aresa.Glimpse.Compositor",
    default_path = "/me/aresa/Glimpse/Compositor"
)]
pub trait Compositor1 {
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn capabilities(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn outputs(&self) -> zbus::Result<Vec<Item>>;
    #[zbus(property)]
    fn workspaces(&self) -> zbus::Result<Vec<Item>>;
    #[zbus(property)]
    fn windows(&self) -> zbus::Result<Vec<Item>>;
    #[zbus(property)]
    fn focus(&self) -> zbus::Result<Item>;
    #[zbus(property)]
    fn keyboard(&self) -> zbus::Result<Item>;
    #[zbus(property)]
    fn casts(&self) -> zbus::Result<Vec<Item>>;
    #[zbus(property)]
    fn session_lock_restore(&self) -> zbus::Result<bool>;

    fn focus_workspace(&self, id: u64) -> zbus::Result<()>;
    fn step_workspace(&self, forward: bool) -> zbus::Result<()>;
    fn rename_workspace(&self, id: u64, name: &str) -> zbus::Result<()>;
    fn focus_window(&self, id: u64) -> zbus::Result<()>;
    fn step_window(&self, forward: bool) -> zbus::Result<()>;
    fn switch_keyboard_layout(&self, index: u32) -> zbus::Result<()>;
    fn step_keyboard_layout(&self, forward: bool) -> zbus::Result<u32>;
    fn set_output_enabled(&self, connector: &str, enabled: bool) -> zbus::Result<()>;
    fn power_off_monitors(&self) -> zbus::Result<()>;
    fn stop_screencast(&self, session: u64) -> zbus::Result<()>;
    fn screenshot_window(&self, id: u64, path: &str) -> zbus::Result<()>;
    fn window_geometries(&self) -> zbus::Result<HashMap<u64, Item>>;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompositorState {
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub outputs: Vec<Output>,
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub focus: Focus,
    pub keyboard: KeyboardLayouts,
    pub casts: Vec<Cast>,
    pub session_lock_restore: Option<bool>,
    pub unavailable: Option<String>,
}

pub struct Properties {
    name: String,
    capabilities: Vec<String>,
    outputs: Vec<Item>,
    workspaces: Vec<Item>,
    windows: Vec<Item>,
    focus: Item,
    keyboard: Item,
    casts: Vec<Item>,
    session_lock_restore: Option<bool>,
}

impl Follow for CompositorState {
    type Proxy = Compositor1Proxy<'static>;
    type Error = Error;

    const NAME: &'static str = "compositor";

    fn lost(reason: String, _: Option<&Self>) -> Self {
        Self {
            unavailable: Some(reason),
            ..Self::default()
        }
    }

    fn reason(&self) -> Option<&str> {
        self.unavailable.as_deref()
    }
}

impl Snapshots for CompositorState {
    type Snapshot = Properties;

    const BUS_NAME: &'static str = BUS_NAME;

    async fn build(
        connection: &zbus::Connection,
        owner: zbus::names::OwnedUniqueName,
    ) -> zbus::Result<Self::Proxy> {
        Compositor1Proxy::builder(connection)
            .destination(owner)?
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
    }

    async fn read(proxy: &Self::Proxy) -> zbus::Result<Self::Snapshot> {
        assemble(proxy)
    }

    async fn changes(proxy: &Self::Proxy) -> Changes<Self::Snapshot> {
        let streams: [BoxStream<'static, zbus::Result<()>>; 7] = [
            refreshed(proxy.receive_outputs_changed().await),
            refreshed(proxy.receive_workspaces_changed().await),
            refreshed(proxy.receive_windows_changed().await),
            refreshed(proxy.receive_focus_changed().await),
            refreshed(proxy.receive_keyboard_changed().await),
            refreshed(proxy.receive_casts_changed().await),
            refreshed(proxy.receive_session_lock_restore_changed().await),
        ];
        let proxy = proxy.clone();
        stream::select_all(streams)
            .map(move |fetched| {
                let proxy = proxy.clone();
                async move {
                    fetched?;
                    assemble(&proxy)
                }
                .boxed()
            })
            .boxed()
    }

    fn decode(raw: Self::Snapshot) -> Result<Self, String> {
        Ok(Self {
            name: sanitize(&raw.name, NAME_CAP),
            capabilities: Capability::parse_all(&raw.capabilities),
            outputs: raw.outputs.iter().filter_map(output).collect(),
            workspaces: raw.workspaces.iter().filter_map(workspace).collect(),
            windows: raw.windows.iter().filter_map(window).collect(),
            focus: Focus {
                output: text(&raw.focus, key::OUTPUT, capped_name_str),
                window: get(&raw.focus, key::WINDOW).map(WindowId),
            },
            keyboard: KeyboardLayouts {
                names: names(&raw.keyboard, key::NAMES),
                codes: names(&raw.keyboard, key::CODES),
                current: get(&raw.keyboard, key::CURRENT),
            },
            casts: raw.casts.iter().filter_map(cast).collect(),
            session_lock_restore: raw.session_lock_restore,
            unavailable: None,
        })
    }
}

fn refreshed<T>(
    stream: zbus::proxy::PropertyStream<'static, T>,
) -> BoxStream<'static, zbus::Result<()>>
where
    T: TryFrom<OwnedValue> + Unpin + Send + Sync + 'static,
    T::Error: Into<zbus::Error>,
{
    stream
        .then(|changed| async move { changed.get().await.map(|_| ()) })
        .boxed()
}

fn assemble(proxy: &Compositor1Proxy<'_>) -> zbus::Result<Properties> {
    Ok(Properties {
        name: proxy.cached_name()?.ok_or_else(|| {
            zbus::Error::Failure("the compositor provider has no Name".to_owned())
        })?,
        capabilities: proxy.cached_capabilities()?.unwrap_or_default(),
        outputs: proxy.cached_outputs()?.unwrap_or_default(),
        workspaces: proxy.cached_workspaces()?.unwrap_or_default(),
        windows: proxy.cached_windows()?.unwrap_or_default(),
        focus: proxy.cached_focus()?.unwrap_or_default(),
        keyboard: proxy.cached_keyboard()?.unwrap_or_default(),
        casts: proxy.cached_casts()?.unwrap_or_default(),
        session_lock_restore: proxy.cached_session_lock_restore()?,
    })
}

fn get<T: TryFrom<OwnedValue>>(item: &Item, key: &str) -> Option<T> {
    T::try_from(item.get(key)?.try_clone().ok()?).ok()
}

fn text(item: &Item, key: &str, cap: fn(&str) -> Option<String>) -> Option<String> {
    cap(&get::<String>(item, key)?)
}

fn names(item: &Item, key: &str) -> Vec<String> {
    get::<Vec<String>>(item, key)
        .unwrap_or_default()
        .iter()
        .map(|name| sanitize(name, NAME_CAP))
        .collect()
}

fn output(item: &Item) -> Option<Output> {
    Some(Output {
        connector: text(item, key::CONNECTOR, capped_name_str)?,
        enabled: get(item, key::ENABLED).unwrap_or_default(),
        description: text(item, key::DESCRIPTION, capped_name_str),
        make: text(item, key::MAKE, capped_edid_str),
        model: text(item, key::MODEL, capped_edid_str),
        mode: get(item, key::MODE).map(|(width, height, refresh_mhz)| Mode {
            width,
            height,
            refresh_mhz,
        }),
        logical: get(item, key::LOGICAL).map(|(x, y, width, height, scale)| Logical {
            x,
            y,
            width,
            height,
            scale,
        }),
    })
}

fn workspace(item: &Item) -> Option<Workspace> {
    Some(Workspace {
        id: WorkspaceId(get(item, key::ID)?),
        index: get(item, key::INDEX),
        name: text(item, key::NAME, capped_name_str),
        output: text(item, key::OUTPUT, capped_name_str),
        active: get(item, key::ACTIVE).unwrap_or_default(),
    })
}

fn window(item: &Item) -> Option<Window> {
    Some(Window {
        id: WindowId(get(item, key::ID)?),
        title: text(item, key::TITLE, capped_title_str),
        app_id: text(item, key::APP_ID, capped_app_id_str),
        pid: get(item, key::PID),
        workspace: get(item, key::WORKSPACE).map(WorkspaceId),
        floating: get(item, key::FLOATING).unwrap_or_default(),
        urgent: get(item, key::URGENT).unwrap_or_default(),
        order: get(item, key::ORDER),
    })
}

fn cast(item: &Item) -> Option<Cast> {
    Some(Cast {
        id: get(item, key::ID)?,
        active: get(item, key::ACTIVE).unwrap_or_default(),
        output: text(item, key::OUTPUT, capped_name_str),
        session: get(item, key::SESSION),
        pw_node: get(item, key::PW_NODE),
    })
}

fn rect(item: &Item) -> Option<Rect> {
    Some(Rect {
        x: get(item, key::X)?,
        y: get(item, key::Y)?,
        width: get(item, key::WIDTH)?,
        height: get(item, key::HEIGHT)?,
    })
}

impl ProviderError for Error {
    fn unavailable(reason: String) -> Self {
        Self::Unavailable(reason)
    }

    fn timed_out() -> Self {
        Self::Failed("the compositor provider did not answer in time".to_owned())
    }

    fn call(reason: String) -> Self {
        Self::Failed(reason)
    }

    fn named(name: &str, reason: String) -> Option<Self> {
        match name.strip_prefix("me.aresa.Glimpse.Compositor1.Error.")? {
            "Unsupported" => Some(Self::Unsupported(reason)),
            "Refused" => Some(Self::Refused(reason)),
            "Unavailable" => Some(Self::Unavailable(reason)),
            "Failed" => Some(Self::Failed(reason)),
            _ => None,
        }
    }
}

pub type CompositorProvider = Provider<CompositorState>;

#[derive(Clone)]
pub struct Handle(pub ProviderHandle<CompositorState>);

impl Handle {
    pub fn keyboard_updates(&self) -> (KeyboardBarrier, BoxStream<'static, KeyboardUpdate>) {
        keyboard::updates(self.clone())
    }

    pub fn snapshot(&self) -> CompositorState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<CompositorState> {
        self.0.subscribe()
    }

    pub fn supports(&self, capability: Capability) -> bool {
        self.0
            .subscribe()
            .borrow()
            .capabilities
            .contains(&capability)
    }

    pub async fn focus_workspace(&self, id: WorkspaceId) -> Result<(), Error> {
        call(self.0.proxy().await?.focus_workspace(id.0)).await
    }

    pub async fn step_workspace(&self, forward: bool) -> Result<(), Error> {
        call(self.0.proxy().await?.step_workspace(forward)).await
    }

    pub async fn rename_workspace(&self, id: WorkspaceId, name: &str) -> Result<(), Error> {
        call(self.0.proxy().await?.rename_workspace(id.0, name)).await
    }

    pub async fn focus_window(&self, id: WindowId) -> Result<(), Error> {
        call(self.0.proxy().await?.focus_window(id.0)).await
    }

    pub async fn step_window(&self, forward: bool) -> Result<(), Error> {
        call(self.0.proxy().await?.step_window(forward)).await
    }

    pub async fn switch_keyboard_layout(&self, index: u32) -> Result<(), Error> {
        call(self.0.proxy().await?.switch_keyboard_layout(index)).await
    }

    pub async fn step_keyboard_layout(&self, forward: bool) -> Result<u32, Error> {
        call(self.0.proxy().await?.step_keyboard_layout(forward)).await
    }

    pub async fn set_output_enabled(&self, connector: &str, enabled: bool) -> Result<(), Error> {
        call(self.0.proxy().await?.set_output_enabled(connector, enabled)).await
    }

    pub async fn power_off_monitors(&self) -> Result<(), Error> {
        call(self.0.proxy().await?.power_off_monitors()).await
    }

    pub async fn stop_screencast(&self, session: u64) -> Result<(), Error> {
        call(self.0.proxy().await?.stop_screencast(session)).await
    }

    pub async fn screenshot_window(&self, id: WindowId, path: &Path) -> Result<(), Error> {
        let Some(path) = path.to_str() else {
            return Err(Error::Refused(
                "the screenshot path is not UTF-8".to_owned(),
            ));
        };
        call(self.0.proxy().await?.screenshot_window(id.0, path)).await
    }

    pub async fn window_geometries(&self) -> Result<HashMap<WindowId, Rect>, Error> {
        let geometries: HashMap<u64, Item> =
            call(self.0.proxy().await?.window_geometries()).await?;
        Ok(geometries
            .iter()
            .filter_map(|(id, item)| Some((WindowId(*id), rect(item)?)))
            .collect())
    }
}

#[cfg(test)]
mod tests;
