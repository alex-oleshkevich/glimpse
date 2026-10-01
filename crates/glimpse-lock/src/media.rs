use glimpse_services::{MprisPlayers, Playback};
use glimpse_widgets::Track;

pub fn track_of(players: &MprisPlayers, enabled: bool) -> Option<Track> {
    if !enabled {
        return None;
    }
    let current = players.players.iter().find(|player| player.current)?;
    Some(Track {
        title: current.title.clone().unwrap_or_default(),
        artist: current.artist.clone().unwrap_or_default(),
        playing: current.playback == Playback::Playing,
        can_play_pause: current.can.play || current.can.pause,
        can_next: current.can.next,
    })
}

#[cfg(test)]
mod tests;
