use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;

use super::*;

async fn tray() -> (
    Tray,
    Ctx<Tray>,
    tokio::sync::watch::Receiver<TrayItems>,
    tokio::sync::watch::Receiver<crate::ServiceState>,
) {
    let cancel = CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel(4);
    let (state, state_rx) = tokio::sync::watch::channel(TrayItems::default());
    let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Tray>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let tray = Tray::start(&ctx, NoConfig, ()).await;
    (tray, ctx, state_rx, health_rx)
}

#[tokio::test]
async fn a_claim_that_failed_reaches_for_the_watcher_that_won_before_it_degrades() {
    let (mut tray, ctx, state, health) = tray().await;

    // The claim's bus work runs off the handler — awaiting name requests, `ListNames` and a
    // probe per candidate here would put every other tray event behind one hung application —
    // so the handler's contract is the *outcome*, not the attempt.
    tray.handle(
        &ctx,
        Input::Event(Event::Claimed(Err("no bus in tests".to_owned()))),
    )
    .await;

    assert!(
        !matches!(&*health.borrow(), crate::ServiceState::Degraded { .. }),
        "a taken name is not a verdict: the host registration has not been tried yet"
    );

    // Only when nobody will have us as a host either is the tray actually out of options.
    tray.handle(
        &ctx,
        Input::Event(Event::Attached(Err("no bus in tests".to_owned()))),
    )
    .await;

    assert_eq!(*state.borrow(), TrayItems::default());
    assert!(
        matches!(&*health.borrow(), crate::ServiceState::Degraded { reason } if reason.contains("no bus")),
        "the reason names why there is no bus, rather than saying the tray is broken"
    );
}

#[tokio::test]
async fn an_attach_that_answered_after_we_took_the_name_is_ignored() {
    let (mut tray, ctx, _state, health) = tray().await;

    // The foreign watcher exited mid-attach and we won the name it freed.
    tray.handle(&ctx, Input::Event(Event::Claimed(Ok(()))))
        .await;
    assert_eq!(*health.borrow(), crate::ServiceState::Running);

    tray.handle(
        &ctx,
        Input::Event(Event::Attached(Err("the watcher went away".to_owned()))),
    )
    .await;

    assert_eq!(
        *health.borrow(),
        crate::ServiceState::Running,
        "a stale answer must not degrade a tray that now holds the name; nothing clears that"
    );
}

#[tokio::test]
async fn a_watcher_changing_hands_restarts_the_follower_rather_than_reusing_it() {
    let (mut tray, ctx, _state, _health) = tray().await;

    tray.handle(
        &ctx,
        Input::Event(Event::Claimed(Err("owned by someone".to_owned()))),
    )
    .await;
    assert!(tray.hosting, "a failed claim declares the follower");
    let first = tray.generation;

    tray.handle(
        &ctx,
        Input::Event(Event::NameOwnerChanged {
            name: WATCHER_NAME.to_owned(),
            new_owner: Some(":1.9".to_owned()),
        }),
    )
    .await;

    assert!(tray.hosting);
    assert_ne!(
        tray.generation, first,
        "the generation is the subscription key, and a new owner knows nothing of us: the old \
             signal rules named the process that just left"
    );
}

#[tokio::test]
async fn a_foreign_watchers_list_replaces_ours_and_drops_what_it_stopped_listing() {
    let (mut tray, ctx, state, health) = tray().await;

    tray.handle(
        &ctx,
        Input::Event(Event::Attached(Ok(vec![
            ":1.1/StatusNotifierItem".to_owned(),
            ":1.2/StatusNotifierItem".to_owned(),
        ]))),
    )
    .await;
    assert_eq!(
        *health.borrow(),
        crate::ServiceState::Running,
        "hosting on someone else's watcher is a working tray, not a degraded one"
    );

    // Nothing has answered a `GetAll` yet, so the bar stays empty: a key with no snapshot
    // behind it would render as a blank chip that fills in a frame later.
    assert_eq!(*state.borrow(), TrayItems::default());

    tray.handle(
        &ctx,
        Input::Event(Event::Attached(Ok(vec![
            ":1.2/StatusNotifierItem".to_owned(),
        ]))),
    )
    .await;
    assert_eq!(
        tray.keys(),
        [":1.2/StatusNotifierItem"],
        "the foreign list is the whole truth, so a dropped entry leaves"
    );
}

#[tokio::test]
async fn a_claim_is_requested_rather_than_awaited_so_one_hung_item_cannot_block_the_queue() {
    let (mut tray, ctx, _state, health) = tray().await;

    tray.handle(&ctx, Input::Event(Event::Claim)).await;

    assert!(
        !matches!(&*health.borrow(), crate::ServiceState::Degraded { .. }),
        "the handler returns immediately; the outcome arrives as its own event"
    );
}

#[tokio::test]
async fn losing_the_backend_empties_the_bar_rather_than_leaving_stale_chips() {
    let (mut tray, ctx, mut state, health) = tray().await;
    tray.items.set(TrayItems {
        items: vec![glimpse_dbus::status_notifier_item::TrayItem {
            key: ":1.1/StatusNotifierItem".to_owned(),
            ..Default::default()
        }],
    });
    assert_eq!(state.borrow_and_update().items.len(), 1);

    tray.handle(
        &ctx,
        Input::Event(Event::Unavailable("the watcher name was taken".to_owned())),
    )
    .await;

    assert_eq!(state.borrow().items, Vec::new());
    assert!(
        matches!(&*health.borrow(), crate::ServiceState::Degraded { reason } if reason.contains("taken")),
    );
}
