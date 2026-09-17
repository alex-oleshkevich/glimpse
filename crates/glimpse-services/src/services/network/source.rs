use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use zbus::fdo::{DBusProxy, ObjectManagerProxy};
use zbus::message::Type;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, MatchRule, Message, MessageStream};

use glimpse_dbus::network_manager as nm;

use crate::context::Ctx;

use super::{Event, Interfaces, Network, Objects, Properties};

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
        .sender(nm::SERVICE)
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

fn object_manager_rule(member: &str) -> Option<MatchRule<'static>> {
    Some(
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(nm::SERVICE)
            .ok()?
            .interface(OBJECT_MANAGER)
            .ok()?
            .member(member)
            .ok()?
            .path(nm::OBJECTS)
            .ok()?
            .build()
            .to_owned(),
    )
}

async fn object_manager_signals(connection: &Connection, member: &str) -> Option<MessageStream> {
    let rule = object_manager_rule(member)?;
    MessageStream::for_match_rule(rule, connection, None)
        .await
        .ok()
}

pub async fn name_owner(ctx: Ctx<Network>) -> Events {
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
        if args.name.as_str() != nm::SERVICE {
            return None;
        }
        Some(Event::NameOwner(
            args.new_owner.as_ref().map(ToString::to_string),
        ))
    }))
}

pub async fn objects(ctx: Ctx<Network>) -> Events {
    let connection = match ctx.system_bus() {
        Ok(connection) => connection.clone(),
        Err(reason) => return unavailable(reason),
    };

    let added = object_manager_signals(&connection, "InterfacesAdded").await;
    let removed = object_manager_signals(&connection, "InterfacesRemoved").await;

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

pub async fn properties(ctx: Ctx<Network>) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Some(stream) = signals(connection, PROPERTIES, "PropertiesChanged", nm::MANAGER).await
    else {
        return nothing();
    };

    Box::pin(stream.filter_map(|message| async move { properties_changed(&message.ok()?) }))
}

pub async fn active_states(ctx: Ctx<Network>) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Some(stream) = signals(connection, nm::ACTIVE1, "StateChanged", nm::MANAGER).await else {
        return nothing();
    };

    Box::pin(stream.filter_map(|message| async move {
        let message = message.ok()?;
        let path = message.header().path()?.to_string();
        let (state, reason) = message.body().deserialize::<(u32, u32)>().ok()?;
        Some(Event::ActiveStateChanged {
            path,
            state: nm::ActiveState::from_code(state),
            reason,
        })
    }))
}

pub async fn device_states(ctx: Ctx<Network>) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Some(stream) = signals(connection, nm::DEVICE1, "StateChanged", nm::MANAGER).await else {
        return nothing();
    };

    Box::pin(stream.filter_map(|message| async move {
        let message = message.ok()?;
        let path = message.header().path()?.to_string();
        let (state, _previous, reason) = message.body().deserialize::<(u32, u32, u32)>().ok()?;
        Some(Event::DeviceStateChanged {
            path,
            state: nm::DeviceState::from_code(state),
            reason,
        })
    }))
}

pub async fn profile_updates(ctx: Ctx<Network>) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Some(stream) = signals(connection, nm::SETTINGS_CONNECTION1, "Updated", nm::MANAGER).await
    else {
        return nothing();
    };

    Box::pin(
        stream.filter_map(|message| async move { message.ok().map(|_| Event::ProfilesChanged) }),
    )
}

async fn enumerate(connection: &Connection) -> Event {
    let manager = ObjectManagerProxy::builder(connection)
        .destination(nm::SERVICE)
        .and_then(|builder| builder.path(nm::OBJECTS));
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

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::match_rule::PathSpec;

    #[test]
    fn the_object_manager_signals_are_matched_at_their_own_path_and_not_under_the_manager() {
        let rule = object_manager_rule("InterfacesAdded").expect("a rule");

        assert_eq!(
            rule.path_spec(),
            Some(&PathSpec::Path(
                nm::OBJECTS.try_into().expect("an object path")
            )),
            "NetworkManager emits InterfacesAdded from {} — a path_namespace of {} is above it \
             and matches nothing, which freezes the object set at the first enumeration",
            nm::OBJECTS,
            nm::MANAGER
        );
    }
}
