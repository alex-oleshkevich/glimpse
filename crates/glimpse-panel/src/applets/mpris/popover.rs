use std::collections::HashMap;

use glimpse_services::{Playback, PlayerStatus};
use glimpse_widgets::Player;
use gtk4::{gio, prelude::*};

use super::Themed;

const FALLBACK_ICON: &str = "multimedia-player-symbolic";

pub fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

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

/// Icons come from the applet's own cache rather than being resolved here, so every player on
/// screen is resolved on the same terms as the one on the bar.
pub fn rows(players: &[PlayerStatus], icons: &HashMap<String, Themed>) -> Vec<Player> {
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
mod tests {
    use super::*;

    fn player(id: &str, entry: Option<&str>) -> PlayerStatus {
        PlayerStatus {
            id: id.to_owned(),
            identity: id.to_owned(),
            desktop_entry: entry.map(str::to_owned),
            title: None,
            artist: None,
            album: None,
            length_us: None,
            ..super::super::render::tests::player()
        }
    }

    #[test]
    fn a_desktop_entry_is_tried_before_anything_guessed_from_the_bus_name() {
        assert_eq!(
            candidates(&player("chromium.instance4181", Some("chromium-browser"))),
            ["chromium-browser", "chromium.instance4181", "chromium"]
        );
    }

    #[test]
    fn an_instance_number_names_no_icon_and_is_not_offered() {
        assert_eq!(
            candidates(&player("chromium.instance4181", None)),
            ["chromium.instance4181", "chromium"]
        );
    }

    #[test]
    fn a_reverse_dns_bus_name_offers_its_last_segment_too() {
        assert_eq!(
            candidates(&player("org.gnome.Rhythmbox3", None)),
            ["org.gnome.Rhythmbox3", "org", "gnome", "Rhythmbox3"]
        );
    }

    #[test]
    fn a_plain_id_offers_itself_once() {
        assert_eq!(candidates(&player("spotify", None)), ["spotify"]);
    }

    #[test]
    fn the_current_player_is_never_repeated_in_the_others_list() {
        let mut current = player("spotify", None);
        current.current = true;

        let rows = rows(&[current, player("mpv", None)], &HashMap::new());

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "mpv");
        assert_eq!(
            rows[0].key, "mpv",
            "a row carries the id a command is addressed to, not its position"
        );
    }
}
