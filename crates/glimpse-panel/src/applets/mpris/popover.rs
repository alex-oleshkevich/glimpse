use std::collections::HashMap;

use super::ResolvedIcon;
use glimpse_services::{Playback, PlayerStatus};
use glimpse_widgets::Player;

const FALLBACK_ICON: &str = "multimedia-player-symbolic";

/// A player's own application icon where the theme has one, and a category icon where it does not.
/// An unresolvable name renders as a broken-image glyph, which reads worse than a generic icon, so
/// a candidate is taken only once the theme says it exists.
pub fn icon_name(player: &PlayerStatus) -> String {
    candidates(player)
        .into_iter()
        .find(|name| installed(name))
        .unwrap_or_else(|| FALLBACK_ICON.to_owned())
}

/// `DesktopEntry` first, because a player that names one is telling the truth about itself.
/// Otherwise the bus-name suffix, whole and then a segment at a time: `chromium.instance4181`
/// carries the application's name in front of a number nobody ships an icon for, and
/// `org.mpris.MediaPlayer2.spotify` carries it at the end.
fn candidates(player: &PlayerStatus) -> Vec<String> {
    let mut names: Vec<String> = player.desktop_entry.iter().cloned().collect();
    names.push(player.id.clone());
    names.extend(
        player
            .id
            .split('.')
            .filter(|segment| !segment.is_empty() && !segment.starts_with("instance"))
            .map(str::to_owned),
    );
    names.dedup();
    names
}

fn installed(name: &str) -> bool {
    gtk4::gdk::Display::default()
        .map(|display| gtk4::IconTheme::for_display(&display))
        .is_some_and(|theme| theme.has_icon(name))
}

pub fn rows(players: &[PlayerStatus], icons: &HashMap<String, ResolvedIcon>) -> Vec<Player> {
    players
        .iter()
        .filter(|player| !player.current)
        .map(|player| Player {
            key: player.id.clone(),
            name: player.identity.clone(),
            icon_name: icons
                .get(&player.id)
                .map(|held| held.name.clone())
                .unwrap_or_default(),
            title: player.title.clone().unwrap_or_default(),
            artist: player.artist.clone().unwrap_or_default(),
            playing: player.playback == Playback::Playing,
        })
        .collect()
}

#[cfg(test)]
mod tests;
