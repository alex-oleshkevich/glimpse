use super::*;

const TEXT: &str = "text/plain;charset=utf-8";

fn limits() -> Limits {
    Limits {
        entries: 3,
        max_entry_bytes: 64,
        max_total_bytes: 256,
        images: true,
    }
}

fn capture(mime: &str, data: &[u8]) -> Capture {
    Capture {
        mime: mime.to_owned(),
        data: Arc::from(data),
        sensitive: false,
    }
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000, 0).expect("a fixed instant")
}

fn put(history: &mut History, body: &str) -> Result<bool, Refused> {
    history.insert(capture(TEXT, body.as_bytes()), now())
}

#[test]
fn a_fresh_history_holds_nothing_before_any_event_arrives() {
    let history = History::new(limits());

    assert!(history.entries().is_empty());
    assert_eq!(history.total_bytes(), 0);
}

#[test]
fn a_capture_lands_at_the_front() {
    let mut history = History::new(limits());

    assert_eq!(put(&mut history, "first"), Ok(true));
    assert_eq!(put(&mut history, "second"), Ok(true));

    let previews: Vec<_> = history
        .entries()
        .iter()
        .map(|e| e.preview.clone())
        .collect();
    assert_eq!(previews, ["second", "first"]);
}

#[test]
fn a_recaptured_entry_moves_to_the_front_rather_than_duplicating() {
    let mut history = History::new(limits());
    put(&mut history, "first").expect("recorded");
    put(&mut history, "second").expect("recorded");

    put(&mut history, "first").expect("recorded");

    assert_eq!(
        history.entries().len(),
        2,
        "the history must not have grown"
    );
    assert_eq!(history.entries()[0].preview, "first");
}

/// The echo of our own `offer` arrives as an ordinary capture and is collapsed by the same
/// path. Nothing else guards it, so this is the assertion that keeps the loop closed.
#[test]
fn re_reading_what_was_just_offered_does_not_grow_the_history() {
    let mut history = History::new(limits());
    put(&mut history, "only").expect("recorded");
    let offered = history.entries()[0].offer();

    history
        .insert(
            Capture {
                mime: offered.mime,
                data: offered.data,
                sensitive: false,
            },
            now(),
        )
        .expect("recorded");

    assert_eq!(history.entries().len(), 1);
}

#[test]
fn a_sensitive_capture_is_refused_before_it_is_stored() {
    let mut history = History::new(limits());

    let refused = history.insert(
        Capture {
            mime: TEXT.to_owned(),
            data: Arc::from(&b"hunter2"[..]),
            sensitive: true,
        },
        now(),
    );

    assert_eq!(refused, Err(Refused::Sensitive));
    assert!(history.entries().is_empty());
}

#[test]
fn an_entry_at_the_cap_is_kept_and_one_byte_past_it_is_refused() {
    let mut history = History::new(limits());
    let at_cap = "a".repeat(64);
    let past_cap = "a".repeat(65);

    assert_eq!(put(&mut history, &at_cap), Ok(true));
    assert_eq!(put(&mut history, &past_cap), Err(Refused::TooLarge));
    assert_eq!(history.entries().len(), 1);
}

#[test]
fn an_image_is_refused_when_images_are_off_and_kept_when_they_are_on() {
    let png = capture("image/png", b"\x89PNG not really");
    let mut on = History::new(limits());
    let mut off = History::new(Limits {
        images: false,
        ..limits()
    });

    assert_eq!(on.insert(png.clone(), now()), Ok(true));
    assert_eq!(off.insert(png, now()), Err(Refused::ImagesDisabled));
    assert_eq!(on.entries()[0].kind, Kind::Image);
    assert!(
        on.entries()[0].preview.is_empty(),
        "the service has no decoder and must not invent a preview"
    );
}

#[test]
fn an_unsupported_mime_is_refused_rather_than_stored_blank() {
    let mut history = History::new(limits());

    let refused = history.insert(capture("application/x-secret", b"..."), now());

    assert_eq!(refused, Err(Refused::UnsupportedKind));
}

