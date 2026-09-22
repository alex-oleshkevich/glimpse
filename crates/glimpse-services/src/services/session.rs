use std::{convert::Infallible, pin::Pin};

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::login1::{Login1SessionProxy, session_path};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, NoConfig, Service, ServiceError},
    subscription::Sub,
};

use super::compositor::CompositorHandle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStatus {
    pub locked: bool,
    pub private: bool,
}

pub enum Event {
    Privacy(Option<bool>),
    Locked(bool),
    Unavailable(String),
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Privacy,
    Locked,
}

pub struct Session {
    status: Publisher<Option<SessionStatus>>,
    locked: Option<bool>,
    lock_available: bool,
    compositor_private: Option<bool>,
    compositor: CompositorHandle,
}

#[derive(Clone)]
pub struct SessionHandle(crate::ServiceEndpoint<Session>);

impl SessionHandle {
    pub fn snapshot(&self) -> Option<SessionStatus> {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Option<SessionStatus>> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }
}

pub struct Dependencies {
    pub compositor: CompositorHandle,
}

impl Service for Session {
    const NAME: &'static str = "session";
    type Config = NoConfig;
    type State = Option<SessionStatus>;
    type Handle = SessionHandle;
    type Command = Infallible;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        SessionHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::watch(
                Watch::Privacy,
                self.compositor.subscribe(),
                |state| Event::Privacy(state.privacy.map(|privacy| privacy.active)),
                Event::Privacy(None),
            ),
            Sub::stream(Watch::Locked, locked),
        ]
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            status: ctx.publisher(),
            locked: None,
            lock_available: false,
            compositor_private: None,
            compositor: dependencies.compositor,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => match command {},
            Input::Event(Event::Privacy(private)) => {
                self.compositor_private = private;
                match (self.lock_available, self.compositor_private) {
                    (true, Some(_)) => ctx.running(),
                    (_, None) => ctx.degraded("compositor privacy state is unavailable"),
                    _ => {}
                }
                self.publish();
            }
            Input::Event(Event::Locked(locked)) => {
                self.locked = Some(locked);
                self.lock_available = true;
                if self.compositor_private.is_some() {
                    ctx.running();
                }
                self.publish();
            }
            Input::Event(Event::Unavailable(reason)) => {
                self.locked = Some(true);
                self.lock_available = false;
                ctx.degraded(reason.clone());
                tracing::warn!(%reason, "the session lock state is unavailable; treating it as locked");
                self.publish();
            }
            Input::Config(NoConfig) => {}
        }
    }
}

impl Session {
    fn publish(&mut self) {
        self.status
            .set(
                self.locked
                    .zip(self.compositor_private)
                    .map(|(locked, compositor_private)| SessionStatus {
                        locked,
                        private: compositor_private,
                    }),
            );
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
    let path = session_path(&bus).await?;
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

    async fn session() -> (
        Session,
        Ctx<Session>,
        tokio::sync::watch::Receiver<Option<SessionStatus>>,
    ) {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(4);
        let (state, state_rx) = tokio::sync::watch::channel(None);
        let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Session>::new(events, &cancel, state, health, Buses::unavailable("no bus"));
        let (_runtime, compositor) =
            crate::ServiceRuntime::<super::super::compositor::Compositor>::new(
                NoConfig,
                Buses::unavailable("no bus"),
                CancellationToken::new(),
            );
        let session = Session::start(&ctx, NoConfig, Dependencies { compositor })
            .await
            .expect("starts");
        (session, ctx, state_rx)
    }

    #[tokio::test]
    async fn each_authoritative_gate_is_required_and_published() {
        let (mut session, ctx, mut state) = session().await;

        session
            .handle(&ctx, Input::Event(Event::Privacy(Some(false))))
            .await;
        assert_eq!(*state.borrow(), None);
        session
            .handle(&ctx, Input::Event(Event::Locked(true)))
            .await;

        assert!(state.changed().await.is_ok());
        assert_eq!(
            *state.borrow(),
            Some(SessionStatus {
                locked: true,
                private: false,
            })
        );
    }

    #[tokio::test]
    async fn compositor_privacy_is_published_directly() {
        let (mut session, ctx, mut state) = session().await;

        session
            .handle(&ctx, Input::Event(Event::Privacy(Some(true))))
            .await;
        session
            .handle(&ctx, Input::Event(Event::Locked(false)))
            .await;
        assert!(state.changed().await.is_ok());
        assert!(state.borrow().as_ref().is_some_and(|state| state.private));

        session
            .handle(&ctx, Input::Event(Event::Privacy(None)))
            .await;
        assert!(state.changed().await.is_ok());
        assert_eq!(*state.borrow(), None);
        assert!(ctx.is_degraded());
    }

    #[tokio::test]
    async fn losing_the_lock_source_fails_closed() {
        let (mut session, ctx, mut state) = session().await;

        session
            .handle(&ctx, Input::Event(Event::Privacy(Some(false))))
            .await;
        session
            .handle(&ctx, Input::Event(Event::Locked(false)))
            .await;
        session
            .handle(&ctx, Input::Event(Event::Unavailable("lost logind".into())))
            .await;

        assert!(state.changed().await.is_ok());
        assert!(state.borrow().as_ref().is_some_and(|state| state.locked));
    }
}
