use glimpse_contracts::Playback;
use regex::Regex;

use super::Player;

const PLAYERCTLD: &str = "playerctld";
const KDECONNECT: &str = "kdeconnect.mpris_";

pub fn compile(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|pattern| match Regex::new(pattern) {
            Ok(compiled) => Some(compiled),
            Err(error) => {
                tracing::warn!(pattern, %error, "ignoring an mpris filter that does not compile");
                None
            }
        })
        .collect()
}

pub fn ignored(patterns: &[Regex], id: &str, identity: &str) -> bool {
    patterns
        .iter()
        .any(|pattern| pattern.is_match(id) || pattern.is_match(identity))
}

pub fn arrange(players: Vec<Player>) -> Vec<Player> {
    let mut kept: Vec<Player> = players
        .into_iter()
        .filter(|player| !ghost(player))
        .collect();
    kept = without_mirrors(kept);
    kept.sort_by(|a, b| {
        standing(a.playback)
            .cmp(&standing(b.playback))
            .then(b.last_active.cmp(&a.last_active))
            .then(a.id.cmp(&b.id))
    });
    if let Some(first) = kept.first_mut() {
        first.current = true;
    }
    kept
}

fn standing(playback: Playback) -> u8 {
    match playback {
        Playback::Playing => 0,
        Playback::Paused => 1,
        Playback::Stopped | Playback::Unknown => 2,
    }
}

/// KDE Connect exports a player for a phone that is playing nothing, and reports its own
/// `Identity` back as the track. It looks like a paused player to anything that only reads the
/// fields, and leaves a chip on the bar that does nothing.
fn ghost(player: &Player) -> bool {
    if player.playback != Playback::Paused {
        return false;
    }
    if player.length_us.is_some_and(|length| length > 0) {
        return false;
    }
    if player.can.previous || player.can.next || player.can.seek {
        return false;
    }
    [&player.title, &player.artist, &player.album]
        .into_iter()
        .all(|field| echoes(field.as_deref(), &player.identity))
}

fn echoes(field: Option<&str>, identity: &str) -> bool {
    field.is_none_or(|text| text.trim().is_empty() || text.eq_ignore_ascii_case(identity))
}

/// `playerctld` and KDE Connect re-export another player's media under their own name, so the same
/// track arrives twice. A mirror is dropped only when something else is carrying the same media —
/// a phone playing something of its own is a second player, not a duplicate.
fn without_mirrors(players: Vec<Player>) -> Vec<Player> {
    let mirrored: Vec<bool> = players
        .iter()
        .map(|player| {
            mirror(&player.id).is_some_and(|rank| {
                players.iter().any(|other| {
                    other.id != player.id
                        && mirror(&other.id).is_none_or(|theirs| theirs < rank)
                        && same_media(player, other)
                })
            })
        })
        .collect();

    players
        .into_iter()
        .zip(mirrored)
        .filter_map(|(player, dropped)| (!dropped).then_some(player))
        .collect()
}

fn mirror(id: &str) -> Option<u8> {
    if id == PLAYERCTLD {
        return Some(0);
    }
    id.starts_with(KDECONNECT).then_some(1)
}

/// Two players carry the same media when every field a listener would compare agrees. A pair that
/// carries no title at all is not comparable, and is left alone rather than guessed at.
fn same_media(a: &Player, b: &Player) -> bool {
    if a.title.is_none() && b.title.is_none() {
        return false;
    }
    a.title == b.title && a.artist == b.artist && a.album == b.album && a.length_us == b.length_us
}

#[cfg(test)]
pub(in crate::services::mpris) mod tests {
    use chrono::{TimeZone as _, Utc};
    use glimpse_contracts::PlayerCapabilities;

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

    /// Without the final tiebreak the order of two otherwise equal players comes out of whatever
    /// the bus listed, so the bar would pick a different one between two reads.
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

    /// A phone that is genuinely paused on a track of its own has capabilities and a length, so it
    /// must survive the same test the phantom fails.
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

    /// Both are mirrors, so neither may delete the other and leave nothing behind.
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
}
