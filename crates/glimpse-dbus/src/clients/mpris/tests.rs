use super::*;

#[test]
fn mpris_constants_match_spec() {
    assert_eq!(MPRIS_PATH, "/org/mpris/MediaPlayer2");
    assert_eq!(MPRIS_ROOT_INTERFACE, "org.mpris.MediaPlayer2");
    assert_eq!(MPRIS_PLAYER_INTERFACE, "org.mpris.MediaPlayer2.Player");
    assert_eq!(MPRIS_NAME_PREFIX, "org.mpris.MediaPlayer2.");
    assert_eq!(
        MPRIS_ROOT_INTERFACE,
        MPRIS_NAME_PREFIX.trim_end_matches('.')
    );
}
