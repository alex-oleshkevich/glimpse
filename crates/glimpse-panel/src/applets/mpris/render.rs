use chrono::{DateTime, Utc};
use gettextrs::gettext;
use glimpse_contracts::{Playback, PlayerStatus};

/// Where a player has got to now, advanced locally from the instant the daemon read it. MPRIS
/// emits no change signal for `Position`, so the alternative is the daemon re-reading it once a
/// second and republishing the whole list every time.
pub fn position(player: &PlayerStatus, now: DateTime<Utc>) -> i64 {
    let advanced = match player.playback {
        Playback::Playing => {
            let elapsed = (now - player.position_at).num_microseconds().unwrap_or(0);
            player.position_us + (elapsed as f64 * player.rate) as i64
        }
        _ => player.position_us,
    };

    match player.length_us.filter(|length| *length > 0) {
        Some(length) => advanced.clamp(0, length),
        None => advanced.max(0),
    }
}

pub fn seconds(microseconds: i64) -> f64 {
    microseconds.max(0) as f64 / 1_000_000.0
}

/// The popover's `Scrubber` spells the same elapsed time, so both go through one formatter rather
/// than disagreeing by a second over the track the viewer is looking at.
pub fn clock(microseconds: i64) -> String {
    glimpse_widgets::clock(seconds(microseconds))
}

fn state(playback: Playback) -> String {
    match playback {
        Playback::Playing => gettext("Playing"),
        Playback::Paused => gettext("Paused"),
        Playback::Stopped => gettext("Stopped"),
        Playback::Unknown => String::new(),
    }
}

/// Placeholders are replaced by name so a translated format may reorder them; `format!` into the
/// msgid would fix the order at the point the string was written.
pub fn label(format: &str, player: &PlayerStatus, now: DateTime<Utc>) -> String {
    let at = position(player, now);
    let remaining = player
        .length_us
        .filter(|length| *length > 0)
        .map(|length| clock(length - at))
        .unwrap_or_default();

    format
        .replace("{player}", &player.identity)
        .replace("{title}", player.title.as_deref().unwrap_or_default())
        .replace("{artist}", player.artist.as_deref().unwrap_or_default())
        .replace("{album}", player.album.as_deref().unwrap_or_default())
        .replace("{state}", &state(player.playback))
        .replace("{position}", &clock(at))
        .replace(
            "{duration}",
            &player
                .length_us
                .filter(|length| *length > 0)
                .map(clock)
                .unwrap_or_default(),
        )
        .replace("{remaining}", &remaining)
}

/// A format whose placeholders were all empty renders as separators with nothing between them —
/// ` — `, ` · ` — which reads as a broken label rather than as an absent one.
pub fn trimmed(rendered: &str, cap: usize) -> Option<String> {
    let text = rendered.trim();
    if !text.chars().any(char::is_alphanumeric) {
        return None;
    }

    let capped: String = text.chars().take(cap).collect();
    match capped.chars().count() < text.chars().count() {
        true => Some(format!("{capped}…")),
        false => Some(capped),
    }
}

#[cfg(test)]
pub(in crate::applets::mpris) mod tests {
    use chrono::TimeZone as _;
    use glimpse_contracts::PlayerCapabilities;

    use super::*;

    /// Seconds after the instant the fixture's position was read, so a test can say "half a
    /// minute later" without composing a wall-clock time that may not exist.
    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap() + chrono::TimeDelta::seconds(seconds)
    }

    /// One player with every field filled, so a test overrides only what it is about.
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

    /// A player that stops emitting anything would otherwise count past the end of its own track
    /// and show a negative remaining time.
    #[test]
    fn position_never_runs_past_the_length() {
        assert_eq!(position(&player(), at(3600)), 305_000_000);
    }

    /// A live stream has no end to count towards, so nothing may clamp against it.
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

    /// A video has no artist, and a format naming one must not leave its separator behind.
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

    /// The cap counts characters, not bytes: a byte slice through a multi-byte title panics, and
    /// track titles are chosen by whatever is playing.
    #[test]
    fn the_cap_counts_characters_and_marks_what_it_cut() {
        assert_eq!(
            trimmed("«Я ніколі не вярнуся»", 8).as_deref(),
            Some("«Я нікол…"),
            "eight characters, not eight bytes — a byte slice through this panics"
        );
        assert_eq!(trimmed("短い", 8).as_deref(), Some("短い"));
    }
}
