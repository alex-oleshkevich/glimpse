use super::*;

#[tokio::test]
async fn a_scan_is_declared_only_while_one_is_running_and_stops_when_the_radio_goes_off() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    assert!(
        !service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::ScanDeadline(_))),
        "nothing may start a scan on its own"
    );

    service.begin_scan(Hold::Timed);
    assert!(
        service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::ScanDeadline(1)))
    );

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: ADAPTER.to_owned(),
                interface: bluez::ADAPTER1.to_owned(),
                changed: properties(vec![("PowerState", "off".into())]),
                invalidated: Vec::new(),
            }),
        )
        .await;

    assert_eq!(service.scan, None);
}

#[tokio::test]
async fn the_published_scanning_flag_is_ours_and_not_the_adapters() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    assert!(!state.borrow().scanning());

    service.begin_scan(Hold::Timed);
    service.publish();
    assert!(state.borrow().scanning());

    service
        .handle(&ctx, Input::Event(Event::ScanExpired(1)))
        .await;
    service.publish();
    assert!(
        !state.borrow().scanning(),
        "the deadline stopping the scan must reach the popover"
    );
}

#[tokio::test]
async fn a_held_scan_carries_no_deadline_and_a_timed_one_does() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service.begin_scan(Hold::Timed);
    service.publish();
    assert!(service.deadline.is_some());
    assert!(
        !state.borrow().held(),
        "the bar must not light for a scan the popover started"
    );

    service.begin_scan(Hold::Held);
    service.publish();
    assert_eq!(
        service.deadline, None,
        "a scan the user switched on runs until they switch it off"
    );
    assert!(state.borrow().held());

    assert!(
        !service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::ScanDeadline(_))),
        "a held scan must declare no deadline source at all"
    );
}

#[tokio::test]
async fn bluez_going_away_clears_a_scan_that_cannot_have_survived_it() {
    for owner in [Some("a".to_owned()), None] {
        let (mut service, ctx, state, _health) = bluetooth().await;
        enumerated(&mut service, &ctx, session()).await;
        service.begin_scan(Hold::Held);
        service.publish();
        assert!(state.borrow().scanning());

        service
            .handle(&ctx, Input::Event(Event::NameOwner(owner.clone())))
            .await;

        assert!(
            !state.borrow().scanning(),
            "the session died with the daemon, and a held scan has no deadline to retire it"
        );
        assert_eq!(service.deadline, None);
    }
}

#[tokio::test]
async fn an_expired_deadline_from_a_previous_scan_does_not_stop_the_current_one() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    service.begin_scan(Hold::Timed);
    service.begin_scan(Hold::Timed);

    service
        .handle(&ctx, Input::Event(Event::ScanExpired(1)))
        .await;

    assert_eq!(service.scan, Some((2, Hold::Timed)));
}

#[tokio::test]
async fn a_zero_timeout_declares_no_deadline_at_all() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    service.config.scan_timeout = None;
    service.begin_scan(Hold::Timed);
    let _ = &ctx;

    assert_eq!(service.scan, Some((1, Hold::Timed)), "the scan still runs");
    assert!(
        !service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::ScanDeadline(_))),
        "with no timeout there is nothing to stop it but the popover"
    );
}

#[tokio::test]
async fn missed_owner_messages_re_read_without_dropping_a_running_scan() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    service.begin_scan(Hold::Held);
    service.publish();
    let before = service.generation;

    service.handle(&ctx, Input::Event(Event::OwnerMissed)).await;

    assert!(state.borrow().scanning());
    assert_eq!(service.scan, Some((1, Hold::Held)));
    assert_ne!(service.generation, before);
    assert!(!service.adopted);
}
