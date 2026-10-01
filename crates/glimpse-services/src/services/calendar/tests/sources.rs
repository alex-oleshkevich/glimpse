use super::*;

/// `Duration::from_secs(0)` makes `tokio::time::interval` panic, and every value between one
/// and the floor is a request the provider would answer by blocking us.
#[test]
fn a_poll_interval_under_the_floor_is_raised_to_it() {
    assert_eq!(Config::from(&document(0, Vec::new())).poll_interval, 60);
    assert_eq!(Config::from(&document(59, Vec::new())).poll_interval, 60);
    assert_eq!(Config::from(&document(900, Vec::new())).poll_interval, 900);
}

/// Two sources under one id would share a subscription key, so the second would never run
/// while silently overwriting the first's events in the published map.
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

/// `webcal://` is what a provider's Subscribe button hands out, and it is https underneath.
/// Refusing it made the most common way to get a feed URL the one shape that did not work.
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

/// The whole point of the sidecar is that the secret URL stays out of `config.toml`, so a file
/// holding one has to be told apart from a file holding a calendar.
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
