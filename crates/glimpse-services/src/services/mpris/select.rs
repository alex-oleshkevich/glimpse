use crate::services::Playback;
use regex::Regex;

use super::Player;

const PLAYERCTLD: &str = "playerctld";
const KDECONNECT: &str = "kdeconnect.mpris_";

pub fn compile(patterns: &[String]) -> Vec<Regex> {
    crate::services::compile_patterns(patterns, |pattern, error| {
        tracing::warn!(pattern, %error, "ignoring an mpris filter that does not compile");
    })
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
pub(in crate::services::mpris) mod tests;
