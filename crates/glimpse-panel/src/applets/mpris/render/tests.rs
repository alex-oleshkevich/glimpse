use chrono::TimeZone as _;
use glimpse_services::PlayerCapabilities;

use super::*;

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap() + chrono::TimeDelta::seconds(seconds)
}

pub(in crate::applets::mpris) fn player() -> PlayerStatus {
    PlayerStatus {
        id: "spotify".to_owned(),
        identity: "Spotify".to_owned(),
        desktop_entry: None,
        playback: Playback::Playing,
        current: false,
        title: Some("Dayvan Cowboy".to_owned()),
        artist: Some("Boards of Canada".to_owned()),
        album: Some("The Campfire Headphase".to_owned()),
        art: None,
        length_us: Some(305_000_000),
        position_us: 60_000_000,
        position_at: at(0),
        rate: 1.0,
        volume: None,
        repeat: None,
        shuffle: None,
        can: PlayerCapabilities {
            play: true,
            pause: true,
            previous: true,
            next: true,
            seek: true,
            control: true,
            raise: true,
        },
    }
}

#[test]
fn a_playing_position_advances_with_the_clock() {
    let player = player();

    assert_eq!(position(&player, at(0)), 60_000_000);
    assert_eq!(position(&player, at(30)), 90_000_000);
}

#[test]
fn a_paused_position_does_not_move_however_long_ago_it_was_read() {
    let mut player = player();
    player.playback = Playback::Paused;

    assert_eq!(position(&player, at(30)), 60_000_000);
}

#[test]
fn a_rate_other_than_one_advances_at_that_rate() {
    let mut player = player();
    player.rate = 2.0;

    assert_eq!(position(&player, at(30)), 120_000_000);
}

#[test]
fn position_never_runs_past_the_length() {
    assert_eq!(position(&player(), at(3600)), 305_000_000);
}

#[test]
fn a_stream_with_no_length_keeps_counting() {
    let mut player = player();
    player.length_us = None;

    assert_eq!(position(&player, at(3600)), 3_660_000_000);

    player.length_us = Some(0);
    assert_eq!(position(&player, at(3600)), 3_660_000_000);
}

#[test]
fn microseconds_become_the_seconds_a_scrubber_takes() {
    assert_eq!(seconds(1_500_000), 1.5);
    assert_eq!(
        seconds(-1),
        0.0,
        "a scrubber is given a position, and there is no position before the start"
    );
}

#[test]
fn a_clock_gains_an_hours_place_only_when_it_needs_one() {
    assert_eq!(clock(0), "0:00");
    assert_eq!(clock(61_000_000), "1:01");
    assert_eq!(clock(3_600_000_000), "1:00:00");
    assert_eq!(clock(-5), "0:00");
}

#[test]
fn every_placeholder_is_replaced_by_name() {
    let rendered = label(
        "{artist} — {title} ({state} {position}/{duration}, {remaining} left) on {player}",
        &player(),
        at(0),
    );

    assert_eq!(
        rendered,
        "Boards of Canada — Dayvan Cowboy (Playing 1:00/5:05, 4:05 left) on Spotify"
    );
}

#[test]
fn a_placeholder_with_nothing_behind_it_renders_as_nothing() {
    let mut player = player();
    player.artist = None;
    player.album = None;

    assert_eq!(
        label("{artist}{album}{title}", &player, at(0)),
        "Dayvan Cowboy"
    );
}

#[test]
fn a_label_of_nothing_but_separators_is_no_label_at_all() {
    assert_eq!(trimmed(" — ", 40), None);
    assert_eq!(trimmed("", 40), None);
    assert_eq!(trimmed("·", 40), None);
    assert_eq!(
        trimmed("Dayvan Cowboy", 40).as_deref(),
        Some("Dayvan Cowboy")
    );
}

#[test]
fn the_cap_counts_characters_and_marks_what_it_cut() {
    assert_eq!(
        trimmed("«Я ніколі не вярнуся»", 8).as_deref(),
        Some("«Я нікол…"),
        "eight characters, not eight bytes — a byte slice through this panics"
    );
    assert_eq!(trimmed("短い", 8).as_deref(), Some("短い"));
}

#[test]
fn metadata_that_looks_like_a_placeholder_is_rendered_as_written() {
    let mut player = player();
    player.title = Some("a {artist} b".to_owned());

    assert_eq!(label("{title}", &player, at(0)), "a {artist} b");
    assert_eq!(label("{foo} {title}", &player, at(0)), "{foo} a {artist} b");
}
