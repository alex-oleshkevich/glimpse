use super::*;

#[tokio::test]
async fn a_calendar_with_no_sources_publishes_an_empty_list_and_stays_healthy() {
    let mock = settled(Vec::new(), |mock| !published(mock).is_empty()).await;

    assert_eq!(published(&mock).last(), Some(&Vec::new()));
    assert!(
        !mock
            .health()
            .iter()
            .any(|state| matches!(state, ServiceState::Degraded { .. })),
        "nothing configured is a working calendar, got {:?}",
        mock.health()
    );
}

#[tokio::test]
async fn a_source_that_cannot_be_read_degrades_without_naming_its_uri() {
    let uri = "file:///nonexistent/glimpse-test-secret-token.ics";
    let mock = settled(
        vec![source("work", CalendarSourceKind::Ical, uri)],
        |mock| {
            mock.health()
                .iter()
                .any(|state| matches!(state, ServiceState::Degraded { .. }))
        },
    )
    .await;

    let degraded: Vec<String> = mock
        .health()
        .into_iter()
        .filter_map(|state| match state {
            ServiceState::Degraded { reason } => Some(reason),
            _ => None,
        })
        .collect();

    assert!(
        degraded.iter().any(|reason| reason.contains("work")),
        "the reason names the source that failed, got {degraded:?}"
    );
    assert!(
        !degraded
            .iter()
            .any(|reason| reason.contains("secret-token")),
        "the reason must never carry the uri, got {degraded:?}"
    );
}

#[tokio::test]
async fn a_directory_source_reads_the_ics_files_in_it() {
    let root = tempfile::tempdir().expect("a scratch directory");
    tokio::fs::write(root.path().join("a.ics"), DOCUMENT)
        .await
        .expect("a written fixture");
    tokio::fs::write(root.path().join("notes.txt"), "ignored")
        .await
        .expect("a written decoy");

    let read = read(
        None,
        CalendarSourceKind::Directory,
        root.path().to_string_lossy().into_owned(),
    )
    .await;

    let fetched = read.expect("a readable directory");
    assert!(
        !fetched.remote,
        "a directory never reaches the network, so it must not be given a timer"
    );
    let summaries: BTreeSet<String> = fetched
        .calendars
        .iter()
        .flat_map(|calendar| expand(calendar, window()))
        .map(|occurrence| occurrence.summary)
        .collect();
    assert!(summaries.contains("Standup"));
    assert!(summaries.contains("Holiday"));
}

#[test]
fn the_cap_applies_to_the_merged_list_rather_than_to_each_source() {
    let base = now();
    let early = parsed(&crowded(base, EVENTS));
    let late = parsed(&crowded(base + TimeDelta::days(1), EVENTS));

    let merged = payload(expanding(
        0,
        vec![loaded("early", early), loaded("late", late)],
        window(),
    ));

    assert_eq!(merged.events.len(), EVENTS);
    let mark = merged.truncated_from.expect("a truncated payload");
    assert!(
        merged.events.iter().all(|kept| kept.start <= mark),
        "everything kept starts no later than the first entry dropped"
    );
    assert!(
        merged.events.iter().any(|event| event.source == "early")
            && merged.events.iter().any(|event| event.source == "late"),
        "both sources reach the payload, so the cap fell on the merge"
    );
}

#[tokio::test]
async fn a_source_that_comes_back_under_the_cap_clears_its_mark() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let file = root.path().join("crowd.ics");
    let base = whole_second();
    tokio::fs::write(&file, crowded(base, EVENTS + 20))
        .await
        .expect("a written fixture");

    let live = Live::start(vec![source(
        "crowd",
        CalendarSourceKind::Directory,
        &root.path().to_string_lossy(),
    )]);
    assert!(
        live.until(|mock| marks(mock).iter().any(|mark| mark.is_some()))
            .await,
        "the crowded directory is marked to begin with"
    );

    tokio::fs::write(&file, crowded(base, 3))
        .await
        .expect("a rewritten fixture");
    let cleared = live.until(|mock| marks(mock).last() == Some(&None)).await;

    let mock = live.stop().await;
    assert!(
        cleared,
        "shrinking under the cap clears the mark, got {:?}",
        marks(&mock)
    );
}

