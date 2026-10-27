use super::*;

#[test]
fn a_transport_repeat_carries_the_same_three_states_as_the_wire() {
    assert_eq!(transport_repeat(Repeat::Off), TransportRepeat::Off);
    assert_eq!(
        transport_repeat(Repeat::Playlist),
        TransportRepeat::Playlist
    );
    assert_eq!(transport_repeat(Repeat::Track), TransportRepeat::Track);
    assert_eq!(
        transport_repeat(Repeat::Unknown),
        TransportRepeat::Off,
        "a state this panel does not know renders as off rather than as a missing icon"
    );
}

#[test]
fn repeat_cycles_off_playlist_track_and_round_again() {
    assert_eq!(cycled(Repeat::Off), Repeat::Playlist);
    assert_eq!(cycled(Repeat::Playlist), Repeat::Track);
    assert_eq!(cycled(Repeat::Track), Repeat::Off);
    assert_eq!(cycled(Repeat::Unknown), Repeat::Off);
}

#[test]
fn art_still_loading_is_not_asked_for_again() {
    let mut art = Art::<u8>::default();
    assert!(art.want(Some("/a.png")).is_some());
    assert_eq!(
        art.want(Some("/a.png")),
        None,
        "a load that never returns must not start a second one every tick"
    );
}

#[test]
fn art_for_a_path_no_longer_wanted_is_dropped_when_it_lands() {
    let mut art = Art::<u8>::default();
    let (old, _) = art.want(Some("/a.png")).expect("a first request");
    let (current, _) = art.want(Some("/b.png")).expect("a changed request");
    assert!(!art.land(old, Some(1)));
    assert_eq!(art.texture(), None);
    assert!(art.land(current, Some(2)));
    assert_eq!(art.texture(), Some(&2));
    assert_eq!(art.want(Some("/b.png")), None);
}

#[test]
fn art_that_failed_to_load_is_tried_again() {
    let mut art = Art::<u8>::default();
    let (request, _) = art.want(Some("/a.png")).expect("a first request");
    assert!(
        !art.land(request, None),
        "failure must not trigger another wake"
    );
    assert!(art.want(Some("/a.png")).is_some());
}

#[test]
fn no_art_wanted_clears_the_held_texture() {
    let mut art = Art::<u8>::default();
    let (request, _) = art.want(Some("/a.png")).expect("a first request");
    art.land(request, Some(1));
    assert_eq!(art.want(None), None);
    assert_eq!(art.texture(), None);
}

#[test]
fn returning_to_a_path_accepts_only_its_latest_requested_image() {
    let mut art = Art::<u8>::default();
    let (old_a, _) = art.want(Some("/a.png")).expect("an old A request");
    let (b, _) = art.want(Some("/b.png")).expect("a B request");
    let (new_a, _) = art.want(Some("/a.png")).expect("a new A request");

    assert!(!art.land(old_a, Some(1)));
    assert!(!art.land(b, Some(2)));
    assert_eq!(art.texture(), None);
    assert!(art.land(new_a, Some(3)));
    assert_eq!(art.texture(), Some(&3));
}
