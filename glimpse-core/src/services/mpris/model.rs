use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Artwork {
    #[default]
    None,
    FilePath(String),
    FileUri(String),
    RemoteUrl(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    pub player_id: String,
    pub bus_name: String,
    pub identity: String,
    pub playback_status: PlaybackStatus,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub subtitle: String,
    pub artwork: Artwork,
    pub position: Option<u64>,
    pub length: Option<u64>,
    pub progress_visible: bool,
    pub can_play_pause: bool,
    pub can_go_previous: bool,
    pub can_go_next: bool,
    pub can_seek: bool,
    pub can_raise: bool,
    pub last_active: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub current_player: Option<Player>,
    pub players: Vec<Player>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

impl Default for Health {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub health: Health,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    PlayPause {
        player_id: String,
    },
    Previous {
        player_id: String,
    },
    Next {
        player_id: String,
    },
    Seek {
        player_id: String,
        offset_micros: i64,
    },
    Raise {
        player_id: String,
    },
    SetFilterRegex(Vec<String>),
}

#[derive(Debug, Default)]
pub struct PlayerFilters {
    rules: Vec<Regex>,
}

impl PlayerFilters {
    #[cfg(test)]
    pub fn compile<I, S>(rules: I) -> std::result::Result<Self, regex::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        rules
            .into_iter()
            .map(|rule| Regex::new(rule.as_ref()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(|rules| Self { rules })
    }

    pub fn compile_lossy<I, S>(rules: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rules = rules
            .into_iter()
            .filter_map(|rule| match Regex::new(rule.as_ref()) {
                Ok(regex) => Some(regex),
                Err(error) => {
                    tracing::warn!(
                        rule = rule.as_ref(),
                        %error,
                        "invalid mpris filter regex; skipping rule"
                    );
                    None
                }
            })
            .collect();
        Self { rules }
    }

    pub fn matches(&self, player: &Player) -> bool {
        self.rules.iter().any(|rule| {
            rule.is_match(&player.identity)
                || rule.is_match(&player.title)
                || rule.is_match(&player.artist)
                || rule.is_match(&player.album)
                || rule.is_match(&player.player_id)
        })
    }
}

pub fn visible_players(players: &[Player]) -> Vec<Player> {
    players
        .iter()
        .filter(|player| is_visible(player))
        .cloned()
        .collect()
}

fn is_visible(player: &Player) -> bool {
    match player.playback_status {
        PlaybackStatus::Playing => true,
        PlaybackStatus::Paused => {
            has_meaningful_metadata(player)
                || player.can_play_pause
                || player.can_go_next
                || player.can_go_previous
        }
        PlaybackStatus::Stopped => false,
    }
}

// Returns false when title/artist are just echoing the player identity — KDE Connect
// does this when no track is loaded, leaving placeholder strings instead of empty fields.
fn has_meaningful_metadata(player: &Player) -> bool {
    (!player.title.is_empty() && player.title != player.identity)
        || (!player.artist.is_empty() && player.artist != player.identity)
        || !player.album.is_empty()
        || player.length.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(player_id: &str, playback_status: PlaybackStatus) -> Player {
        Player {
            player_id: player_id.into(),
            playback_status,
            ..Default::default()
        }
    }

    #[test]
    fn state_defaults_to_starting_with_empty_snapshot() {
        let state = State::default();

        assert_eq!(state.health, Health::Starting);
        assert_eq!(state.snapshot, Snapshot::default());
    }

    #[test]
    fn visible_players_hide_stopped_players() {
        let players = vec![
            player("spotify", PlaybackStatus::Playing),
            player("firefox", PlaybackStatus::Paused), // no metadata, no controls → hidden
            player("mpv", PlaybackStatus::Stopped),
        ];

        let ids = visible_players(&players)
            .into_iter()
            .map(|player| player.player_id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["spotify"]);
    }

    #[test]
    fn visible_players_show_paused_player_with_title() {
        let p = Player {
            player_id: "spotify".into(),
            playback_status: PlaybackStatus::Paused,
            title: "Some Song".into(),
            ..Default::default()
        };
        assert_eq!(visible_players(&[p.clone()]), vec![p]);
    }

    #[test]
    fn visible_players_show_paused_player_with_controls() {
        let p = Player {
            player_id: "spotify".into(),
            playback_status: PlaybackStatus::Paused,
            can_play_pause: true,
            ..Default::default()
        };
        assert_eq!(visible_players(&[p.clone()]), vec![p]);
    }

    #[test]
    fn visible_players_hide_paused_ghost_player_with_no_metadata_and_no_controls() {
        let ghost = Player {
            player_id: "kdeconnect.mpris_hash".into(),
            identity: "Spotify - Pixel 10 Pro".into(),
            playback_status: PlaybackStatus::Paused,
            ..Default::default()
        };
        assert!(visible_players(&[ghost]).is_empty());
    }

    #[test]
    fn visible_players_hide_paused_kdeconnect_placeholder_where_title_echoes_identity() {
        // KDE Connect sets title and artist to the identity string when no track is loaded.
        let ghost = Player {
            player_id: "kdeconnect.mpris_hash".into(),
            identity: "Spotify - Pixel 10 Pro".into(),
            title: "Spotify - Pixel 10 Pro".into(),
            artist: "Spotify - Pixel 10 Pro".into(),
            playback_status: PlaybackStatus::Paused,
            ..Default::default()
        };
        assert!(visible_players(&[ghost]).is_empty());
    }

    #[test]
    fn player_filters_compile_rejects_invalid_regex() {
        assert!(PlayerFilters::compile(["("]).is_err());
    }

    #[test]
    fn player_filters_compile_lossy_skips_invalid_rules() {
        let filters = PlayerFilters::compile_lossy(["(", "(?i)^spotify$"]);

        let player = Player {
            player_id: "Spotify".into(),
            ..Default::default()
        };

        assert!(filters.matches(&player));
    }

    #[test]
    fn player_filters_match_each_field() {
        for player in [
            Player {
                identity: "blocked".into(),
                ..Default::default()
            },
            Player {
                title: "a blocked track".into(),
                ..Default::default()
            },
            Player {
                artist: "blocked artist".into(),
                ..Default::default()
            },
            Player {
                album: "blocked album".into(),
                ..Default::default()
            },
            Player {
                player_id: "blocked".into(),
                ..Default::default()
            },
        ] {
            let filters = PlayerFilters::compile(["blocked"]).expect("valid regex");
            assert!(filters.matches(&player));
        }
    }

    #[test]
    fn player_filters_default_matches_nothing() {
        let player = Player {
            player_id: "spotify".into(),
            identity: "Spotify".into(),
            title: "Some Song".into(),
            ..Default::default()
        };

        assert!(!PlayerFilters::default().matches(&player));
    }

    #[test]
    fn player_filters_do_not_match_unrelated_player() {
        let filters = PlayerFilters::compile(["(?i)^firefox$"]).expect("valid regex");
        let player = Player {
            player_id: "spotify".into(),
            identity: "Spotify".into(),
            ..Default::default()
        };

        assert!(!filters.matches(&player));
    }
}