#[tokio::test]
async fn removing_a_truncated_source_removes_its_mark() {
    let root = tempfile::tempdir().expect("a scratch directory");
    tokio::fs::write(
        root.path().join("crowd.ics"),
        crowded(whole_second(), EVENTS + 20),
    )
    .await
    .expect("a written fixture");

    let live = Live::start(vec![source(
        "crowd",
        CalendarSourceKind::Directory,
        &root.path().to_string_lossy(),
    )]);
    assert!(
        live.until(|mock| marks(mock).iter().any(|mark| mark.is_some()))
            .await,
        "the crowded source is marked to begin with"
    );

    live.sender
        .send(Input::Config(Config {
            poll_interval: MIN_POLL,
            sources: Vec::new(),
        }))
        .await
        .expect("queued");
    let gone = live
        .until(|mock| {
            marks(mock).last() == Some(&None) && published(mock).last() == Some(&Vec::new())
        })
        .await;

    let mock = live.stop().await;
    assert!(
        gone,
        "the mark and the events went with the source, got {:?}",
        marks(&mock)
    );
}

#[tokio::test]
async fn a_source_over_the_cap_says_where_its_list_stops() {
    let payload = crowd(EVENTS + 40).await;

    assert_eq!(payload.events.len(), EVENTS);
    let from = payload.truncated_from.expect("a truncated payload");
    assert_eq!(
        from,
        payload.events[EVENTS - 1].start + TimeDelta::minutes(30),
        "the mark is the first entry dropped, not the last one kept"
    );
    assert!(
        payload.events.iter().all(|kept| kept.start < from),
        "everything published is complete strictly before the mark"
    );
}

#[tokio::test]
async fn a_source_inside_the_cap_carries_no_mark_at_all() {
    let payload = crowd(12).await;

    assert_eq!(payload.events.len(), 12);
    assert_eq!(payload.truncated_from, None);
}

#[tokio::test]
async fn a_watched_directory_is_read_at_start_and_again_when_a_file_arrives() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let at = Utc::now() + TimeDelta::hours(1);
    tokio::fs::write(
        root.path().join("first.ics"),
        ics("one@example", "First", at),
    )
    .await
    .expect("a written fixture");

    let live = Live::start(vec![source(
        "local",
        CalendarSourceKind::Directory,
        &root.path().to_string_lossy(),
    )]);

    assert!(
        live.until(|mock| holds(mock, "First")).await,
        "the watch opens with a read, since nothing else ever reads a directory"
    );

    tokio::fs::write(
        root.path().join("second.ics"),
        ics("two@example", "Second", at),
    )
    .await
    .expect("a written fixture");
    let arrived = live.until(|mock| holds(mock, "Second")).await;

    live.stop().await;
    assert!(arrived, "a file dropped in reached the topic");
}

#[test]
fn only_a_source_that_reaches_the_network_is_given_a_timer() {
    let fresh = false;
    let sidecar = true;

    let table = [
        (
            "dir bare path",
            source("d", CalendarSourceKind::Directory, "/tmp/cal"),
            fresh,
            true,
            false,
        ),
        (
            "dir file url",
            source("d", CalendarSourceKind::Directory, "file:///tmp/cal"),
            fresh,
            true,
            false,
        ),
        (
            "dir given a feed",
            source(
                "d",
                CalendarSourceKind::Directory,
                "https://example.test/a.ics",
            ),
            fresh,
            true,
            false,
        ),
        (
            "ical https",
            source("h", CalendarSourceKind::Ical, "https://example.test/a.ics"),
            fresh,
            false,
            true,
        ),
        (
            "ical bare path",
            source("f", CalendarSourceKind::Ical, "/tmp/a.ics"),
            fresh,
            true,
            false,
        ),
        (
            "ical file url",
            source("f", CalendarSourceKind::Ical, "file:///tmp/a.ics"),
            fresh,
            true,
            false,
        ),
        (
            "ical webcal",
            source("w", CalendarSourceKind::Ical, "webcal://x.test/a.ics"),
            fresh,
            false,
            true,
        ),
        (
            "sidecar, known remote",
            source("side", CalendarSourceKind::Ical, "file:///tmp/a.url"),
            sidecar,
            true,
            true,
        ),
    ];

    for (name, source, remote, watch, timer) in table {
        let got = declares(&source, remote);
        assert_eq!(got.watch, watch, "{name}: watch");
        assert_eq!(got.timer, timer, "{name}: timer");
    }
}

