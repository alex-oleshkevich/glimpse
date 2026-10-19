use super::*;

#[test]
fn a_sender_naming_replaces_id_writes_over_the_notification_rather_than_sending_another() {
    let mut held = store(10, &[(1, "Telegram", "First"), (2, "PagerDuty", "Other")]);
    held.post(1, incoming("Telegram", "Edited"));

    let records = held.records();
    assert_eq!(
        records.len(),
        2,
        "a replacement is not a second notification"
    );
    let first = records
        .iter()
        .find(|record| record.id == 1)
        .expect("the replaced notification is still held");
    assert_eq!(first.summary, "Edited");
}

#[test]
fn the_bound_drops_only_old_read_history() {
    let mut held = store(
        2,
        &[(1, "A", "oldest"), (2, "B", "middle"), (3, "C", "newest")],
    );

    assert_eq!(
        held.records().len(),
        3,
        "unread notifications are never evicted"
    );
    assert!(held.dismiss(1));
    assert!(held.dismiss(2));
    assert!(held.dismiss(3));

    let summaries: Vec<_> = held
        .records()
        .into_iter()
        .map(|record| record.summary)
        .collect();
    assert_eq!(summaries, ["newest", "middle"]);
}

#[test]
fn suppression_patterns_match_application_title_and_body_before_storage() {
    let patterns = [
        "(?i)^org\\.example$".to_owned(),
        "(?i)build succeeded".to_owned(),
        "secret token".to_owned(),
    ];
    let mut held = Store::with_suppression(10, &patterns);

    assert!(!held.post(
        1,
        Incoming {
            app_id: "org.example".to_owned(),
            ..incoming("Friendly name", "Visible title")
        }
    ));
    assert!(!held.post(
        2,
        Incoming {
            summary: "Build succeeded".to_owned(),
            ..incoming("Builder", "unimportant")
        }
    ));
    assert!(!held.post(
        3,
        Incoming {
            body: "A secret token arrived".to_owned(),
            ..incoming("Builder", "unimportant")
        }
    ));
    assert!(held.records().is_empty());
}

#[test]
fn an_invalid_suppression_pattern_does_not_disable_valid_patterns() {
    let patterns = ["[".to_owned(), "quiet".to_owned()];
    let mut held = Store::with_suppression(10, &patterns);

    assert!(!held.post(1, incoming("quiet", "shown")));
    assert!(held.post(2, incoming("loud", "shown")));
    assert_eq!(held.records().len(), 1);
}

#[test]
fn adding_a_suppression_pattern_removes_matching_history() {
    let mut held = store(10, &[(1, "Chat", "Keep"), (2, "Build", "Done")]);

    assert!(held.reconfigure(10, &["Build".to_owned()]));
    assert_eq!(
        held.records()
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        [1]
    );
}

#[test]
fn lowering_the_bound_takes_effect_without_waiting_for_another_notification() {
    let mut held = store(10, &[(1, "A", "one"), (2, "B", "two"), (3, "C", "three")]);
    assert!(held.dismiss(1));
    assert!(held.dismiss(2));
    assert!(held.dismiss(3));
    assert!(
        held.reconfigure(1, &[]),
        "the store shrank, so the state must be republished"
    );
    assert_eq!(held.records().len(), 1);
    assert!(
        !held.reconfigure(1, &[]),
        "a bound that changes nothing must not force a republish"
    );
}

#[test]
fn clearing_one_app_leaves_every_other_app_alone() {
    let mut held = store(
        10,
        &[
            (1, "Telegram", "a"),
            (2, "PagerDuty", "b"),
            (3, "Telegram", "c"),
        ],
    );

    let gone = held.drain(|record| record.app_id == "Telegram");
    assert_eq!(gone, [3, 1], "every cleared id is reported, newest first");
    assert_eq!(held.records().len(), 1);
    assert_eq!(held.records()[0].app_id, "PagerDuty");
}

#[test]
fn an_action_is_invocable_only_on_a_notification_that_offered_it() {
    let mut held = Store::new(10);
    held.post(
        1,
        Incoming {
            actions: vec![("reply".to_owned(), "Reply".to_owned())],
            ..incoming("Telegram", "Marta")
        },
    );

    assert_eq!(held.offered(1, "reply"), Some(false));
    assert!(
        held.offered(1, "detonate").is_none(),
        "an action nobody offered is refused"
    );
    assert!(
        held.offered(2, "reply").is_none(),
        "an id nobody holds is refused"
    );
}

#[test]
fn dismissing_retains_a_read_history_entry_and_disables_its_actions() {
    let mut held = Store::new(10);
    held.post(
        1,
        Incoming {
            actions: vec![(DEFAULT_ACTION.to_owned(), "Open".to_owned())],
            ..incoming("Telegram", "Marta")
        },
    );

    assert!(held.dismiss(1));
    assert!(!held.records()[0].unread);
    assert!(held.offered(1, DEFAULT_ACTION).is_none());
    assert!(!held.dismiss(1), "a read entry is not closed twice");
}

#[test]
fn activation_reports_the_default_action_and_resident_policy_without_mutating() {
    let mut held = Store::new(10);
    held.post(1, incoming("Mail", "No action"));
    held.post(
        2,
        Incoming {
            actions: vec![(DEFAULT_ACTION.to_owned(), "Open".to_owned())],
            resident: true,
            ..incoming("Telegram", "Marta")
        },
    );

    assert_eq!(
        held.activation(1),
        Some(Activation {
            invoke_default: false,
            resident: false,
        })
    );
    assert_eq!(
        held.activation(2),
        Some(Activation {
            invoke_default: true,
            resident: true,
        })
    );
    assert!(held.records().iter().all(|record| record.unread));
}

