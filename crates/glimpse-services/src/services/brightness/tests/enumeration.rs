use super::*;

#[tokio::test]
async fn a_fresh_service_before_enumeration_is_empty_and_not_degraded() {
    let harness = started(Vec::new(), 3).await;

    assert!(harness.state.borrow().sources.is_empty());
    assert!(!matches!(
        &*harness.health.borrow(),
        ServiceState::Degraded { .. }
    ));
}

#[tokio::test]
async fn enumeration_runs_through_the_declared_subscription() {
    let harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;

    assert_eq!(harness.state.borrow().sources.len(), 1);
    assert_eq!(harness.state.borrow().sources[0].id, "panel");
}

#[tokio::test]
async fn the_declared_subscriptions_include_a_live_udev_source() {
    let harness = harness(vec![entry("panel", "raw", 50, 100, None)], 3).await;
    let subs = harness.service.subscriptions();

    assert!(subs.iter().any(|sub| *sub.key() == Watch::Enumerate));
    assert!(subs.iter().any(|sub| *sub.key() == Watch::Udev));
    assert!(subs.iter().any(|sub| *sub.key() == Watch::DrmHotplug));
    assert!(subs.iter().any(|sub| *sub.key() == Watch::Keyboard));
}

#[tokio::test]
async fn a_rescan_touches_only_the_device_it_names() {
    let mut harness = harness(
        vec![
            entry("panel", "raw", 50, 100, None),
            entry("kbd", "raw", 20, 100, None),
        ],
        0,
    )
    .await;

    harness
        .feed(Input::Event(Event::Rescanned {
            id: "panel".to_owned(),
            entry: Some(entry("panel", "raw", 65, 100, None)),
        }))
        .await;

    let sources = harness.state.borrow().sources.clone();
    let panel = sources
        .iter()
        .find(|source| source.id == "panel")
        .expect("the panel source is still published");
    let keyboard = sources
        .iter()
        .find(|source| source.id == "kbd")
        .expect("the keyboard source is untouched");
    assert_eq!(panel.current, 65);
    assert_eq!(keyboard.current, 20);
}

#[tokio::test]
async fn a_rescan_reporting_no_entry_drops_the_vanished_source() {
    let mut harness = harness(
        vec![
            entry("panel", "raw", 50, 100, None),
            entry("kbd", "raw", 20, 100, None),
        ],
        0,
    )
    .await;

    harness
        .feed(Input::Event(Event::Rescanned {
            id: "panel".to_owned(),
            entry: None,
        }))
        .await;

    let sources = harness.state.borrow().sources.clone();
    assert!(sources.iter().all(|source| source.id != "panel"));
    assert!(sources.iter().any(|source| source.id == "kbd"));
}

#[tokio::test]
async fn a_rescan_after_our_own_write_settles_does_not_trigger_a_second_write() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 80,
            reply,
        }))
        .await;
    harness.settle_next().await;
    assert!(result.await.expect("answered").is_ok());
    assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 80)]);

    harness
        .feed(Input::Event(Event::Rescanned {
            id: "panel".to_owned(),
            entry: Some(entry("panel", "raw", 80, 100, None)),
        }))
        .await;

    assert_eq!(harness.backend.writes(), vec![("panel".to_owned(), 80)]);
}

#[tokio::test]
async fn a_refresh_command_rereads_every_known_source() {
    let mut harness = harness(
        vec![
            entry("panel", "raw", 50, 100, None),
            entry("kbd", "raw", 20, 100, None),
        ],
        0,
    )
    .await;
    harness.backend.set_brightness("panel", 66);
    harness.backend.set_brightness("kbd", 33);
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::Refresh { reply }))
        .await;
    assert!(result.await.expect("answered").is_ok());

    harness.settle_next().await;
    harness.settle_next().await;

    let sources = harness.state.borrow().sources.clone();
    let panel = sources
        .iter()
        .find(|source| source.id == "panel")
        .expect("the panel source is still published");
    let keyboard = sources
        .iter()
        .find(|source| source.id == "kbd")
        .expect("the keyboard source is still published");
    assert_eq!(panel.current, 66);
    assert_eq!(keyboard.current, 33);
}

#[tokio::test]
async fn a_rescan_reconciles_a_value_that_moved_out_of_band() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
    assert_eq!(harness.state.borrow().sources[0].current, 50);

    harness
        .feed(Input::Event(Event::Rescanned {
            id: "panel".to_owned(),
            entry: Some(entry("panel", "raw", 77, 100, None)),
        }))
        .await;

    assert_eq!(harness.state.borrow().sources[0].current, 77);
}

#[tokio::test]
async fn a_rescan_naming_the_in_flight_write_does_not_clobber_a_newer_queued_value() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
    let (reply1, result1) = oneshot::channel();
    let (reply2, result2) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 80,
            reply: reply1,
        }))
        .await;
    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 90,
            reply: reply2,
        }))
        .await;
    assert_eq!(harness.state.borrow().sources[0].current, 90);

    harness
        .feed(Input::Event(Event::Rescanned {
            id: "panel".to_owned(),
            entry: Some(entry("panel", "raw", 80, 100, None)),
        }))
        .await;
    assert_eq!(harness.state.borrow().sources[0].current, 90);

    harness.settle_next().await;
    harness.settle_next().await;

    assert_eq!(
        harness.backend.writes(),
        vec![("panel".to_owned(), 80), ("panel".to_owned(), 90)]
    );
    assert!(result1.await.expect("answered").is_ok());
    assert!(result2.await.expect("answered").is_ok());
    assert_eq!(harness.state.borrow().sources[0].current, 90);
}
