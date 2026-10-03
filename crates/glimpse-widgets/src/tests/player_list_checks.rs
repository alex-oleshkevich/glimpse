use super::*;

pub(super) fn check() {
    let players = PlayerList::new();
    players.set_players(&[
        Player {
            key: "firefox".to_owned(),
            name: "Firefox".to_owned(),
            icon_name: "web-browser-symbolic".to_owned(),
            title: "How the Chip Shortage Ends".to_owned(),
            artist: "Odd Lots".to_owned(),
            playing: false,
        },
        Player {
            key: "vlc".to_owned(),
            name: "VLC".to_owned(),
            icon_name: "video-x-generic-symbolic".to_owned(),
            title: "The Wire".to_owned(),
            artist: String::new(),
            playing: true,
        },
    ]);
    let player_rows = children_of::<PlayerRow>(&players);
    assert_eq!(player_rows.len(), 2);

    let first: &LegacyRow = player_rows[0].upcast_ref();
    let second: &LegacyRow = player_rows[1].upcast_ref();
    assert_eq!(first.subtitle().as_deref(), Some("Odd Lots · Firefox"));
    assert_eq!(
        second.subtitle().as_deref(),
        Some("VLC"),
        "a video with no artist says which application is playing it rather than showing a \
             bare separator"
    );

    let toggle = child_named::<gtk4::Button>(&player_rows[1], "player-row__toggle");
    assert_eq!(
        toggle.icon_name().as_deref(),
        Some("media-playback-pause-symbolic")
    );

    let promoted = Rc::new(RefCell::new(Vec::new()));
    players.connect_activated({
        let promoted = Rc::clone(&promoted);
        move |_, key| promoted.borrow_mut().push(key)
    });
    let toggled = Rc::new(RefCell::new(Vec::new()));
    players.connect_toggled({
        let toggled = Rc::clone(&toggled);
        move |_, key| toggled.borrow_mut().push(key)
    });
    player_rows[1].emit_clicked();
    toggle.emit_clicked();
    assert_eq!(*promoted.borrow(), ["vlc".to_owned()]);
    assert_eq!(
        *toggled.borrow(),
        ["vlc".to_owned()],
        "the button in the trail is a second gesture on the same row, and each carries the \
             same key so one list handles both"
    );

    players.set_players(&[Player {
        key: "vlc".to_owned(),
        name: "VLC".to_owned(),
        ..Default::default()
    }]);
    children_of::<PlayerRow>(&players)[0].emit_clicked();
    assert_eq!(
        *promoted.borrow(),
        ["vlc".to_owned(), "vlc".to_owned()],
        "a row is reused in place, so the key it reports is read back at the moment it fires \
             rather than captured when the row was built"
    );
}
