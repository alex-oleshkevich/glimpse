use super::*;
use glimpse_services::{PlayerCapabilities, PlayerStatus};

fn player(current: bool, playback: Playback, play: bool, pause: bool, next: bool) -> PlayerStatus {
    PlayerStatus {
        id: "player".to_owned(),
        identity: "Player".to_owned(),
        desktop_entry: None,
        playback,
        current,
        title: Some("Title".to_owned()),
        artist: Some("Artist".to_owned()),
        album: None,
        art: None,
        length_us: None,
        position_us: 0,
        position_at: chrono::Utc::now(),
        rate: 1.0,
        volume: None,
        repeat: None,
        shuffle: None,
        can: PlayerCapabilities {
            play,
            pause,
            previous: false,
            next,
            seek: false,
            control: true,
            raise: false,
        },
    }
}

#[test]
fn no_current_player_is_none() {
    let players = MprisPlayers {
        players: vec![player(false, Playback::Playing, true, true, true)],
    };
    assert!(track_of(&players, true, None).is_none());
}

#[test]
fn an_empty_player_list_is_none() {
    assert!(track_of(&MprisPlayers::default(), true, None).is_none());
}

#[test]
fn disabled_is_none_even_with_a_current_player() {
    let players = MprisPlayers {
        players: vec![player(true, Playback::Playing, true, true, true)],
    };
    assert!(track_of(&players, false, None).is_none());
}

#[test]
fn the_current_player_maps_title_artist_and_playing() {
    let players = MprisPlayers {
        players: vec![player(true, Playback::Playing, true, true, true)],
    };
    let track = track_of(&players, true, None).expect("a track");
    assert_eq!(track.title, "Title");
    assert_eq!(track.artist, "Artist");
    assert!(track.playing);
    assert!(track.can_play_pause);
    assert!(track.can_next);
}

#[test]
fn paused_is_not_playing() {
    let players = MprisPlayers {
        players: vec![player(true, Playback::Paused, true, false, false)],
    };
    let track = track_of(&players, true, None).expect("a track");
    assert!(!track.playing);
}

#[test]
fn can_play_pause_is_true_when_either_capability_is_present() {
    let only_pause = MprisPlayers {
        players: vec![player(true, Playback::Paused, false, true, false)],
    };
    assert!(
        track_of(&only_pause, true, None)
            .expect("a track")
            .can_play_pause
    );

    let only_play = MprisPlayers {
        players: vec![player(true, Playback::Paused, true, false, false)],
    };
    assert!(
        track_of(&only_play, true, None)
            .expect("a track")
            .can_play_pause
    );

    let neither = MprisPlayers {
        players: vec![player(true, Playback::Paused, false, false, false)],
    };
    assert!(
        !track_of(&neither, true, None)
            .expect("a track")
            .can_play_pause
    );
}

#[test]
fn can_next_follows_the_player_capability_directly() {
    let players = MprisPlayers {
        players: vec![player(true, Playback::Playing, true, true, false)],
    };
    assert!(!track_of(&players, true, None).expect("a track").can_next);
}

#[test]
fn missing_title_and_artist_become_empty_strings() {
    let mut status = player(true, Playback::Playing, true, true, true);
    status.title = None;
    status.artist = None;
    let players = MprisPlayers {
        players: vec![status],
    };
    let track = track_of(&players, true, None).expect("a track");
    assert_eq!(track.title, "");
    assert_eq!(track.artist, "");
}

#[test]
fn art_reaches_the_track_only_for_the_current_players_path() {
    let mut status = player(true, Playback::Playing, true, true, true);
    status.art = Some("/tmp/cover-a.png".to_owned());
    let players = MprisPlayers {
        players: vec![status],
    };
    let texture: gdk::Texture = gdk::MemoryTexture::new(
        1,
        1,
        gdk::MemoryFormat::R8g8b8a8,
        &gtk4::glib::Bytes::from_static(&[0; 4]),
        4,
    )
    .into();
    let held = ("/tmp/cover-a.png".to_owned(), texture.clone());
    assert_eq!(
        track_of(&players, true, Some(&held)).and_then(|track| track.art),
        Some(texture.clone()),
        "the decoded cover of the current track is shown"
    );
    let stale = ("/tmp/cover-b.png".to_owned(), texture);
    assert!(
        track_of(&players, true, Some(&stale))
            .and_then(|track| track.art)
            .is_none(),
        "a cover decoded for the previous track is not shown on this one"
    );
}
