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
