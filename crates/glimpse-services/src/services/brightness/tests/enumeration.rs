use super::*;

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

#[tokio::test]
async fn reenumeration_preserves_pending_writes_while_devices_change() {
    let mut harness = harness(
        vec![
            entry("panel", "raw", 50, 100, None),
            entry("removed", "raw", 20, 100, None),
        ],
        0,
    )
    .await;
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
    harness
        .feed(Input::Event(Event::Enumerated(
            vec![
                entry("added", "firmware", 30, 100, None),
                entry("panel", "raw", 50, 100, None),
            ]
            .into(),
        )))
        .await;

    let sources = harness.state.borrow().sources.clone();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].id, "added");
    assert_eq!(sources[0].current, 30);
    assert_eq!(sources[1].id, "panel");
    assert_eq!(sources[1].current, 90);

    harness.settle_next().await;
    assert_eq!(harness.state.borrow().sources[1].current, 90);
    harness.settle_next().await;

    assert_eq!(
        harness.backend.writes(),
        vec![("panel".to_owned(), 80), ("panel".to_owned(), 90)]
    );
    assert!(result1.await.expect("answered").is_ok());
    assert!(result2.await.expect("answered").is_ok());
    assert_eq!(harness.state.borrow().sources[1].current, 90);
}

#[tokio::test]
async fn a_failed_write_after_reenumeration_rolls_back_to_the_confirmed_level() {
    let mut harness = harness(vec![entry("panel", "raw", 50, 100, None)], 0).await;
    harness.backend.fail(Some("write failed"));
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "panel".to_owned(),
            value: 80,
            reply,
        }))
        .await;
    harness
        .feed(Input::Event(Event::Enumerated(
            vec![entry("panel", "raw", 70, 100, None)].into(),
        )))
        .await;
    assert_eq!(harness.state.borrow().sources[0].current, 80);

    harness.settle_next().await;
    assert!(result.await.expect("answered").is_err());
    assert_eq!(harness.state.borrow().sources[0].current, 50);

    harness
        .feed(Input::Event(Event::Enumerated(
            vec![entry("panel", "raw", 75, 100, None)].into(),
        )))
        .await;
    assert_eq!(harness.state.borrow().sources[0].current, 75);
}

#[tokio::test]
async fn a_level_queued_for_a_removed_source_is_answered() {
    let mut harness = harness(vec![entry("monitor", "raw", 50, 100, None)], 0).await;
    let (reply1, _result1) = oneshot::channel();
    let (reply2, result2) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: "monitor".to_owned(),
            value: 80,
            reply: reply1,
        }))
        .await;
    harness
        .feed(Input::Command(Command::SetLevel {
            id: "monitor".to_owned(),
            value: 90,
            reply: reply2,
        }))
        .await;
    harness
        .feed(Input::Event(Event::Enumerated(Vec::new().into())))
        .await;

    assert!(matches!(
        result2.await.expect("answered"),
        Err(CommandError::Unavailable(reason)) if reason.contains("went away")
    ));
    assert!(harness.state.borrow().sources.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_ddc_monitor_that_misses_its_probes_appears_once_it_answers() {
    let mut harness = started(vec![entry("panel", "raw", 50, 100, None)], 0).await;
    harness.backend.answer_late(
        "card1-DP-2",
        3,
        entry("ddc:15", "ddc", 40, 100, Some("../../card1-DP-2")),
    );
    harness.enumerate().await;
    assert_eq!(harness.state.borrow().sources.len(), 1);

    harness
        .live
        .reconcile(&harness.ctx, harness.service.subscriptions());
    let reprobed = tokio::time::timeout(std::time::Duration::from_secs(60), harness.inbox.recv())
        .await
        .expect("the unanswered connector is reprobed without another output event")
        .expect("the inbox stays open");
    harness.feed(reprobed).await;

    let sources = harness.state.borrow().sources.clone();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[1].id, "ddc:15");
    assert_eq!(sources[1].connector.as_deref(), Some("DP-2"));
    assert_eq!(sources[1].current, 40);
}
