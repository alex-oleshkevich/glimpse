use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::status_notifier_item::{StatusNotifierItemProxy, decode_item};
use glimpse_dbus::status_notifier_watcher::split_key;
use zbus::fdo::PropertiesProxy;
use zbus::names::InterfaceName;

use crate::context::Ctx;

use super::{Event, Tray};

const ITEM: &str = "org.kde.StatusNotifierItem";

type Trigger = Pin<Box<dyn Stream<Item = ()> + Send>>;

/// Everything an item can say about itself, as one stream of decoded snapshots. The service's
/// equality gate collapses the duplicates, which is what makes it safe to subscribe to both
/// `PropertiesChanged` *and* every `New*` signal — applications are inconsistent about which they
/// emit, and several emit both.
pub async fn follow(ctx: Ctx<Tray>, key: String) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match sources(&ctx, &key).await {
        Some(stream) => stream,
        None => Box::pin(stream::once(async move { Event::ItemGone(key) })),
    }
}

async fn sources(ctx: &Ctx<Tray>, key: &str) -> Option<Pin<Box<dyn Stream<Item = Event> + Send>>> {
    let connection = ctx.session_bus().ok()?;
    let (owner, path) = split_key(key)?;

    let item = StatusNotifierItemProxy::builder(connection)
        .destination(owner.to_owned())
        .ok()?
        .path(path.to_owned())
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    let properties = PropertiesProxy::builder(connection)
        .destination(owner.to_owned())
        .ok()?
        .path(path.to_owned())
        .ok()?
        .build()
        .await
        .ok()?;

    let mut triggers: Vec<Trigger> = Vec::new();
    if let Ok(changed) = properties.receive_properties_changed().await {
        triggers.push(Box::pin(changed.map(|_| ())));
    }
    // The `New*` signals carry no arguments — they mean "read it again".
    if let Ok(signal) = item.receive_new_title().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_new_icon().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_new_attention_icon().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_new_overlay_icon().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_new_tool_tip().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_new_menu().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_new_icon_theme_path().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_new_status().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }
    if let Ok(signal) = item.receive_x_ayatana_new_label().await {
        triggers.push(Box::pin(signal.map(|_| ())));
    }

    let first = read(&properties, key).await;
    let owned = key.to_owned();
    let updates = stream::select_all(triggers).then(move |()| {
        let properties = properties.clone();
        let key = owned.clone();
        async move { read(&properties, &key).await }
    });

    Some(Box::pin(stream::once(async move { first }).chain(updates)))
}

/// One `GetAll` is the whole read. Asking property by property costs a round trip *and* an error
/// for every member the application did not implement, and every application implements a subset.
async fn read(properties: &PropertiesProxy<'static>, key: &str) -> Event {
    let Ok(interface) = InterfaceName::try_from(ITEM) else {
        return Event::ItemGone(key.to_owned());
    };
    match properties.get_all(interface).await {
        Ok(map) => Event::ItemChanged(key.to_owned(), Box::new(decode_item(key, &map))),
        // `ServiceUnknown` and `UnknownObject` are ordinary: applications quit. Drop the item
        // rather than logging an error and retrying something that will never answer.
        Err(error) => {
            tracing::debug!(%key, %error, "a tray item stopped answering");
            Event::ItemGone(key.to_owned())
        }
    }
}
