use chrono::{DateTime, Utc};
use gettextrs::gettext;
use glimpse_services::{Playback, PlayerStatus};

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
    let duration = player
        .length_us
        .filter(|length| *length > 0)
        .map(clock)
        .unwrap_or_default();
    let state = state(player.playback);
    let position = clock(at);

    crate::applets::tokens::render(format, |token| match token {
        "player" => Some(player.identity.as_str()),
        "title" => Some(player.title.as_deref().unwrap_or_default()),
        "artist" => Some(player.artist.as_deref().unwrap_or_default()),
        "album" => Some(player.album.as_deref().unwrap_or_default()),
        "state" => Some(state.as_str()),
        "position" => Some(position.as_str()),
        "duration" => Some(duration.as_str()),
        "remaining" => Some(remaining.as_str()),
        _ => None,
    })
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
pub(in crate::applets::mpris) mod tests;
