use super::*;

pub(super) fn check() {
    let transport = Transport::new();
    let buttons = children_of::<gtk4::Button>(&transport);
    let [shuffle, previous, play, next, repeat] =
        <[gtk4::Button; 5]>::try_from(buttons).expect("five transport buttons");
    assert!(
        play.has_css_class("transport__play"),
        "play is the middle of five, and the order shuffle-previous-play-next-repeat is the \
             layout rather than an accident of how they were declared"
    );

    assert_eq!(
        play.icon_name().as_deref(),
        Some("media-playback-start-symbolic")
    );
    transport.set_playing(true);
    assert_eq!(
        play.icon_name().as_deref(),
        Some("media-playback-pause-symbolic")
    );

    transport.set_can_next(false);
    assert!(
        next.get_visible() && !next.is_sensitive(),
        "a capability a player lacks dims its button; removing it would move the other four \
             under the pointer between one track and the next"
    );

    assert!(
        !shuffle.get_visible() && !repeat.get_visible(),
        "shuffle and repeat are the two that hide instead, because a player without them has \
             no state for them to show and a permanently dead icon is worse than none"
    );
    transport.set_can_shuffle(true);
    transport.set_can_repeat(true);
    assert!(shuffle.get_visible() && repeat.get_visible());

    transport.set_repeat(Repeat::Track);
    assert_eq!(
        repeat.icon_name().as_deref(),
        Some("media-playlist-repeat-song-symbolic"),
        "repeat-one is a different icon, not a different shade of the same one"
    );
    assert!(repeat.has_css_class("transport--on"));
    transport.set_repeat(Repeat::Playlist);
    assert_eq!(
        repeat.icon_name().as_deref(),
        Some("media-playlist-repeat-symbolic")
    );
    assert!(repeat.has_css_class("transport--on"));
    transport.set_repeat(Repeat::Off);
    assert!(!repeat.has_css_class("transport--on"));

    let actions = Rc::new(RefCell::new(Vec::new()));
    transport.connect_action({
        let actions = Rc::clone(&actions);
        move |_, action| actions.borrow_mut().push(action)
    });
    previous.emit_clicked();
    play.emit_clicked();
    next.emit_clicked();
    shuffle.emit_clicked();
    repeat.emit_clicked();
    assert_eq!(
        *actions.borrow(),
        [
            TransportAction::Previous,
            TransportAction::PlayPause,
            TransportAction::Next,
            TransportAction::Shuffle,
            TransportAction::Repeat,
        ],
        "every button reports which one it was, so one handler covers the whole row"
    );
}