#[test]
fn the_oldest_unpinned_entry_is_dropped_once_the_count_is_exceeded() {
    let mut history = History::new(limits());
    for body in ["one", "two", "three", "four"] {
        put(&mut history, body).expect("recorded");
    }

    let previews: Vec<_> = history
        .entries()
        .iter()
        .map(|e| e.preview.clone())
        .collect();
    assert_eq!(previews, ["four", "three", "two"]);
}

#[test]
fn the_byte_budget_evicts_even_when_the_count_would_not() {
    let mut history = History::new(Limits {
        entries: 100,
        max_total_bytes: 20,
        ..limits()
    });

    put(&mut history, &"a".repeat(15)).expect("recorded");
    put(&mut history, &"b".repeat(15)).expect("recorded");

    assert_eq!(history.entries().len(), 1);
    assert!(history.total_bytes() <= 20);
}

#[test]
fn a_pinned_entry_outlives_eviction() {
    let mut history = History::new(limits());
    put(&mut history, "keep me").expect("recorded");
    let pinned = history.entries()[0].id;
    assert!(history.pin(pinned, true));

    for body in ["one", "two", "three", "four", "five"] {
        put(&mut history, body).expect("recorded");
    }

    assert!(
        history.get(pinned).is_some(),
        "a pin is the one statement the viewer made about an entry"
    );
}

#[test]
fn clearing_forgets_everything_except_the_pins() {
    let mut history = History::new(limits());
    put(&mut history, "keep me").expect("recorded");
    let pinned = history.entries()[0].id;
    history.pin(pinned, true);
    put(&mut history, "forget me").expect("recorded");

    assert!(history.clear());

    assert_eq!(history.entries().len(), 1);
    assert_eq!(history.entries()[0].id, pinned);
}

#[test]
fn a_removed_entry_stays_gone_until_it_is_copied_again() {
    let mut history = History::new(limits());
    put(&mut history, "first").expect("recorded");
    let id = history.entries()[0].id;
    put(&mut history, "second").expect("recorded");

    assert!(history.remove(id));
    assert!(!history.remove(id), "removing it twice changes nothing");
    put(&mut history, "third").expect("recorded");

    assert!(
        history.get(id).is_none(),
        "an unrelated capture must not resurrect it"
    );
    assert_eq!(history.entries().len(), 2);
}

/// A preview is another application's text. Cutting it by byte would split a codepoint, and the
/// cap has to survive one that is four bytes wide.
#[test]
fn a_multi_byte_preview_is_capped_by_character_and_does_not_panic() {
    let mut history = History::new(Limits {
        max_entry_bytes: 8192,
        max_total_bytes: 8192,
        ..limits()
    });
    let long = "Марта🙂".repeat(200);

    put(&mut history, &long).expect("recorded");

    let preview = &history.entries()[0].preview;
    assert!(preview.chars().count() <= PREVIEW_CHARS + 1);
    assert!(preview.starts_with("Марта🙂"));
}

#[test]
fn a_preview_is_flattened_to_one_line() {
    let mut history = History::new(limits());

    put(&mut history, "one\ntwo\tthree").expect("recorded");

    assert_eq!(history.entries()[0].preview, "one two three");
}

#[test]
fn lowering_the_limits_evicts_at_once_rather_than_on_the_next_capture() {
    let mut history = History::new(limits());
    for body in ["one", "two", "three"] {
        put(&mut history, body).expect("recorded");
    }

    assert!(history.set_limits(Limits {
        entries: 1,
        ..limits()
    }));

    assert_eq!(history.entries().len(), 1);
    assert_eq!(history.entries()[0].preview, "three");
}