#[test]
fn clearing_history_does_not_report_an_already_closed_id_again() {
    let mut held = store(10, &[(1, "Telegram", "Marta"), (2, "Mail", "New")]);
    assert!(held.dismiss(1));

    assert_eq!(held.drain(|_| true), [2]);
    assert!(held.records().is_empty());
}

#[test]
fn the_default_action_survives_the_named_button_cap_and_long_keys_are_dropped() {
    let too_long = "x".repeat(ACTION_KEY_MAX_CHARS + 1);
    let record = record(
        1,
        Incoming {
            actions: vec![
                ("one".to_owned(), "One".to_owned()),
                ("two".to_owned(), "Two".to_owned()),
                ("three".to_owned(), "Three".to_owned()),
                ("four".to_owned(), "Four".to_owned()),
                (too_long, "Wrong".to_owned()),
                (DEFAULT_ACTION.to_owned(), "Open".to_owned()),
            ],
            ..incoming("Telegram", "Marta")
        },
    );

    let keys: Vec<_> = record
        .actions
        .iter()
        .map(|action| action.key.as_str())
        .collect();
    assert_eq!(keys, [DEFAULT_ACTION, "one", "two", "three"]);
}

#[test]
fn progress_is_bounded_at_the_bus_and_record_boundaries() {
    assert_eq!(normalized_progress(-20), 0.0);
    assert_eq!(normalized_progress(25), 0.25);
    assert_eq!(normalized_progress(250), 1.0);

    let record = record(
        1,
        Incoming {
            progress: Some(2.5),
            ..incoming("Build", "Compiling")
        },
    );
    assert_eq!(record.progress, Some(1.0));
}

#[tokio::test]
async fn invoking_a_non_resident_action_closes_it_into_history() {
    let (handle, sender, mut state, cancel, running) = dnd_service().await;

    sender
        .send(Input::Event(Event::Posted {
            id: 1,
            incoming: Box::new(Incoming {
                actions: vec![(DEFAULT_ACTION.to_owned(), "Open".to_owned())],
                ..incoming("Telegram", "Marta")
            }),
        }))
        .await
        .expect("posted");
    state
        .wait_for(|state| {
            state.list.as_ref().is_some_and(|list| {
                list.notifications
                    .iter()
                    .any(|notification| notification.id == 1 && notification.unread)
            })
        })
        .await
        .expect("posted state");
    handle
        .invoke_action(1, DEFAULT_ACTION.to_owned(), None)
        .await
        .expect("invoked");
    state
        .wait_for(|state| {
            state.list.as_ref().is_some_and(|list| {
                list.notifications
                    .iter()
                    .any(|notification| notification.id == 1 && !notification.unread)
            })
        })
        .await
        .expect("dismissed state");
    let latest = handle.snapshot();
    cancel.cancel();
    running.await.expect("joined");

    let list = latest.list.expect("a list was published");
    assert_eq!(list.notifications.len(), 1);
    assert!(!list.notifications[0].unread);
}

#[tokio::test]
async fn bulk_clears_keep_a_request_still_waiting_for_an_answer() {
    let (handle, sender, mut state, cancel, running) = dnd_service().await;
    for (id, expire_timeout, action) in
        [(1, 0, "accept"), (2, -1, "accept"), (3, 0, DEFAULT_ACTION)]
    {
        sender
            .send(Input::Event(Event::Posted {
                id,
                incoming: Box::new(Incoming {
                    expire_timeout,
                    actions: vec![(action.to_owned(), "Go".to_owned())],
                    ..incoming("Pairing", "Request")
                }),
            }))
            .await
            .expect("posted");
    }
    state
        .wait_for(|state| {
            state
                .list
                .as_ref()
                .is_some_and(|list| list.notifications.len() == 3)
        })
        .await
        .expect("posted state");
    handle.clear_all().await.expect("cleared all");
    let after_all = handle.snapshot();
    handle
        .clear_app("Pairing".to_owned())
        .await
        .expect("cleared application");
    let after_app = handle.snapshot();
    cancel.cancel();
    running.await.expect("joined");

    for latest in [after_all, after_app] {
        let ids: Vec<u32> = latest
            .list
            .expect("a list was published")
            .notifications
            .iter()
            .map(|notification| notification.id)
            .collect();
        assert_eq!(ids, vec![1]);
    }
}

#[test]
fn dismissing_a_transient_notification_leaves_no_history() {
    let mut store = Store::new(10);
    store.post(
        1,
        Incoming {
            transient: true,
            ..incoming("Volume", "50%")
        },
    );
    store.post(2, incoming("Mail", "Hi"));

    assert!(store.dismiss(1));
    assert!(store.dismiss(2));
    let ids: Vec<u32> = store.records().iter().map(|record| record.id).collect();
    assert_eq!(ids, vec![2]);
}

#[tokio::test]
async fn a_timed_out_popup_removes_only_a_transient_record() {
    let (handle, sender, mut state, cancel, running) = dnd_service().await;
    for (id, transient) in [(1, true), (2, false)] {
        sender
            .send(Input::Event(Event::Posted {
                id,
                incoming: Box::new(Incoming {
                    transient,
                    ..incoming("Volume", "50%")
                }),
            }))
            .await
            .expect("posted");
    }
    state
        .wait_for(|state| {
            state
                .list
                .as_ref()
                .is_some_and(|list| list.notifications.len() == 2)
        })
        .await
        .expect("posted state");
    handle.expire(1).await.expect("expired transient");
    handle.expire(2).await.expect("expired ordinary");
    let latest = handle.snapshot();
    cancel.cancel();
    running.await.expect("joined");

    let list = latest.list.expect("a list was published");
    let ids: Vec<u32> = list.notifications.iter().map(|record| record.id).collect();
    assert_eq!(ids, vec![2]);
    assert!(list.notifications[0].unread);
}