#[tokio::test]
async fn a_sidecar_resolves_to_a_fetch_and_a_plain_file_does_not() {
    let root = tempfile::tempdir().expect("a scratch directory");

    let pointer = root.path().join("work.url");
    tokio::fs::write(&pointer, "https://example.test/private/a.ics\n")
        .await
        .expect("a written sidecar");
    let resolved = resolve(&pointer.to_string_lossy())
        .await
        .expect("a readable sidecar");
    assert!(
        resolved.remote(),
        "a sidecar names a calendar on the network"
    );
    assert!(matches!(resolved, Fetching::Remote(_)));

    let document = root.path().join("a.ics");
    tokio::fs::write(&document, DOCUMENT)
        .await
        .expect("a written calendar");
    let resolved = resolve(&document.to_string_lossy())
        .await
        .expect("a readable calendar");
    assert!(
        !resolved.remote(),
        "a local calendar is watched, not fetched"
    );
    assert!(matches!(resolved, Fetching::Document(_)));
}

#[tokio::test]
async fn a_directory_source_pointed_at_a_feed_says_so_instead_of_going_quiet() {
    let mock = settled(
        vec![source(
            "wrong",
            CalendarSourceKind::Directory,
            "https://example.test/a.ics",
        )],
        |mock| {
            mock.health()
                .iter()
                .any(|state| matches!(state, ServiceState::Degraded { .. }))
        },
    )
    .await;

    assert!(
        mock.health().iter().any(|state| matches!(
            state,
            ServiceState::Degraded { reason } if reason.contains("wrong")
        )),
        "expected a Degraded naming the source, got {:?}",
        mock.health()
    );
}

#[tokio::test]
async fn a_local_ical_file_is_re_read_when_it_changes() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let file = root.path().join("one.ics");
    let at = Utc::now() + TimeDelta::hours(1);
    tokio::fs::write(&file, ics("one@example", "Before", at))
        .await
        .expect("a written fixture");

    let live = Live::start(vec![source(
        "localfile",
        CalendarSourceKind::Ical,
        &format!("file://{}", file.display()),
    )]);

    assert!(
        live.until(|mock| holds(mock, "Before")).await,
        "the watch opens with a read"
    );

    tokio::fs::write(&file, ics("one@example", "After", at))
        .await
        .expect("a rewritten fixture");
    let reread = live.until(|mock| holds(mock, "After")).await;

    live.stop().await;
    assert!(
        reread,
        "an edited local .ics reached the topic with no timer to carry it"
    );
}

#[tokio::test]
async fn a_directory_that_is_not_there_reports_the_read_rather_than_the_watch() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let missing = root.path().join("no-such-directory");

    let mock = settled(
        vec![source(
            "typo",
            CalendarSourceKind::Directory,
            &missing.to_string_lossy(),
        )],
        |mock| {
            mock.health()
                .iter()
                .any(|state| matches!(state, ServiceState::Degraded { .. }))
        },
    )
    .await;

    let degraded: Vec<String> = mock
        .health()
        .into_iter()
        .filter_map(|state| match state {
            ServiceState::Degraded { reason } => Some(reason),
            _ => None,
        })
        .collect();

    assert!(
        degraded
            .last()
            .is_some_and(|reason| reason.contains("cannot be read")),
        "the read's message is the one that tells you the path is wrong, got {degraded:?}"
    );
}

#[tokio::test]
async fn a_directory_with_nothing_in_it_is_reported_rather_than_read_as_empty() {
    let root = tempfile::tempdir().expect("a scratch directory");

    let read = read(
        None,
        CalendarSourceKind::Directory,
        root.path().to_string_lossy().into_owned(),
    )
    .await;

    assert!(read.is_err());
}
