use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::dbusmenu::{DBusMenuProxy, MenuNode, decode_layout};
use glimpse_dbus::status_notifier_watcher::split_key;
use zbus::Connection;

use crate::context::Ctx;
use crate::service::CommandError;

use super::{Event, Tray};

const NOTICE: &str = "notice";

/// `LayoutUpdated` carries the new revision. Comparing it against the one already decoded is what
/// stops a refetch per keystroke in an application that rewrites its menu constantly.
pub async fn follow(
    ctx: Ctx<Tray>,
    key: String,
    path: String,
) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    let Ok(connection) = ctx.session_bus() else {
        return Box::pin(stream::empty());
    };
    let Some(menu) = proxy(connection, &key, &path).await else {
        return Box::pin(stream::empty());
    };
    let Ok(updates) = menu.receive_layout_updated().await else {
        return Box::pin(stream::empty());
    };
    let layouts = updates.filter_map({
        let key = key.clone();
        move |signal| {
            let key = key.clone();
            async move {
                let revision = signal.args().ok()?.revision;
                Some(Event::MenuChanged(key, revision))
            }
        }
    });

    // Labels, enabled flags and toggle states move on `ItemsPropertiesUpdated`, which carries no
    // revision at all. Without this the cached tree keeps its old text and `fetch` sees a revision
    // that has not moved, so it hands the stale one back forever. `MenuStale` drops the cache.
    let properties = match menu.receive_items_properties_updated().await {
        Ok(stream) => {
            let key = key.clone();
            futures_util::future::Either::Left(stream.map(move |_| Event::MenuStale(key.clone())))
        }
        Err(_) => futures_util::future::Either::Right(stream::empty()),
    };

    // The menu's own `Status` is `normal` or `notice` — the calm counterpart to the item's
    // `NeedsAttention`, and a different property on a different interface.
    let first = Event::MenuStatus(
        key.clone(),
        menu.status().await.unwrap_or_default() == NOTICE,
    );
    let statuses = menu.receive_status_changed().await.filter_map({
        let key = key.clone();
        move |changed| {
            let key = key.clone();
            async move { Some(Event::MenuStatus(key, changed.get().await.ok()? == NOTICE)) }
        }
    });

    let signals = stream::select(
        stream::select(Box::pin(layouts), Box::pin(statuses)),
        Box::pin(properties),
    );
    Box::pin(stream::once(async move { first }).chain(signals))
}

async fn proxy(connection: &Connection, key: &str, path: &str) -> Option<DBusMenuProxy<'static>> {
    let (owner, _) = split_key(key)?;
    DBusMenuProxy::builder(connection)
        .destination(owner.to_owned())
        .ok()?
        .path(path.to_owned())
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()
}

/// `AboutToShow` reports whether the layout changed and **must be awaited** before rendering;
/// getting it the wrong way round gives a stale menu. It is also where an application populates a
/// menu lazily, which is why this runs on pointer-enter rather than on click.
pub async fn fetch(
    connection: &Connection,
    key: &str,
    path: &str,
    known: Option<u32>,
) -> Result<Option<(u32, MenuNode)>, CommandError> {
    let menu = proxy(connection, key, path)
        .await
        .ok_or_else(|| CommandError::Unavailable("the item offers no menu".to_owned()))?;

    let changed = menu.about_to_show(0).await.unwrap_or(false);
    let (revision, layout) = menu
        .get_layout(0, -1, &[])
        .await
        .map_err(|error| CommandError::Unavailable(error.to_string()))?;
    let decoded = decode_layout(&layout);

    // An application may populate a submenu only when told that submenu is about to show. Naming
    // just the root leaves every nested level empty, and `PopoverMenu` builds the whole tree at
    // once, so there is no later moment to ask.
    let nested = submenus(&decoded);
    if !nested.is_empty()
        && let Ok((updated, _)) = menu.about_to_show_group(&nested).await
        && !updated.is_empty()
        && let Ok((again, filled)) = menu.get_layout(0, -1, &[]).await
    {
        return Ok(Some((again, decode_layout(&filled))));
    }

    if !changed && known == Some(revision) {
        return Ok(None);
    }
    Ok(Some((revision, decoded)))
}

/// Every id that says it has a submenu, so all of them can be notified in one call.
fn submenus(node: &MenuNode) -> Vec<i32> {
    let mut ids = Vec::new();
    collect(node, &mut ids);
    ids
}

fn collect(node: &MenuNode, ids: &mut Vec<i32>) {
    for child in &node.children {
        if child.submenu || !child.children.is_empty() {
            ids.push(child.id);
            collect(child, ids);
        }
    }
}

/// `Event` is void on the wire and must be sent without waiting for a reply — awaiting one blocks
/// the click on whatever the application decides to do about it.
pub async fn event(
    connection: &Connection,
    key: &str,
    path: &str,
    id: i32,
    name: &str,
) -> Result<(), CommandError> {
    let menu = proxy(connection, key, path)
        .await
        .ok_or_else(|| CommandError::Unavailable("the item offers no menu".to_owned()))?;
    menu.event(
        id,
        name,
        &zbus::zvariant::Value::from(0u8),
        chrono::Utc::now().timestamp() as u32,
    )
    .await
    .map_err(|error| CommandError::Unavailable(error.to_string()))
}
