use futures_util::stream::BoxStream;
use futures_util::{StreamExt, future};
use glimpse_dbus::Delivery;
use zbus::message::Type;
use zbus::{MatchRule, Message, MessageStream};

pub(crate) const NOTIFICATIONS: &str = "org.freedesktop.Notifications";

pub(crate) enum Signal {
    Action(u32, String),
    Closed(u32),
}

pub(crate) fn signal(message: &Message) -> Option<Signal> {
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
            let (id, _) = message.body().deserialize::<(u32, u32)>().ok()?;
            Some(Signal::Closed(id))
        }
        _ => None,
    }
}

pub(crate) async fn signals(
    connection: &zbus::Connection,
) -> zbus::Result<BoxStream<'static, Delivery<Message>>> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(NOTIFICATIONS)?
        .interface(NOTIFICATIONS)?
        .build()
        .to_owned();
    Ok(glimpse_dbus::events(
        MessageStream::for_match_rule(rule, connection, None)
            .await?
            .filter_map(|message| future::ready(message.ok())),
    ))
}
