use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;
use crate::services::compositor::{
    CompositorStatus, CompositorWindows, CompositorWorkspaces, WindowInfo, WorkspaceInfo,
};
use crate::services::exec::wire::FromApplet;
use crate::services::mpris::{PlayerCapabilities, PlayerStatus};

#[test]
fn players_reach_the_applet_capped_and_without_art() {
    let players = MprisPlayers {
        players: vec![PlayerStatus {
            id: "spotify".to_owned(),
            identity: "Spotify".to_owned(),
            desktop_entry: Some("spotify".to_owned()),
            playback: Playback::Playing,
            current: true,
            title: Some("x".repeat(1000)),
            artist: Some("Artist".to_owned()),
            album: None,
            art: Some("/home/user/.cache/art.png".to_owned()),
            length_us: Some(180_000_000),
            position_us: 5_000_000,
            position_at: Utc.timestamp_millis_opt(1_000).single().expect("time"),
            rate: 1.0,
            volume: Some(0.5),
            repeat: None,
            shuffle: None,
            can: PlayerCapabilities {
                play: true,
                next: true,
                ..Default::default()
            },
        }],
    };
    let value = mpris_value(&players);
    let player = &value["players"][0];
    assert_eq!(player["playback"], "playing");
    assert_eq!(player["positionAt"], 1_000);
    assert_eq!(player["desktopEntry"], "spotify");
    assert!(player["title"].as_str().expect("title").chars().count() <= TEXT_CAP + 1);
    assert!(player.get("art").is_none());
    assert!(player.get("volume").is_none());
    assert_eq!(
        player["can"],
        json!({"play": true, "pause": false, "previous": false, "next": true, "seek": false})
    );
}

#[test]
fn compositor_state_flattens_to_lists() {
    let state = CompositorState {
        status: Some(CompositorStatus {
            name: "niri".to_owned(),
            capabilities: crate::services::compositor::CompositorCapabilities {
                floating: false,
                workspace_reorder: false,
                output_power: false,
                blank_screens: false,
            },
        }),
        workspaces: Some(CompositorWorkspaces {
            workspaces: vec![WorkspaceInfo {
                id: 3,
                index: Some(1),
                name: None,
                output: Some("eDP-1".to_owned()),
                active: true,
                focused: true,
                urgent: false,
                windows: 2,
            }],
        }),
        windows: Some(CompositorWindows {
            windows: vec![WindowInfo {
                id: 9,
                title: Some("t".repeat(1000)),
                app_id: Some("firefox".to_owned()),
                workspace: Some(3),
                focused: true,
                floating: false,
                urgent: false,
                order: None,
            }],
        }),
        outputs: None,
        privacy: None,
    };
    let value = compositor_value(&state);
    assert_eq!(value["name"], "niri");
    assert_eq!(value["workspaces"][0]["id"], 3);
    assert_eq!(value["windows"][0]["appId"], "firefox");
    assert!(
        value["windows"][0]["title"]
            .as_str()
            .expect("title")
            .chars()
            .count()
            <= TEXT_CAP + 1
    );
    assert_eq!(value["outputs"], json!([]));
    assert_eq!(
        compositor_value(&CompositorState::default()),
        json!({"name": null, "workspaces": [], "windows": [], "outputs": []})
    );
}

#[test]
fn service_requests_parse_from_the_wire() {
    let parse = |line: &str| serde_json::from_str::<FromApplet>(line).expect("parses");
    assert_eq!(
        parse(r#"{"t":"mpris","player":"spotify","op":"control","action":"play-pause"}"#),
        FromApplet::Mpris {
            player: "spotify".to_owned(),
            call: MprisCall::Control {
                action: MprisAction::PlayPause
            },
        }
    );
    assert_eq!(
        parse(r#"{"t":"mpris","player":"spotify","op":"seek","offset":-5000000}"#),
        FromApplet::Mpris {
            player: "spotify".to_owned(),
            call: MprisCall::Seek { offset: -5_000_000 },
        }
    );
    assert_eq!(
        parse(r#"{"t":"compositor","op":"focus-workspace","target":{"index":2}}"#),
        FromApplet::Compositor {
            call: CompositorCall::FocusWorkspace {
                target: WorkspaceTarget::Index { index: 2 }
            },
        }
    );
    assert_eq!(
        parse(r#"{"t":"compositor","op":"move-window","window":9,"target":"next"}"#),
        FromApplet::Compositor {
            call: CompositorCall::MoveWindow {
                window: 9,
                target: WorkspaceTarget::Step(Step::Next),
            },
        }
    );
    assert_eq!(
        parse(r#"{"t":"compositor","op":"rename-workspace","id":3,"name":null}"#),
        FromApplet::Compositor {
            call: CompositorCall::RenameWorkspace { id: 3, name: None },
        }
    );
    assert_eq!(
        parse(r#"{"t":"launch","id":"org.gnome.Nautilus"}"#),
        FromApplet::Launch {
            id: "org.gnome.Nautilus".to_owned()
        }
    );
    assert!(
        serde_json::from_str::<FromApplet>(r#"{"t":"compositor","op":"power-off-monitors"}"#)
            .is_err()
    );
}
