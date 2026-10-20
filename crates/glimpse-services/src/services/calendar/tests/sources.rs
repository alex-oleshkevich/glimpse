use super::*;

#[test]
fn a_poll_interval_under_the_floor_is_raised_to_it() {
    assert_eq!(Config::from(&document(0, Vec::new())).poll_interval, 60);
    assert_eq!(Config::from(&document(59, Vec::new())).poll_interval, 60);
    assert_eq!(Config::from(&document(900, Vec::new())).poll_interval, 900);
}

#[test]
fn two_sources_sharing_an_id_are_reduced_to_the_first() {
    let sources = vec![
        source(
            "work",
            CalendarSourceKind::Ical,
            "https://one.example/a.ics",
        ),
        source(
            "work",
            CalendarSourceKind::Ical,
            "https://two.example/b.ics",
        ),
        source(
            "home",
            CalendarSourceKind::Ical,
            "https://one.example/c.ics",
        ),
    ];
    let kept = Config::from(&document(600, sources)).sources;

    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].uri, "https://one.example/a.ics");
    assert_eq!(kept[1].id, "home");
}

#[test]
fn a_uri_is_read_as_a_feed_a_path_or_nothing_at_all() {
    assert!(matches!(
        location("https://example.test/a.ics"),
        Ok(Location::Remote(_))
    ));
    assert!(matches!(
        location("file:///home/someone/a.ics"),
        Ok(Location::Local(path)) if path == Path::new("/home/someone/a.ics")
    ));
    assert!(matches!(
        location("/home/someone/a.ics"),
        Ok(Location::Local(path)) if path == Path::new("/home/someone/a.ics")
    ));
    assert!(
        location("gopher://example.test/a.ics").is_err(),
        "a scheme nothing here fetches must say so rather than become a path"
    );
}

#[test]
fn a_webcal_subscription_is_read_as_the_https_feed_it_is() {
    assert!(matches!(
        location("webcal://example.test/a.ics"),
        Ok(Location::Remote(url)) if url.as_str() == "https://example.test/a.ics"
    ));
    assert!(
        matches!(
            location("WEBCAL://example.test/a.ics"),
            Ok(Location::Remote(_))
        ),
        "a url scheme is case-insensitive"
    );
    assert_eq!(
        subscribed("https://example.test/a.ics"),
        "https://example.test/a.ics",
        "everything else is passed through untouched"
    );
    assert_eq!(
        subscribed("webc"),
        "webc",
        "a uri shorter than the scheme is not sliced"
    );
    assert_eq!(subscribed("wébcal://x"), "wébcal://x");
}

#[test]
fn only_a_one_line_url_is_read_as_a_sidecar() {
    assert_eq!(
        sidecar("https://example.test/private/a.ics\n")
            .map(|url| url.to_string())
            .as_deref(),
        Some("https://example.test/private/a.ics")
    );
    assert!(sidecar(DOCUMENT).is_none(), "a calendar is not a sidecar");
    assert!(sidecar("").is_none());
    assert!(
        sidecar("https://example.test/a.ics\nhttps://example.test/b.ics").is_none(),
        "two lines is neither a sidecar nor something to guess at"
    );
    assert!(sidecar("file:///etc/passwd").is_none());
    assert_eq!(
        sidecar("webcal://example.test/private/a.ics")
            .map(|url| url.to_string())
            .as_deref(),
        Some("https://example.test/private/a.ics"),
        "the sidecar exists to hold the link a provider gave you, and that link is webcal"
    );
}

#[tokio::test]
async fn a_directory_with_more_than_256_events_is_read_whole() {
    let root = tempfile::tempdir().expect("a scratch directory");
    for index in 0..300 {
        tokio::fs::write(
            root.path().join(format!("{index}.ics")),
            ics(
                &format!("{index}@example"),
                "Synced",
                now() + TimeDelta::hours(index),
            ),
        )
        .await
        .expect("an event file");
    }

    let documents = directory(root.path().to_str().expect("utf-8"))
        .await
        .expect("a readable directory");

    assert_eq!(documents.len(), 300);
}

#[tokio::test]
async fn a_directory_stops_at_the_byte_cap_without_failing() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let padding = "X".repeat(1024 * 1024);
    for index in 0..20 {
        tokio::fs::write(root.path().join(format!("{index}.ics")), &padding)
            .await
            .expect("a large file");
    }

    let documents = directory(root.path().to_str().expect("utf-8"))
        .await
        .expect("a capped read is not a failure");

    assert_eq!(documents.len(), 16);
}

#[tokio::test]
async fn an_unreadable_file_is_skipped_and_the_rest_are_kept() {
    let root = tempfile::tempdir().expect("a scratch directory");
    tokio::fs::write(root.path().join("bad.ics"), [0xff, 0xfe, 0xfd])
        .await
        .expect("a file that is not text");
    tokio::fs::write(
        root.path().join("good.ics"),
        ics("g@example", "Good", now()),
    )
    .await
    .expect("an event file");

    let documents = directory(root.path().to_str().expect("utf-8"))
        .await
        .expect("a readable directory");

    assert_eq!(documents.len(), 1);
}

#[tokio::test]
async fn a_missing_calendar_directory_is_followed_once_created() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let calendars = root.path().join("calendars");
    let mut source = watching(
        None,
        "work".to_owned(),
        calendars.to_string_lossy().into_owned(),
        CalendarSourceKind::Directory,
    );

    assert!(matches!(
        source.next().await,
        Some(Event::Fetched { result: Err(_), .. })
    ));

    tokio::fs::create_dir_all(&calendars)
        .await
        .expect("calendar directory");
    tokio::fs::write(calendars.join("a.ics"), ics("a@example", "Late", now()))
        .await
        .expect("an event file");

    let next = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(event) = source.next().await {
            if matches!(event, Event::Fetched { result: Ok(_), .. }) {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(next, Ok(true));
}
