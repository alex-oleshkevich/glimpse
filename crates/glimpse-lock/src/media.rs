use glimpse_services::{MprisPlayers, Playback, PlayerStatus};
use glimpse_widgets::Track;
use gtk4::gdk;

fn current(players: &MprisPlayers) -> Option<&PlayerStatus> {
    players.players.iter().find(|player| player.current)
}

pub fn art_path(players: &MprisPlayers) -> Option<&str> {
    current(players)?.art.as_deref()
}

pub fn track_of(
    players: &MprisPlayers,
    enabled: bool,
    art: Option<&(String, gdk::Texture)>,
) -> Option<Track> {
    if !enabled {
        return None;
    }
    let current = current(players)?;
    Some(Track {
        title: current.title.clone().unwrap_or_default(),
        artist: current.artist.clone().unwrap_or_default(),
        playing: current.playback == Playback::Playing,
        can_play_pause: current.can.play || current.can.pause,
        can_next: current.can.next,
        art: art
            .filter(|(path, _)| current.art.as_deref() == Some(path.as_str()))
            .map(|(_, texture)| texture.clone()),
    })
}

#[cfg(test)]
mod tests;
