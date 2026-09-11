use std::{convert::Infallible, pin::Pin, process};

use futures_util::{Stream, StreamExt, stream};
use glimpse_contracts::{CompositorPrivacy, Message, SessionStatus};
use glimpse_dbus::login1::{Login1ManagerProxy, Login1SessionProxy};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, NoConfig, Service, ServiceError},
    subscription::Sub,
};

pub enum Event {
    Privacy(bool),
    Locked(bool),
    Unavailable(String),
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Privacy,
    Locked,
}

pub struct Session {
    status: Publisher<SessionStatus>,
    locked: Option<bool>,
    compositor_private: Option<bool>,
}

impl Service for Session {
    const NAME: &'static str = "session";
    const TOPICS: &'static [&'static str] = &[SessionStatus::NAME];
    type Config = NoConfig;
    type Command = Infallible;
    type Event = Event;
    type SubKey = Watch;

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::topic::<CompositorPrivacy>(Watch::Privacy, |status| Event::Privacy(status.active)),
            Sub::stream(Watch::Locked, locked),
        ]
    }

    async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        Ok(Self {
            status: ctx.publisher::<SessionStatus>(),
            locked: None,
            compositor_private: None,
        })
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command, _) => match command {},
            Input::Event(Event::Privacy(private)) => {
                self.compositor_private = Some(private);
                self.publish();
            }
            Input::Event(Event::Locked(locked)) => {
                self.locked = Some(locked);
                self.publish();
            }
            Input::Event(Event::Unavailable(reason)) => {
                tracing::warn!(%reason, "the session lock state is unavailable");
            }
            Input::Config(NoConfig) => {}
        }
    }
}

impl Session {
    fn publish(&mut self) {
        let (Some(locked), Some(compositor_private)) = (self.locked, self.compositor_private)
        else {
            return;
        };
        self.status.set(SessionStatus {
            locked,
            private: compositor_private,
        });
    }
}

async fn locked(ctx: Ctx<Session>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match locked_events(&ctx).await {
        Ok(events) => Box::pin(events),
        Err(reason) => Box::pin(stream::once(async move { Event::Unavailable(reason) })),
    }
}

async fn locked_events(
    ctx: &Ctx<Session>,
) -> Result<impl Stream<Item = Event> + Send + 'static, String> {
    let bus = ctx.system_bus().map_err(str::to_owned)?.clone();
    let manager = Login1ManagerProxy::new(&bus).await.map_err(say)?;
    let path = manager
        .get_session_by_pid(process::id())
        .await
        .map_err(say)?;
    let session = Login1SessionProxy::builder(&bus)
        .path(path)
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;
    let changes = session.receive_locked_hint_changed().await;
    let first = session.locked_hint().await.map_err(say)?;
    let following = changes.filter_map(|change| async move {
        match change.get().await {
            Ok(locked) => Some(Event::Locked(locked)),
            Err(error) => Some(Event::Unavailable(error.to_string())),
        }
    });
    Ok(stream::once(async move { Event::Locked(first) }).chain(following))
}

fn say(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{BrokerHandle, MockBroker};

    #[test]
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Session>();
    }

    #[tokio::test]
    async fn each_authoritative_gate_is_required_and_published() {
        let mock = std::sync::Arc::new(MockBroker::default());
        let broker: std::sync::Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(4);
        let ctx = Ctx::<Session>::new(events, &cancel, broker, Buses::unavailable("no bus"));
        let mut session = Session::start(&ctx, NoConfig).await.expect("starts");

        session
            .handle(&ctx, Input::Event(Event::Privacy(false)))
            .await;
        assert!(mock.published().is_empty());
        session
            .handle(&ctx, Input::Event(Event::Locked(true)))
            .await;

        let published = mock.published();
        assert_eq!(published.last().unwrap().1["locked"], true);
        assert_eq!(published.last().unwrap().1["private"], false);
    }

    #[tokio::test]
    async fn compositor_privacy_is_published_directly() {
        let mock = std::sync::Arc::new(MockBroker::default());
        let broker: std::sync::Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(4);
        let ctx = Ctx::<Session>::new(events, &cancel, broker, Buses::unavailable("no bus"));
        let mut session = Session::start(&ctx, NoConfig).await.expect("starts");

        session
            .handle(&ctx, Input::Event(Event::Privacy(true)))
            .await;
        session
            .handle(&ctx, Input::Event(Event::Locked(false)))
            .await;
        assert_eq!(mock.published().last().unwrap().1["private"], true);
    }
}