/// The defect this exists for: the entry was pushed first and `evict` removed oldest-first, so
/// everything else unpinned was destroyed before the offender was finally dropped — and
/// `insert` answered `Ok`.
#[test]
fn a_capture_too_large_for_the_whole_budget_is_refused_and_destroys_nothing() {
    let mut history = History::new(Limits {
        entries: 100,
        max_entry_bytes: 4096,
        max_total_bytes: 64,
        images: true,
    });
    put(&mut history, "keep one").expect("recorded");
    put(&mut history, "keep two").expect("recorded");

    let refused = put(&mut history, &"a".repeat(128));

    assert_eq!(refused, Err(Refused::TooLarge));
    assert_eq!(
        history.entries().len(),
        2,
        "a refused capture must not evict anything"
    );
}

/// Pins were counted against the byte budget, so a history pinned to the cap evicted every new
/// entry the instant it arrived — an empty clipboard with nothing to explain it.
#[test]
fn a_history_pinned_to_the_byte_cap_still_records_new_entries() {
    let mut history = History::new(Limits {
        entries: 100,
        max_entry_bytes: 4096,
        max_total_bytes: 64,
        images: true,
    });
    put(&mut history, &"p".repeat(60)).expect("recorded");
    let pinned = history.entries()[0].id;
    assert!(history.pin(pinned, true));

    put(&mut history, "small").expect("recorded");

    assert_eq!(history.entries().len(), 2);
    assert_eq!(history.entries()[0].preview, "small");
    assert!(history.get(pinned).is_some());
}

/// `kind_of` accepts five spellings of text, so the same line arrives under different mimes.
/// Fingerprinting the spelling filed them as two rows of identical content.
#[test]
fn the_same_text_under_two_mime_spellings_is_one_entry() {
    let mut history = History::new(limits());

    history
        .insert(capture("text/plain;charset=utf-8", b"same"), now())
        .expect("recorded");
    history
        .insert(capture("UTF8_STRING", b"same"), now())
        .expect("recorded");

    assert_eq!(
        history.entries().len(),
        1,
        "identical content is one entry however it was spelled"
    );
}

#[test]
fn forgetting_everything_takes_the_pins_too() {
    let mut history = History::new(limits());
    put(&mut history, "pinned").expect("recorded");
    let id = history.entries()[0].id;
    history.pin(id, true);

    assert!(history.forget_all());

    assert!(history.entries().is_empty());
}

/// `DefaultHasher` has fixed keys, so its output is byte-identical in every process and an
/// application choosing clipboard content could craft a collision. Two instances must not agree.
#[test]
fn the_fingerprint_is_seeded_per_instance_rather_than_fixed() {
    let one = History::new(limits());
    let two = History::new(limits());

    assert_ne!(
        one.fingerprint(Kind::Text, b"same bytes"),
        two.fingerprint(Kind::Text, b"same bytes"),
        "a fixed-seed fingerprint is forgeable by the application that chose the content"
    );
    assert_eq!(
        one.fingerprint(Kind::Text, b"same bytes"),
        one.fingerprint(Kind::Text, b"same bytes"),
        "but it must still be stable within one history"
    );
}

/// Taking the dedup branch on a collision would discard the bytes just copied and leave the
/// older entry in place — the viewer pastes something other than what they copied.
#[test]
fn colliding_content_is_kept_apart_rather_than_silently_substituted() {
    let mut history = History::new(limits());
    put(&mut history, "first").expect("recorded");
    let id = history.entries()[0].id;

    // Forge the collision the hash is meant to prevent: a second entry carrying the first's id.
    history.entries[0].id = 7;
    put(&mut history, "second").expect("recorded");
    history.entries[0].id = 7;

    assert_eq!(
        history.entries().len(),
        2,
        "two different contents must stay two entries even under one id"
    );
    assert_eq!(history.entries()[0].preview, "second");
    let _ = id;
}

#[test]
fn an_entry_is_debugged_without_its_content() {
    let mut history = History::new(limits());
    put(&mut history, "hunter2").expect("recorded");

    let rendered = format!("{:?}", history.entries()[0]);

    assert!(!rendered.contains("hunter2"), "got {rendered}");
    assert!(rendered.contains("bytes: 7"), "got {rendered}");
}
