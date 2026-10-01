use super::*;

#[test]
fn more_actions_than_the_cap_are_trimmed_and_a_keyless_one_is_dropped() {
    let many = record(
        1,
        Incoming {
            actions: vec![
                ("a".to_owned(), "A".to_owned()),
                ("b".to_owned(), "B".to_owned()),
                ("c".to_owned(), "C".to_owned()),
                ("d".to_owned(), "D".to_owned()),
            ],
            ..incoming("Telegram", "Marta")
        },
    );
    assert_eq!(many.actions.len(), ACTIONS_MAX);

    let keyless = record(
        2,
        Incoming {
            actions: vec![(String::new(), "Nameless".to_owned())],
            ..incoming("Telegram", "Marta")
        },
    );
    assert!(
        keyless.actions.is_empty(),
        "an action with no key can never be sent back"
    );
}

/// The wire carries actions as one flat list of alternating key and label, so an odd length is
/// a sender's mistake rather than a final action with no label.
#[test]
fn actions_arrive_as_pairs_and_an_odd_tail_is_dropped() {
    assert_eq!(
        pairs(vec![
            "reply".to_owned(),
            "Reply".to_owned(),
            "orphan".to_owned()
        ]),
        [("reply".to_owned(), "Reply".to_owned())]
    );
    assert!(pairs(Vec::new()).is_empty());
}

/// `clean` spends the cap on content and then adds `…` as a marker, so an overflowing string
/// is `cap + 1` characters. Asserting the raw length would pin the marker rather than the
/// budget, and would move the day the marker changes.
#[test]
fn a_summary_from_a_sender_is_capped_and_says_that_it_was() {
    let long = record(1, incoming("Telegram", &"A".repeat(SUMMARY_MAX_CHARS * 2)));

    assert!(long.summary.ends_with('…'));
    assert_eq!(
        long.summary.chars().filter(|glyph| *glyph != '…').count(),
        SUMMARY_MAX_CHARS
    );
}

/// A summary is somebody else's text and need not be ASCII. Capping by bytes would cut inside
/// a character, which is a panic reachable by anyone who can send a notification.
#[test]
fn a_multi_byte_summary_is_capped_by_characters_rather_than_by_bytes() {
    let long = record(1, incoming("Telegram", &"é".repeat(SUMMARY_MAX_CHARS * 2)));

    assert_eq!(
        long.summary.chars().filter(|glyph| *glyph != '…').count(),
        SUMMARY_MAX_CHARS
    );
    assert!(
        long.summary.len() > SUMMARY_MAX_CHARS,
        "each character is two bytes here"
    );
}

#[test]
fn a_token_that_is_empty_or_over_the_bound_is_dropped_rather_than_shortened() {
    assert_eq!(
        valid_activation_token(Some("tok-1".to_owned())).as_deref(),
        Some("tok-1")
    );
    assert_eq!(
        valid_activation_token(None),
        None,
        "a client that cannot mint one leaves it out"
    );
    assert_eq!(valid_activation_token(Some(String::new())), None);
    assert_eq!(
        valid_activation_token(Some("t".repeat(ACTIVATION_TOKEN_MAX_CHARS + 1))),
        None,
        "an overlong token is refused rather than truncated into a wrong one"
    );
}

#[test]
fn a_notification_carries_the_pid_of_the_process_that_sent_it() {
    let held = record(
        1,
        Incoming {
            app_pid: Some(4265),
            ..incoming("Telegram", "Marta")
        },
    );

    assert_eq!(held.app_pid, Some(4265));
}

#[test]
fn ids_are_handed_out_in_order_and_a_sender_naming_one_gets_it_back() {
    let next = Ids::default();

    assert_eq!(next.allocate(0), 1);
    assert_eq!(next.allocate(0), 2);
    assert_eq!(next.allocate(7), 7);
    assert_eq!(
        next.allocate(0),
        3,
        "naming a replacement does not spend an id"
    );
}

/// Zero means "new", so handing it out would make the next sender's replacement read as a
/// fresh notification.
#[test]
fn the_id_counter_steps_over_zero_when_it_wraps() {
    let next = Ids(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(
        u32::MAX,
    )));

    assert_eq!(next.allocate(0), u32::MAX);
    assert_eq!(next.allocate(0), 1);
}
