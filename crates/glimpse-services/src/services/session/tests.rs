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
    let (_runtime, compositor) = crate::ServiceRuntime::<super::super::compositor::Compositor>::new(
        NoConfig,
        Buses::unavailable("no bus"),
        CancellationToken::new(),
    );
    let session = Session::start(&ctx, NoConfig, Dependencies { compositor }).await;
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

#[tokio::test]
async fn with_no_compositor_provider_the_session_is_not_private() {
    use crate::services::compositor::tests::{mirror_on, until};

    let bus = glimpse_dbus::testing::PrivateBus::start();
    let mirror = mirror_on(&bus).await;
    let mut compositor = mirror.handle.subscribe();
    let published = until(&mut compositor, "the unsupported privacy", |state| {
        state.privacy.is_some()
    })
    .await;
    let (mut session, ctx, mut state) = session().await;

    session
        .handle(
            &ctx,
            Input::Event(Event::Privacy(
                published.privacy.map(|privacy| privacy.active),
            )),
        )
        .await;
    session
        .handle(&ctx, Input::Event(Event::Locked(false)))
        .await;

    assert!(state.changed().await.is_ok());
    assert_eq!(
        *state.borrow(),
        Some(SessionStatus {
            locked: false,
            private: false,
        })
    );
}
