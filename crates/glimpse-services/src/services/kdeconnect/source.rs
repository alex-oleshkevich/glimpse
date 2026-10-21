use std::collections::HashMap;
use std::pin::Pin;

use futures_util::stream::BoxStream;
use futures_util::{Stream, StreamExt, future, stream};
use zbus::fdo::{DBusProxy, PropertiesProxy};
use zbus::message::Type;
use zbus::names::{BusName, InterfaceName};
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, MatchRule, Message, MessageStream};

use glimpse_dbus::kdeconnect::{
    self, BATTERY, CLIPBOARD, DEVICE, DaemonProxy, DeviceProxy, PLUGIN_BATTERY, PLUGIN_CLIPBOARD,
    PLUGIN_PING, PLUGIN_RING, PLUGIN_SFTP, PLUGIN_SHARE, PLUGIN_SMS, PairState,
};

use crate::context::Ctx;

use super::{Actions, Device, DeviceId, Event, Kdeconnect, Stale};

const MAX_DEVICES: usize = 64;

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

const LIST_SIGNALS: [&str; 4] = [
    "deviceAdded",
    "deviceRemoved",
    "deviceVisibilityChanged",
    "deviceListChanged",
];

const DEVICE_SIGNALS: [&str; 8] = [
    "reachableChanged",
    "pairStateChanged",
    "pairingFailed",
    "nameChanged",
    "typeChanged",
    "pluginsChanged",
    "refreshed",
    "autoShareDisabledChanged",
];

fn nothing() -> Events {
    Box::pin(stream::empty())
}

fn unavailable(reason: impl Into<String>) -> Events {
    let reason = reason.into();
    Box::pin(stream::once(async move { Event::Unavailable(reason) }))
}

pub async fn name_owner(ctx: Ctx<Kdeconnect>) -> Events {
    let connection = match ctx.session_bus() {
        Ok(connection) => connection.clone(),
        Err(reason) => return unavailable(reason),
    };
    let proxy = match DBusProxy::new(&connection).await {
        Ok(proxy) => proxy,
        Err(error) => return unavailable(error.to_string()),
    };
    let owners = match proxy
        .receive_name_owner_changed_with_args(&[(0, kdeconnect::SERVICE)])
        .await
        .map(|changes| {
            glimpse_dbus::events(changes.filter_map(|signal| async move {
                Some(
                    signal
                        .args()
                        .ok()?
                        .new_owner
                        .as_ref()
                        .map(ToString::to_string),
                )
            }))
        }) {
        Ok(owners) => owners,
        Err(error) => return unavailable(error.to_string()),
    };
    let current = current_owner(&proxy).await;

    let changes = owners.then(move |delivery| {
        let proxy = proxy.clone();
        async move {
            match delivery {
                glimpse_dbus::Delivery::Item(owner) => Event::NameOwner(owner),
                glimpse_dbus::Delivery::Missed => Event::NameOwner(current_owner(&proxy).await),
            }
        }
    });
    Box::pin(stream::once(async move { Event::NameOwner(current) }).chain(changes))
}

async fn current_owner(proxy: &DBusProxy<'_>) -> Option<String> {
    let name = BusName::try_from(kdeconnect::SERVICE).ok()?;
    proxy
        .get_name_owner(name)
        .await
        .ok()
        .map(|owner| owner.to_string())
}

pub async fn devices(ctx: Ctx<Kdeconnect>, owner: String, generation: u64) -> Events {
    let Ok(connection) = ctx.session_bus() else {
        return nothing();
    };
    let Some(changes) = signals(connection, &owner).await else {
        return Box::pin(stream::once(async move {
            Event::Failed {
                generation,
                reason: "could not follow kdeconnectd's signals".to_owned(),
            }
        }));
    };
    let first = stream::once(async move {
        Event::Stale {
            generation,
            stale: Stale::All,
        }
    });
    let changes = changes.map(move |delivery| Event::Stale {
        generation,
        stale: match delivery {
            glimpse_dbus::Delivery::Item(stale) => stale,
            glimpse_dbus::Delivery::Missed => Stale::All,
        },
    });
    Box::pin(first.chain(changes))
}

