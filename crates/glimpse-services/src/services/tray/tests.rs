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

    tray.handle(
        &ctx,
        Input::Event(Event::Claimed(Err("no bus in tests".to_owned()))),
    )
    .await;

    assert!(
        !matches!(&*health.borrow(), crate::ServiceState::Degraded { .. }),
        "a taken name is not a verdict: the host registration has not been tried yet"
    );

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

#[tokio::test(flavor = "multi_thread")]
async fn an_item_that_floods_signals_never_stalls_the_connection() {
    use futures_util::StreamExt;
    use std::time::Duration;

    let bus = glimpse_dbus::testing::PrivateBus::start();
    let app = bus.connection().await;
    let panel = bus.connection().await;
    let app_name = app.unique_name().unwrap().to_string();

    let served = app.clone();
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = reads.clone();
    tokio::spawn(async move {
        let mut calls = zbus::MessageStream::from(&served);
        let mut n = 0;
        while let Some(Ok(msg)) = calls.next().await {
            let header = msg.header();
            if header.message_type() != zbus::message::Type::MethodCall {
                continue;
            }
            if header.member().map(|m| m.as_str()) != Some("GetAll") {
                continue;
            }
            n += 1;
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let map: std::collections::HashMap<String, zbus::zvariant::OwnedValue> =
                Default::default();
            if n == 1 {
                served.reply(&header, &map).await.unwrap();
                served
                    .emit_signal(
                        None::<&str>,
                        "/StatusNotifierItem",
                        "org.kde.StatusNotifierItem",
                        "NewIcon",
                        &(),
                    )
                    .await
                    .unwrap();
            } else if n == 2 {
                for _ in 0..200 {
                    served
                        .emit_signal(
                            None::<&str>,
                            "/StatusNotifierItem",
                            "org.kde.StatusNotifierItem",
                            "NewIcon",
                            &(),
                        )
                        .await
                        .unwrap();
                }
                served.reply(&header, &map).await.unwrap();
            } else {
                served.reply(&header, &map).await.unwrap();
            }
        }
    });

    let cancel = CancellationToken::new();
    let (events, mut inbox) = tokio::sync::mpsc::channel(4);
    let (state, _state_rx) = tokio::sync::watch::channel(TrayItems::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Tray>::new(
        events,
        &cancel,
        state,
        health,
        Buses::for_session(panel.clone()),
    );
    let key = format!("{app_name}/StatusNotifierItem");
    let _source = ctx.stream(move |ctx| item::follow(ctx, key));
    tokio::spawn(async move { while inbox.recv().await.is_some() {} });

    tokio::time::sleep(Duration::from_millis(1000)).await;
    let answered = tokio::time::timeout(
        Duration::from_secs(3),
        zbus::fdo::DBusProxy::new(&panel)
            .await
            .unwrap()
            .list_names(),
    )
    .await;
    assert!(answered.is_ok(), "the connection stopped being read");
    assert!(
        reads.load(std::sync::atomic::Ordering::SeqCst) <= 10,
        "one burst caused {} reads",
        reads.load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missed_name_change_evicts_only_the_item_whose_owner_vanished() {
    use std::time::Duration;

    let bus = glimpse_dbus::testing::PrivateBus::start();
    let panel = bus.connection().await;
    let kept = bus.connection().await;
    let lost = bus.connection().await;
    let kept_name = kept.unique_name().unwrap().to_string();
    let lost_name = lost.unique_name().unwrap().to_string();

    let cancel = CancellationToken::new();
    let (events, mut inbox) = tokio::sync::mpsc::channel(16);
    let (state, _state_rx) = tokio::sync::watch::channel(TrayItems::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Tray>::new(
        events,
        &cancel,
        state,
        health,
        Buses::for_session(panel.clone()),
    );
    let mut tray = Tray::start(&ctx, NoConfig, ()).await;
    glimpse_dbus::own_name(&panel, WATCHER_NAME).await.unwrap();
    tray.claimed = true;
    {
        let mut registry = tray.registry.lock().unwrap();
        registry.register(&kept_name, "/StatusNotifierItem");
        registry.register(&lost_name, "/StatusNotifierItem");
    }
    assert_eq!(tray.keys().len(), 2);

    drop(lost);
    tokio::time::sleep(Duration::from_millis(200)).await;
    tray.handle(&ctx, Input::Event(Event::NamesMissed)).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tray.keys().len() == 2 && tokio::time::Instant::now() < deadline {
        if let Ok(Some(Input::Event(event @ Event::NameOwnerChanged { .. }))) =
            tokio::time::timeout(Duration::from_millis(100), inbox.recv()).await
        {
            tray.handle(&ctx, Input::Event(event)).await;
        }
    }

    assert_eq!(tray.keys(), [format!("{kept_name}/StatusNotifierItem")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missed_release_of_a_foreign_watcher_makes_us_claim_the_name() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let panel = bus.connection().await;
    let cancel = CancellationToken::new();
    let (events, mut inbox) = tokio::sync::mpsc::channel(16);
    let (state, _state_rx) = tokio::sync::watch::channel(TrayItems::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Tray>::new(events, &cancel, state, health, Buses::for_session(panel));
    let mut tray = Tray::start(&ctx, NoConfig, ()).await;
    tray.hosting = true;
    tray.handle(&ctx, Input::Event(Event::NamesMissed)).await;
    assert!(!tray.claimed);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !tray.claimed && tokio::time::Instant::now() < deadline {
        if let Ok(Some(input)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), inbox.recv()).await
        {
            tray.handle(&ctx, input).await;
        }
    }
    assert!(tray.claimed);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missed_loss_of_our_watcher_name_clears_claimed() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let panel = bus.connection().await;
    let other = bus.connection().await;
    glimpse_dbus::own_name(&other, WATCHER_NAME).await.unwrap();
    let cancel = CancellationToken::new();
    let (events, mut inbox) = tokio::sync::mpsc::channel(16);
    let (state, _state_rx) = tokio::sync::watch::channel(TrayItems::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Tray>::new(events, &cancel, state, health, Buses::for_session(panel));
    let mut tray = Tray::start(&ctx, NoConfig, ()).await;
    tray.claimed = true;

    tray.handle(&ctx, Input::Event(Event::NamesMissed)).await;
    let moved = tokio::time::timeout(std::time::Duration::from_secs(5), inbox.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        moved,
        Input::Event(Event::NameOwnerChanged {
            new_owner: None,
            ..
        })
    ));
    tray.handle(&ctx, moved).await;

    assert!(!tray.claimed);
}
