use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt, future};
use glimpse_dbus::Delivery;
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::names::BusName;
use zbus::{MatchRule, Message, MessageStream};

pub(crate) const NOTIFICATIONS: &str = "org.freedesktop.Notifications";

pub(crate) enum Signal {
    Action(u32, String),
    Closed(u32, u32),
    OwnerChanged,
}

enum Raw {
    Message(Delivery<Message>),
    Owner(Delivery<Option<String>>),
}

fn signal(message: &Message) -> Option<Signal> {
    let header = message.header();
    if header.interface()?.as_str() != NOTIFICATIONS {
        return None;
    }
    match header.member()?.as_str() {
        "ActionInvoked" => {
            let (id, key) = message.body().deserialize::<(u32, String)>().ok()?;
            Some(Signal::Action(id, key))
        }
        "NotificationClosed" => {
            let (id, reason) = message.body().deserialize::<(u32, u32)>().ok()?;
            Some(Signal::Closed(id, reason))
        }
        _ => None,
    }
}

fn step(owner: &mut Option<String>, raw: Raw) -> Option<Delivery<Signal>> {
    match raw {
        Raw::Message(Delivery::Missed) | Raw::Owner(Delivery::Missed) => Some(Delivery::Missed),
        Raw::Owner(Delivery::Item(new)) => {
            *owner = new;
            Some(Delivery::Item(Signal::OwnerChanged))
        }
        Raw::Message(Delivery::Item(message)) => {
            let current = owner.as_deref()?;
            let sender = message.header().sender()?.as_str() == current;
            if !sender {
                return None;
            }
            signal(&message).map(Delivery::Item)
        }
    }
}

pub(crate) async fn signals(
    connection: &zbus::Connection,
) -> zbus::Result<BoxStream<'static, Delivery<Signal>>> {
    let dbus = DBusProxy::new(connection).await?;
    let owners = dbus
        .receive_name_owner_changed_with_args(&[(0, NOTIFICATIONS)])
        .await
        .map(|stream| {
            glimpse_dbus::events(stream.filter_map(|change| async move {
                let args = change.args().ok()?;
                Some(args.new_owner().as_ref().map(|owner| owner.to_string()))
            }))
        })?;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(NOTIFICATIONS)?
        .build()
        .to_owned();
    let messages = glimpse_dbus::events(
        MessageStream::for_match_rule(rule, connection, None)
            .await?
            .filter_map(|message| future::ready(message.ok())),
    );
    let owner = dbus
        .get_name_owner(BusName::try_from(NOTIFICATIONS)?)
        .await
        .ok()
        .map(|owner| owner.to_string());
    Ok(
        stream::select(messages.map(Raw::Message), owners.map(Raw::Owner))
            .scan(owner, |owner, raw| future::ready(Some(step(owner, raw))))
            .filter_map(future::ready)
            .boxed(),
    )
}
