use super::*;

#[test]
fn a_file_url_becomes_the_path_it_names() {
    assert_eq!(
        classify(Some("file:///music/cover.png")),
        Some(Art::Local("/music/cover.png".to_owned()))
    );
}

/// A track whose folder has a space in it is ordinary, and the URL that names it is
/// percent-encoded. Opening the undecoded form fails and the artwork silently disappears.
#[test]
fn a_file_url_is_percent_decoded() {
    assert_eq!(
        classify(Some("file:///music/My%20Album/cover.png")),
        Some(Art::Local("/music/My Album/cover.png".to_owned()))
    );
}

#[test]
fn a_bare_absolute_path_is_taken_as_one() {
    assert_eq!(
        classify(Some("/music/cover.png")),
        Some(Art::Local("/music/cover.png".to_owned()))
    );
}

#[test]
fn http_and_https_are_the_only_schemes_that_are_fetched() {
    assert_eq!(
        classify(Some("https://example.test/a.png")),
        Some(Art::Remote("https://example.test/a.png".to_owned()))
    );
    assert!(matches!(
        classify(Some("http://example.test/a.png")),
        Some(Art::Remote(_))
    ));
}

/// The URL is chosen by another application, so a scheme this daemon would not go and read has
/// to come out as nothing rather than be handed to a fetcher.
#[test]
fn any_other_scheme_is_refused() {
    for refused in [
        "javascript:alert(1)",
        "data:image/png;base64,AAAA",
        "ftp://example.test/a.png",
        "mailto:someone@example.test",
    ] {
        assert_eq!(classify(Some(refused)), None, "{refused} was accepted");
    }
}

/// A `file://` URL naming anything at all still resolves to that path, traversal included —
/// which is correct rather than a hole. A player runs as the same user and can name
/// `/etc/shadow` outright without a `..` in sight, so refusing traversal would buy nothing.
/// What bounds this is that the path is only ever decoded as an image, under a size cap.
#[test]
fn a_file_url_is_taken_at_its_word() {
    assert_eq!(
        classify(Some("file:///../../etc/shadow")),
        Some(Art::Local("/etc/shadow".to_owned()))
    );
}

#[test]
fn nothing_at_all_is_nothing() {
    assert_eq!(classify(None), None);
    assert_eq!(classify(Some("")), None);
    assert_eq!(classify(Some("   ")), None);
    assert_eq!(classify(Some("not a url")), None);
}

/// Asserted on the digest rather than on `cached`, which answers `None` where
/// `XDG_RUNTIME_DIR` is unset — a test written against it passes having checked nothing in
/// exactly the environment least like the one it is meant to cover.
#[test]
fn one_url_always_hashes_to_one_name() {
    assert_eq!(
        digest("https://example.test/a.png"),
        digest("https://example.test/a.png")
    );
    assert_ne!(
        digest("https://example.test/a.png"),
        digest("https://example.test/b.png")
    );
    assert_eq!(digest("https://example.test/a.png").len(), 16);
}
