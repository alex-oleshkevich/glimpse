use super::*;

#[test]
fn a_share_reads_as_its_folder_on_its_host_and_never_as_the_raw_mount() {
    assert_eq!(
        share("smb-share:server=nas,share=media%20library"),
        ("media library".to_owned(), "nas · SMB".to_owned())
    );
    assert_eq!(
        share("sftp:host=box.lan,user=alex"),
        ("box.lan".to_owned(), "SFTP".to_owned())
    );
    assert_eq!(
        share("dav:host=cloud,ssl=true,prefix=%2Fremote.php%2Fdav"),
        ("remote.php/dav".to_owned(), "cloud · DAV".to_owned())
    );
    assert_eq!(
        share("something-odd"),
        ("something-odd".to_owned(), String::new()),
        "a name that is not a gvfs mount is shown as it is"
    );
}

fn place(id: &str, name: &str, kind: glimpse_services::PlacesKind, path: &str) -> Place {
    Place {
        id: id.to_owned(),
        name: name.to_owned(),
        path: PathBuf::from(path),
        kind,
    }
}

#[test]
fn nothing_read_yet_shows_no_chip_and_a_known_empty_state_does() {
    assert!(!chip(&PlacesState::default()));

    let known = PlacesState {
        trash: Some(0),
        ..Default::default()
    };
    assert!(chip(&known));
}

#[test]
fn a_hostile_bookmark_label_is_capped_before_it_reaches_a_dto() {
    let long: String = std::iter::repeat_n('\u{e9}', 400).collect();
    let mut state = PlacesState::default();
    state.bookmarks.push(place(
        "bookmark",
        &long,
        glimpse_services::PlacesKind::Bookmark,
        "/home/alex/projects",
    ));

    let listing = bookmarks(&state, 8, false);

    assert_eq!(listing.entries.len(), 1);
    assert!(
        listing.entries[0].title.chars().count() <= NAME_CAP + 1,
        "a multibyte label must be capped by character count"
    );
}

#[test]
fn bookmarks_past_the_cap_go_behind_an_overflow_row() {
    let mut state = PlacesState::default();
    for index in 0..10 {
        state.bookmarks.push(place(
            &format!("b{index}"),
            &format!("bookmark{index}"),
            glimpse_services::PlacesKind::Bookmark,
            "/home/alex",
        ));
    }

    let capped = bookmarks(&state, 8, false);
    assert_eq!(capped.entries.len(), 8);
    assert!(capped.more.is_some());

    let opened = bookmarks(&state, 8, true);
    assert_eq!(opened.entries.len(), 10);
    assert_eq!(opened.more.as_deref(), Some("Show fewer"));
}

#[test]
fn activating_a_place_opens_its_path() {
    let mut places = PlacesState::default();
    places.places.push(place(
        "home",
        "Home",
        glimpse_services::PlacesKind::Home,
        "/home/alex",
    ));

    match activation(&places, "home") {
        Some(Activation::Open(path)) => assert_eq!(path, PathBuf::from("/home/alex")),
        _ => panic!("expected an open request"),
    }
}

#[test]
fn activating_trash_opens_the_trash_uri() {
    match activation(&PlacesState::default(), TRASH_ID) {
        Some(Activation::OpenUri(uri)) => assert_eq!(uri, "trash:///"),
        _ => panic!("expected an open request"),
    }
}
