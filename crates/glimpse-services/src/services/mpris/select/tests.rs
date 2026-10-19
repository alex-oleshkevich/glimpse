use crate::services::PlayerCapabilities;
use chrono::{TimeZone as _, Utc};

use super::*;

fn able() -> PlayerCapabilities {
    PlayerCapabilities {
        play: true,
        pause: true,
        previous: true,
        next: true,
        seek: true,
        control: true,
        raise: true,
    }
}

fn player(id: &str, playback: Playback, minute: u32) -> Player {
    Player {
        bus: format!("org.mpris.MediaPlayer2.{id}"),
        id: id.to_owned(),
        identity: id.to_owned(),
        desktop_entry: None,
        playback,
        current: false,
        title: Some("Dayvan Cowboy".to_owned()),
        artist: Some("Boards of Canada".to_owned()),
        album: Some("The Campfire Headphase".to_owned()),
        art_url: None,
        track_id: None,
        length_us: Some(305_000_000),
        position_us: 0,
        position_at: Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap(),
        rate: 1.0,
        volume: None,
        repeat: None,
        shuffle: None,
        can: able(),
        last_active: Utc.with_ymd_and_hms(2026, 9, 9, 12, minute, 0).unwrap(),
    }
}

pub(in crate::services::mpris) fn sample() -> Player {
    player("spotify", Playback::Playing, 5)
}

fn ids(players: &[Player]) -> Vec<&str> {
    players.iter().map(|player| player.id.as_str()).collect()
}

#[test]
fn a_playing_player_outranks_a_paused_one_however_recent() {
    let arranged = arrange(vec![
        player("firefox", Playback::Paused, 59),
        player("spotify", Playback::Playing, 1),
    ]);

    assert_eq!(ids(&arranged), ["spotify", "firefox"]);
    assert!(arranged[0].current);
    assert!(!arranged[1].current, "only one player is the current one");
}

#[test]
fn two_players_in_one_state_are_ordered_by_the_more_recently_active() {
    let arranged = arrange(vec![
        player("mpv", Playback::Playing, 10),
        player("spotify", Playback::Playing, 40),
    ]);

    assert_eq!(ids(&arranged), ["spotify", "mpv"]);
}

#[test]
fn players_equal_in_every_other_way_keep_a_stable_order() {
    let mut first = player("bbb", Playback::Playing, 5);
    let second = player("aaa", Playback::Playing, 5);
    first.last_active = second.last_active;

    assert_eq!(
        ids(&arrange(vec![first.clone(), second.clone()])),
        ["aaa", "bbb"]
    );
    assert_eq!(ids(&arrange(vec![second, first])), ["aaa", "bbb"]);
}

#[test]
fn a_paused_player_echoing_its_own_name_with_nothing_loaded_is_dropped() {
    let mut phone = player("kdeconnect.mpris_desktop_1", Playback::Paused, 5);
    phone.identity = "Pixel 9".to_owned();
    phone.title = Some("Pixel 9".to_owned());
    phone.artist = Some("Pixel 9".to_owned());
    phone.album = None;
    phone.length_us = None;
    phone.can = PlayerCapabilities::default();

    assert!(arrange(vec![phone]).is_empty());
}

#[test]
fn a_paused_player_with_a_real_track_is_kept() {
    let mut paused = player("spotify", Playback::Paused, 5);
    paused.can = PlayerCapabilities::default();
    paused.length_us = Some(305_000_000);

    assert_eq!(ids(&arrange(vec![paused])), ["spotify"]);
}

#[test]
fn a_phone_actually_holding_a_track_is_not_a_ghost() {
    let mut phone = player("kdeconnect.mpris_desktop_1", Playback::Paused, 5);
    phone.identity = "Pixel 9".to_owned();

    assert_eq!(ids(&arrange(vec![phone])), ["kdeconnect.mpris_desktop_1"]);
}

#[test]
fn playerctld_mirroring_a_real_player_is_dropped() {
    let arranged = arrange(vec![
        player("playerctld", Playback::Playing, 5),
        player("spotify", Playback::Playing, 5),
    ]);

    assert_eq!(ids(&arranged), ["spotify"]);
}

#[test]
fn a_mirror_carrying_different_media_is_its_own_player() {
    let mut phone = player("kdeconnect.mpris_desktop_1", Playback::Playing, 5);
    phone.title = Some("Something Else".to_owned());

    let arranged = arrange(vec![phone, player("spotify", Playback::Playing, 5)]);

    assert_eq!(ids(&arranged), ["kdeconnect.mpris_desktop_1", "spotify"]);
}

#[test]
fn two_mirrors_of_each_other_do_not_both_disappear() {
    let arranged = arrange(vec![
        player("playerctld", Playback::Playing, 5),
        player("kdeconnect.mpris_desktop_1", Playback::Playing, 5),
    ]);

    assert_eq!(
        ids(&arranged),
        ["playerctld"],
        "playerctld outranks a kdeconnect mirror, and the loser is the one dropped"
    );
}

#[test]
fn a_pattern_matches_the_id_or_the_identity() {
    let patterns = compile(&["^chromium".to_owned(), "(?i)kde".to_owned()]);

    assert!(ignored(&patterns, "chromium.instance123", "Chromium"));
    assert!(ignored(&patterns, "somename", "KDE Connect"));
    assert!(!ignored(&patterns, "spotify", "Spotify"));
    assert!(
        !ignored(&patterns, "not-chromium", "Firefox"),
        "an unanchored `^chromium` must not match in the middle"
    );
}

#[test]
fn a_pattern_that_does_not_compile_is_skipped_and_the_rest_still_work() {
    let patterns = compile(&["[".to_owned(), "spotify".to_owned()]);

    assert_eq!(patterns.len(), 1);
    assert!(ignored(&patterns, "spotify", "Spotify"));
}

#[test]
fn an_empty_list_ignores_nothing() {
    assert!(!ignored(&compile(&[]), "spotify", "Spotify"));
}