async fn signals(
    connection: &Connection,
    owner: &str,
) -> Option<BoxStream<'static, glimpse_dbus::Delivery<Stale>>> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(owner)
        .ok()?
        .path_namespace(kdeconnect::ROOT)
        .ok()?
        .build();
    Some(glimpse_dbus::events(
        MessageStream::for_match_rule(rule, connection, None)
            .await
            .ok()?
            .filter_map(|message| future::ready(message.ok().and_then(|message| stale(&message)))),
    ))
}

fn stale(message: &Message) -> Option<Stale> {
    let header = message.header();
    let path = header.path()?.as_str();
    let member = header.member()?;
    let member = member.as_str();
    if path == kdeconnect::ROOT {
        return LIST_SIGNALS.contains(&member).then_some(Stale::All);
    }
    if !DEVICE_SIGNALS.contains(&member) {
        return None;
    }
    Some(Stale::Device(DeviceId::new(kdeconnect::device_of(path)?)))
}

pub(super) async fn enumerate(connection: &Connection, owner: &str) -> Result<Vec<Device>, String> {
    let daemon = daemon(connection, owner)
        .await
        .map_err(|error| error.to_string())?;
    let ids = daemon
        .devices(false, false)
        .await
        .map_err(|error| error.to_string())?;
    let mut devices = Vec::with_capacity(ids.len().min(MAX_DEVICES));
    for id in ids.into_iter().take(MAX_DEVICES) {
        if let Some(device) = fetch(connection, owner, &DeviceId::new(id)).await {
            devices.push(device);
        }
    }
    Ok(devices)
}

pub(super) async fn daemon(
    connection: &Connection,
    owner: &str,
) -> zbus::Result<DaemonProxy<'static>> {
    DaemonProxy::builder(connection)
        .destination(owner.to_owned())?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

async fn properties(
    connection: &Connection,
    owner: &str,
    path: String,
    interface: &'static str,
) -> Option<HashMap<String, OwnedValue>> {
    let proxy = PropertiesProxy::builder(connection)
        .destination(owner.to_owned())
        .ok()?
        .path(path)
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    let interface = InterfaceName::try_from(interface).ok()?;
    proxy.get_all(interface).await.ok()
}

pub(super) async fn fetch(connection: &Connection, owner: &str, id: &DeviceId) -> Option<Device> {
    let raw = properties(
        connection,
        owner,
        kdeconnect::device_path(id.as_str()),
        DEVICE,
    )
    .await?;
    let decoded = kdeconnect::decode_device(&raw);

    let plugins = match decoded.reachable && decoded.pair == PairState::Paired {
        true => loaded_plugins(connection, owner, id).await,
        false => Vec::new(),
    };
    let loaded = |plugin: &str| plugins.iter().any(|loaded| loaded == plugin);

    let battery = match loaded(PLUGIN_BATTERY) {
        true => properties(
            connection,
            owner,
            kdeconnect::plugin_path(id.as_str(), "battery"),
            BATTERY,
        )
        .await
        .and_then(|raw| kdeconnect::decode_battery(&raw)),
        false => None,
    };
    let clipboard = match loaded(PLUGIN_CLIPBOARD) {
        true => properties(
            connection,
            owner,
            kdeconnect::plugin_path(id.as_str(), "clipboard"),
            CLIPBOARD,
        )
        .await
        .is_some_and(|raw| kdeconnect::decode_auto_share_disabled(&raw)),
        false => false,
    };

    Some(Device::project(
        id.clone(),
        decoded,
        battery,
        Actions {
            ring: loaded(PLUGIN_RING),
            ping: loaded(PLUGIN_PING),
            send_clipboard: clipboard,
            share: loaded(PLUGIN_SHARE),
            browse: loaded(PLUGIN_SFTP),
            messages: loaded(PLUGIN_SMS),
        },
    ))
}

async fn loaded_plugins(connection: &Connection, owner: &str, id: &DeviceId) -> Vec<String> {
    let proxy = DeviceProxy::builder(connection)
        .destination(owner.to_owned())
        .and_then(|builder| builder.path(kdeconnect::device_path(id.as_str())))
        .map(|builder| builder.cache_properties(CacheProperties::No));
    let proxy = match proxy {
        Ok(builder) => builder.build().await,
        Err(error) => Err(error),
    };
    match proxy {
        Ok(proxy) => proxy.loaded_plugins().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}
