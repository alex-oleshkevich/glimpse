use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use zbus::fdo::{DBusProxy, ObjectManagerProxy};
use zbus::message::Type;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, MatchRule, Message, MessageStream};

use glimpse_dbus::bluez::{self, DisconnectReason};

use crate::context::Ctx;

use super::{Bluetooth, Event, Interfaces, Objects, Properties};

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

fn nothing() -> Events {
    Box::pin(stream::empty())
}

fn unavailable(reason: impl Into<String>) -> Events {
    let reason = reason.into();
    Box::pin(stream::once(async move { Event::Unavailable(reason) }))
}

async fn signals(
    connection: &Connection,
    interface: &str,
    member: &str,
    namespace: &str,
) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(bluez::SERVICE)
        .ok()?
        .interface(interface)
        .ok()?
        .member(member)
        .ok()?
        .path_namespace(namespace)
        .ok()?
        .build();

    MessageStream::for_match_rule(rule, connection, None)
        .await
        .ok()
}

pub async fn name_owner(ctx: Ctx<Bluetooth>) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Ok(proxy) = DBusProxy::new(connection).await else {
        return nothing();
    };
    let Ok(stream) = proxy.receive_name_owner_changed().await else {
        return nothing();
    };

    Box::pin(stream.filter_map(|signal| async move {
        let args = signal.args().ok()?;
        if args.name.as_str() != bluez::SERVICE {
            return None;
        }
        Some(Event::NameOwner(
            args.new_owner.as_ref().map(ToString::to_string),
        ))
    }))
}

pub async fn objects(ctx: Ctx<Bluetooth>) -> Events {
    let connection = match ctx.system_bus() {
        Ok(connection) => connection.clone(),
        Err(reason) => return unavailable(reason),
    };

    let added = signals(&connection, OBJECT_MANAGER, "InterfacesAdded", bluez::ROOT).await;
    let removed = signals(
        &connection,
        OBJECT_MANAGER,
        "InterfacesRemoved",
        bluez::ROOT,
    )
    .await;

    let mut changes: Vec<Events> = Vec::new();
    if let Some(added) = added {
        changes.push(Box::pin(added.filter_map(|message| async move {
            interfaces_added(&message.ok()?)
        })));
    }
    if let Some(removed) = removed {
        changes.push(Box::pin(removed.filter_map(|message| async move {
            interfaces_removed(&message.ok()?)
        })));
    }

    let first = stream::once(async move { enumerate(&connection).await });
    Box::pin(first.chain(stream::select_all(changes)))
}

pub async fn properties(ctx: Ctx<Bluetooth>) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Some(stream) = signals(connection, PROPERTIES, "PropertiesChanged", bluez::ADAPTERS).await
    else {
        return nothing();
    };

    Box::pin(stream.filter_map(|message| async move { properties_changed(&message.ok()?) }))
}

pub async fn disconnects(ctx: Ctx<Bluetooth>) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Some(stream) = signals(connection, bluez::DEVICE1, "Disconnected", bluez::ADAPTERS).await
    else {
        return nothing();
    };

    Box::pin(stream.filter_map(|message| async move { disconnected(&message.ok()?) }))
}

fn disconnected(message: &Message) -> Option<Event> {
    let path = message.header().path()?.to_string();
    let (reason, _detail) = message.body().deserialize::<(String, String)>().ok()?;

    Some(Event::Disconnected {
        path,
        reason: DisconnectReason::parse(&reason),
    })
}

async fn enumerate(connection: &Connection) -> Event {
    let manager = ObjectManagerProxy::builder(connection)
        .destination(bluez::SERVICE)
        .and_then(|builder| builder.path(bluez::ROOT));
    let manager = match manager {
        Ok(builder) => builder.build().await,
        Err(error) => Err(error),
    };
    let manager = match manager {
        Ok(manager) => manager,
        Err(error) => return Event::Unavailable(error.to_string()),
    };

    match manager.get_managed_objects().await {
        Ok(objects) => Event::Enumerated(Box::new(collect(objects))),
        Err(error) => Event::Unavailable(error.to_string()),
    }
}

fn collect(objects: zbus::fdo::ManagedObjects) -> Objects {
    objects
        .into_iter()
        .map(|(path, interfaces)| {
            let interfaces: Interfaces = interfaces
                .into_iter()
                .map(|(name, properties)| (name.to_string(), properties))
                .collect();
            (path.to_string(), interfaces)
        })
        .collect::<BTreeMap<_, _>>()
}

fn interfaces_added(message: &Message) -> Option<Event> {
    let (path, interfaces) = message
        .body()
        .deserialize::<(OwnedObjectPath, HashMap<String, Properties>)>()
        .ok()?;

    Some(Event::InterfacesAdded {
        path: path.to_string(),
        interfaces: interfaces.into_iter().collect(),
    })
}

fn interfaces_removed(message: &Message) -> Option<Event> {
    let (path, interfaces) = message
        .body()
        .deserialize::<(OwnedObjectPath, Vec<String>)>()
        .ok()?;

    Some(Event::InterfacesRemoved {
        path: path.to_string(),
        interfaces,
    })
}

fn properties_changed(message: &Message) -> Option<Event> {
    let path = message.header().path()?.to_string();
    let (interface, changed, invalidated) = message
        .body()
        .deserialize::<(String, Properties, Vec<String>)>()
        .ok()?;

    Some(Event::PropertiesChanged {
        path,
        interface,
        changed,
        invalidated,
    })
}
