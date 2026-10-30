use gio::prelude::*;
use glimpse_compositors::client::Compositor1Proxy;
use glimpse_compositors::contract::key;
use zbus::proxy::CacheProperties;

const MAX_DEPTH: usize = 64;
const ICON_SIZE: i32 = 64;
const NAME_MAX: usize = 24;

pub(crate) struct Windows {
    pub(super) focused: Option<u32>,
    pub(super) owners: Vec<(u64, u32)>,
}

pub(crate) async fn windows(session: &zbus::Connection) -> Option<Windows> {
    let proxy = Compositor1Proxy::builder(session)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    let focus = tokio::time::timeout(glimpse_dbus::DEADLINE, proxy.focus())
        .await
        .ok()?
        .ok()?;
    let items = tokio::time::timeout(glimpse_dbus::DEADLINE, proxy.windows())
        .await
        .ok()?
        .ok()?;
    let owners: Vec<(u64, u32)> = items
        .iter()
        .filter_map(|item| {
            let id = u64::try_from(&**item.get(key::ID)?).ok()?;
            let pid = i32::try_from(&**item.get(key::PID)?).ok()?;
            Some((id, u32::try_from(pid).ok().filter(|pid| *pid > 0)?))
        })
        .collect();
    let focused = focus
        .get(key::WINDOW)
        .and_then(|window| u64::try_from(&**window).ok())
        .and_then(|window| {
            owners
                .iter()
                .find(|(id, _)| *id == window)
                .map(|(_, pid)| *pid)
        });
    Some(Windows { focused, owners })
}

pub(crate) fn foreground(windows: Option<&Windows>, pid: Option<u32>) -> bool {
    let Some(pid) = pid else {
        return true;
    };
    if glimpse_utils::process::has_terminal(pid) {
        return true;
    }
    windows
        .and_then(|windows| windows.focused)
        .is_none_or(|focused| descends(pid, focused, glimpse_utils::process::parent))
}

fn ancestors(pid: u32, parent: impl Fn(u32) -> Option<u32>) -> impl Iterator<Item = u32> {
    std::iter::successors(Some(pid), move |&current| parent(current))
        .take_while(|&current| current > 1)
        .take(MAX_DEPTH)
}

pub(super) fn descends(pid: u32, ancestor: u32, parent: impl Fn(u32) -> Option<u32>) -> bool {
    ancestors(pid, parent).any(|current| current == ancestor)
}

pub(super) fn window_of(
    windows: &Windows,
    pid: u32,
    parent: impl Fn(u32) -> Option<u32>,
) -> Option<u64> {
    ancestors(pid, parent)
        .find_map(|current| {
            let mut owned = windows
                .owners
                .iter()
                .filter(|(_, owner)| *owner == current)
                .map(|(id, _)| *id);
            match (owned.next(), owned.next()) {
                (Some(id), None) => Some(Some(id)),
                (Some(_), Some(_)) => Some(None),
                _ => None,
            }
        })
        .flatten()
}

pub(crate) struct App {
    pub(crate) name: String,
    pub(crate) image: Option<String>,
}

enum Icon {
    File(String),
    Names(Vec<String>),
}

pub(crate) async fn app(pid: Option<u32>) -> Option<App> {
    let pid = pid?;
    let (name, icon) = tokio::task::spawn_blocking(move || desktop_app(pid))
        .await
        .ok()??;
    let image = match icon {
        Some(Icon::File(path)) => Some(path),
        Some(Icon::Names(names)) => theme_file(names).await,
        None => None,
    };
    Some(App { name, image })
}

fn desktop_app(pid: u32) -> Option<(String, Option<Icon>)> {
    let info = ancestors(pid, glimpse_utils::process::parent).find_map(|pid| {
        glimpse_utils::process::unit_app_ids(pid)
            .into_iter()
            .chain(glimpse_utils::process::exe_name(pid))
            .find_map(|id| gio_unix::DesktopAppInfo::new(&format!("{id}.desktop")))
    })?;
    let name = glimpse_utils::clean(info.display_name().as_str(), NAME_MAX);
    if name.is_empty() {
        return None;
    }
    Some((name, info.icon().and_then(icon_of)))
}

fn icon_of(icon: gio::Icon) -> Option<Icon> {
    if let Some(file) = icon.downcast_ref::<gio::FileIcon>() {
        let path = file.file().path().filter(|path| path.is_file())?;
        return path.into_os_string().into_string().ok().map(Icon::File);
    }
    let themed = icon.downcast_ref::<gio::ThemedIcon>()?;
    let names: Vec<String> = themed
        .names()
        .iter()
        .map(|name| name.to_string())
        .filter(|name| !name.ends_with("-symbolic"))
        .collect();
    (!names.is_empty()).then_some(Icon::Names(names))
}

async fn theme_file(names: Vec<String>) -> Option<String> {
    if !gtk4::is_initialized() {
        return None;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    glib::MainContext::default().invoke(move || {
        let _ = tx.send(lookup(&names));
    });
    tokio::time::timeout(glimpse_dbus::DEADLINE, rx)
        .await
        .ok()?
        .ok()?
}

fn lookup(names: &[String]) -> Option<String> {
    let display = gtk4::gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);
    let name = names.iter().find(|name| theme.has_icon(name))?;
    let file = theme
        .lookup_icon(
            name,
            &[],
            ICON_SIZE,
            1,
            gtk4::TextDirection::None,
            gtk4::IconLookupFlags::empty(),
        )
        .file()?;
    file.path()?.to_str().map(str::to_owned)
}
