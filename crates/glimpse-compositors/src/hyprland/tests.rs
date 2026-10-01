use futures_util::StreamExt;
use serde_json::json;

use super::testing::FakeHyprland;
use super::*;
use crate::event::Resync;
use crate::model::{Cast, CastKind, CastTarget, WindowId};

fn snapshot_server() -> FakeHyprland {
    FakeHyprland::spawn(
        |command| match command {
            "j/monitors" => json!([
                { "id": 0, "name": "eDP-1", "description": "Samsung 14\"", "make": "Samsung",
                  "model": "ATNA60CL10", "width": 2880, "height": 1800,
                  "refreshRate": 120.001, "x": 0, "y": 0, "scale": 1.25, "transform": 0,
                  "focused": true, "disabled": false, "activeWorkspace": { "id": 5 } },
                { "id": 1, "name": "DP-3", "description": "", "disabled": true }
            ])
            .to_string(),
            "j/workspaces" => json!([
                { "id": 5, "name": "5", "monitor": "eDP-1", "lastwindow": "0x9" },
                { "id": 6, "name": "chat", "monitor": "DP-3", "lastwindow": "0x0" }
            ])
            .to_string(),
            "j/clients" => json!([
                { "address": "0x9", "title": "a terminal", "class": "foot", "pid": 4265,
                  "workspace": { "id": 5 }, "floating": false, "urgent": false,
                  "monitor": 0, "at": [0, 0], "size": [2880, 1800] }
            ])
            .to_string(),
            "j/activewindow" => json!({ "address": "0x9" }).to_string(),
            "j/getoption input:kb_layout" => json!({ "str": "pl,ru" }).to_string(),
            "j/devices" => json!({ "keyboards": [
                { "main": false, "active_keymap": "English (US)" },
                { "main": true, "active_keymap": "Russian" }
            ] })
            .to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    )
}

#[tokio::test]
async fn a_snapshot_reads_every_part_of_the_model() {
    let server = snapshot_server();
    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.focused_output.as_deref(), Some("eDP-1"));
    assert_eq!(snapshot.focused_window, Some(WindowId(9)));
    assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
    assert_eq!(snapshot.windows[0].workspace_id, Some(WorkspaceId(5)));
    assert_eq!(snapshot.windows[0].layout_order, None);
    assert_eq!(snapshot.windows[0].size, Some((2880, 1800)));
    assert_eq!(
        snapshot.windows[0].rect,
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 2880.0,
            height: 1800.0
        }),
        "Hyprland reports both at and size for every window, unlike a tiled niri window"
    );
    assert_eq!(snapshot.windows[0].output.as_deref(), Some("eDP-1"));
    assert_eq!(snapshot.workspaces[0].name.as_deref(), Some("5"));
    assert!(snapshot.workspaces[0].is_active && snapshot.workspaces[0].is_focused);
    assert!(!snapshot.workspaces[1].is_active);
    assert_eq!(snapshot.workspaces[0].active_window_id, Some(WindowId(9)));
    // Hyprland writes `0x0` for an empty workspace, which is not a window at address zero.
    assert_eq!(snapshot.workspaces[1].active_window_id, None);
}

/// Hyprland reports focus once for the session rather than on each client, so without filling
/// it in `is_focused` would be false on every window while niri sets it on one.
#[tokio::test]
async fn the_focused_window_is_marked_the_way_niri_marks_it() {
    let server = snapshot_server();
    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.focused_window, Some(WindowId(9)));
    assert_eq!(
        snapshot
            .windows
            .iter()
            .filter(|window| window.is_focused)
            .map(|window| window.id)
            .collect::<Vec<_>>(),
        [WindowId(9)],
    );
}

/// Hyprland sends an absent title or class as `""` where niri sends `null`. Both have to reach
/// a caller as `None`, or a panel renders an empty label under one compositor and nothing
/// under the other.
#[tokio::test]
async fn an_empty_title_is_absent_rather_than_an_empty_string() {
    let server = FakeHyprland::spawn(
        |command| match command {
            "j/clients" => json!([{ "address": "0x1", "title": "", "class": "" }]).to_string(),
            "j/monitors" | "j/workspaces" => "[]".to_owned(),
            "j/activewindow" => "{}".to_owned(),
            "j/getoption input:kb_layout" => json!({ "str": "" }).to_string(),
            "j/devices" => json!({ "keyboards": [] }).to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );

    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.windows[0].title, None);
    assert_eq!(snapshot.windows[0].app_id, None);
}

/// The same question niri answers with a null mode, so both have to land on `enabled: false`.
#[tokio::test]
async fn a_disabled_monitor_reports_the_same_shape_niri_does() {
    let server = snapshot_server();
    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    let built_in = &snapshot.outputs[0];
    let external = &snapshot.outputs[1];

    assert!(built_in.enabled && built_in.built_in);
    assert_eq!(
        built_in.current_mode,
        Some(Mode {
            width: 2880,
            height: 1800,
            refresh_mhz: 120_001
        })
    );
    assert_eq!(built_in.description.as_deref(), Some("Samsung 14\""));
    // Hyprland reports the mode's pixels; niri reports the scaled logical size. 2880 / 1.25.
    let logical = built_in
        .logical
        .as_ref()
        .expect("an enabled output has a logical size");
    assert_eq!((logical.width, logical.height), (2304, 1440));
    assert_eq!(logical.scale, 1.25);
    assert!(!external.enabled);
    assert_eq!(external.current_mode, None);
    assert_eq!(external.logical, None);
    assert_eq!(
        external.description, None,
        "Hyprland sends an absent description as an empty string; a caller must not have to \
             know which compositor it came from to read one"
    );
}

/// Hyprland gives xkb codes and a display name from opposite ends, so both halves have to be
/// filled — otherwise the same layout renders as "Russian" here and "ru" under niri.
#[tokio::test]
async fn both_names_and_codes_are_populated() {
    let server = snapshot_server();
    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.keyboard.codes, ["pl", "ru"]);
    assert_eq!(snapshot.keyboard.names, ["pl", "Russian"]);
    assert_eq!(snapshot.keyboard.current, Some(1));
}

#[tokio::test]
async fn a_refusal_carries_the_reason_hyprland_gave() {
    let server = FakeHyprland::spawn(|_| "Invalid dispatcher".to_owned(), Vec::new());

    let failure = Hyprland::at(&server.dir)
        .focus_window(WindowTarget::Next)
        .await
        .expect_err("refused");

    assert!(matches!(failure, CompositorError::Refused(reason) if reason.contains("Invalid")));
}

#[tokio::test]
async fn an_empty_reply_is_a_closed_connection() {
    let server = FakeHyprland::spawn(|_| String::new(), Vec::new());

    let failure = Hyprland::at(&server.dir)
        .focus_window(WindowTarget::Next)
        .await
        .expect_err("closed");

    assert!(matches!(failure, CompositorError::Closed));
}

#[tokio::test]
async fn a_missing_socket_names_the_path_it_tried() {
    let failure = Hyprland::at("/nonexistent/hypr")
        .focus_window(WindowTarget::Next)
        .await
        .expect_err("no socket");

    assert!(
        failure
            .to_string()
            .contains("/nonexistent/hypr/.socket.sock"),
        "expected the path in {failure}"
    );
}

#[test]
fn every_addon_serializes_to_the_dispatch_hyprland_expects() {
    assert_eq!(
        move_to_output_command(WorkspaceId(4), "DP-3"),
        "dispatch moveworkspacetomonitor 4 DP-3"
    );
    assert_eq!(
        move_window_command(WindowId(0x5b), &WorkspaceTarget::Id(WorkspaceId(4))),
        "dispatch movetoworkspacesilent 4,address:0x5b",
        "the silent form is what keeps focus where the user left it"
    );
    assert_eq!(
        close_command(WindowId(0x5b)),
        "dispatch closewindow address:0x5b"
    );
    assert_eq!(focus_output_command("DP-3"), "dispatch focusmonitor DP-3");
}

#[tokio::test]
async fn reordering_a_workspace_is_refused_rather_than_dispatched() {
    let error = Hyprland::at("/nonexistent")
        .reorder_workspace(WorkspaceId(4), 2)
        .await
        .expect_err("refused");

    assert!(
        matches!(error, CompositorError::Unavailable(_)),
        "a dispatch would have failed to connect first, so this is what proves nothing was sent"
    );
}

#[tokio::test]
async fn stopping_a_screencast_is_refused_rather_than_dispatched() {
    let error = Hyprland::at("/nonexistent")
        .stop_screencast(2)
        .await
        .expect_err("refused");

    assert!(matches!(error, CompositorError::Unavailable(_)));
}

#[tokio::test]
async fn screenshotting_a_window_is_unavailable_rather_than_dispatched() {
    let error = Hyprland::at("/nonexistent")
        .screenshot_window(WindowId(9), Path::new("/tmp/shot.png"))
        .await
        .expect_err("unavailable");

    assert!(
        matches!(error, CompositorError::Unavailable(_)),
        "a dispatch would have failed to connect first, so this is what proves nothing was sent"
    );
}

#[tokio::test]
async fn a_window_s_rect_is_relative_to_its_own_monitors_origin_not_the_layout() {
    let server = FakeHyprland::spawn(
        |command| match command {
            "j/monitors" => json!([
                { "id": 0, "name": "eDP-1", "width": 2880, "height": 1800,
                  "refreshRate": 120.0, "x": 100, "y": 50, "scale": 1.0, "transform": 0,
                  "focused": true, "disabled": false }
            ])
            .to_string(),
            "j/workspaces" => "[]".to_owned(),
            "j/clients" => json!([
                { "address": "0x9", "title": "a terminal", "class": "foot",
                  "monitor": 0, "at": [150, 90], "size": [800, 600] },
                { "address": "0xa", "title": "orphan", "class": "foot",
                  "monitor": 7, "at": [150, 90], "size": [800, 600] }
            ])
            .to_string(),
            "j/activewindow" => "{}".to_owned(),
            "j/getoption input:kb_layout" => json!({ "str": "" }).to_string(),
            "j/devices" => json!({ "keyboards": [] }).to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );

    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(
        snapshot.windows[0].rect,
        Some(Rect {
            x: 50.0,
            y: 40.0,
            width: 800.0,
            height: 600.0
        }),
        "at is global layout coordinates; rect must be relative to the window's own monitor"
    );
    assert_eq!(
        snapshot.windows[1].rect, None,
        "a window whose monitor is unknown has no rect rather than a global one"
    );
}

#[tokio::test]
async fn power_off_monitors_dispatches_dpms_off() {
    let received: Arc<Mutex<Option<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeHyprland::spawn(
        move |command| {
            *sink.lock().expect("lock") = Some(command.to_owned());
            "ok".to_owned()
        },
        Vec::new(),
    );

    Hyprland::at(&server.dir)
        .power_off_monitors()
        .await
        .expect("dispatched");

    assert_eq!(
        received.lock().expect("lock").as_deref(),
        Some("dispatch dpms off")
    );
}

#[test]
fn every_target_serializes_to_the_command_hyprland_expects() {
    assert_eq!(
        layout_command(LayoutTarget::Index(2)),
        "dispatch switchxkblayout all 2"
    );
    assert_eq!(
        layout_command(LayoutTarget::Next),
        "dispatch switchxkblayout all next"
    );
    assert_eq!(
        layout_command(LayoutTarget::Prev),
        "dispatch switchxkblayout all prev"
    );

    assert_eq!(
        workspace_command(&WorkspaceTarget::Id(WorkspaceId(5))),
        "dispatch workspace 5"
    );
    assert_eq!(
        workspace_command(&WorkspaceTarget::Index(3)),
        "dispatch workspace 3"
    );
    assert_eq!(
        workspace_command(&WorkspaceTarget::Name("chat".to_owned())),
        "dispatch workspace name:chat"
    );
    assert_eq!(
        workspace_command(&WorkspaceTarget::Next),
        "dispatch workspace +1"
    );
    assert_eq!(
        workspace_command(&WorkspaceTarget::Prev),
        "dispatch workspace -1"
    );

    assert_eq!(
        window_command(WindowTarget::Id(WindowId(0x5591_e8b2_f5a0))),
        "dispatch focuswindow address:0x5591e8b2f5a0"
    );
    assert_eq!(window_command(WindowTarget::Next), "dispatch cyclenext");
    assert_eq!(
        window_command(WindowTarget::Prev),
        "dispatch cyclenext prev"
    );

    assert_eq!(
        rename_command(WorkspaceId(3), Some("chat")),
        "dispatch renameworkspace 3 chat"
    );
    assert_eq!(
        rename_command(WorkspaceId(3), None),
        "dispatch renameworkspace 3"
    );
}

/// Disabling a monitor discards its mode, so the only way back is the remembered config. The
/// zero-size fallback covers a monitor that was already off when it was first seen.
#[test]
fn a_remembered_monitor_config_restores_its_mode() {
    let config = MonitorConfig {
        width: 2880,
        height: 1800,
        refresh_hz: 120.001,
        x: 100,
        y: 0,
        scale: 1.25,
        transform: 0,
    };
    assert_eq!(
        config.keyword_args("eDP-1"),
        "eDP-1,2880x1800@120.001,100x0,1.25"
    );

    let rotated = MonitorConfig {
        transform: 3,
        ..config
    };
    assert_eq!(
        rotated.keyword_args("eDP-1"),
        "eDP-1,2880x1800@120.001,100x0,1.25,transform,3"
    );

    assert_eq!(
        MonitorConfig::default().keyword_args("DP-3"),
        "DP-3,preferred,auto,1"
    );
}

async fn events_from(lines: &[&str]) -> Vec<Event> {
    let server = FakeHyprland::spawn(
        |command| match command {
            "j/getoption input:kb_layout" => json!({ "str": "pl,ru" }).to_string(),
            "j/devices" => json!({ "keyboards": [] }).to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        lines.iter().map(|line| (*line).to_owned()).collect(),
    );

    Hyprland::at(&server.dir)
        .events()
        .await
        .expect("subscribed")
        .collect()
        .await
}

#[tokio::test]
async fn addresses_arrive_bare_in_events_and_prefixed_in_json() {
    let events = events_from(&["activewindowv2>>5591e8b2f5a0", "closewindow>>0x9"]).await;

    assert_eq!(
        events,
        [
            Event::WindowFocusChanged(Some(WindowId(0x5591_e8b2_f5a0))),
            Event::WindowClosed(WindowId(9)),
        ]
    );
}

/// Hyprland's fine-grained events carry an address and nothing else, so the honest answer is a
/// resync rather than a half-built record.
#[tokio::test]
async fn structural_events_ask_for_a_resync() {
    let events = events_from(&[
        "openwindow>>9,1,foot,a terminal",
        "monitorremovedv2>>1,DP-3,Dell",
        "configreloaded>>",
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::Resync(Resync::Structure),
            Event::Resync(Resync::Outputs),
            Event::Resync(Resync::Keyboard),
        ]
    );
}

#[tokio::test]
async fn an_unrecognized_line_is_skipped_and_the_stream_survives() {
    let events = events_from(&[
        "somethingHyprlandAddedLater>>1,2,3",
        "no separator at all",
        "closewindow>>0x4",
    ])
    .await;

    assert_eq!(events, [Event::WindowClosed(WindowId(4))]);
}

#[tokio::test]
async fn screencasts_keep_privacy_active_until_every_client_stops() {
    let events = events_from(&[
        "screencast>>1,monitor",
        "screencast>>1,window",
        "screencast>>0,monitor",
        "screencast>>0,window",
    ])
    .await;

    let active_cast = vec![Cast {
        stream_id: 0,
        session_id: None,
        kind: CastKind::Unknown,
        target: CastTarget::Unknown,
        pw_node_id: None,
        active: true,
    }];

    assert_eq!(
        events,
        [
            Event::CastsChanged(active_cast.clone()),
            Event::CastsChanged(active_cast.clone()),
            Event::CastsChanged(active_cast),
            Event::CastsChanged(Vec::new()),
        ]
    );
}

/// `activelayout` reports a display name against a configured list of xkb codes, and a keyboard
/// name may itself contain a comma.
#[tokio::test]
async fn a_layout_switch_resolves_a_display_name_to_a_configured_code() {
    let events = events_from(&[
        "activelayout>>AT Translated Set 2, keyboard,Russian",
        "activelayout>>kbd,Klingon",
    ])
    .await;

    assert_eq!(
        events,
        [Event::KeyboardLayoutSwitched {
            idx: 1,
            name: Some("Russian".to_owned()),
        }]
    );
}

/// A window title is attacker-controlled and Hyprland packs it into a comma-separated line, so
/// the split must not let a crafted title impersonate another field.
#[tokio::test]
async fn a_title_containing_a_comma_does_not_shift_the_other_fields() {
    let server = FakeHyprland::spawn(
        |command| match command {
            "j/clients" => json!([{ "address": "0x1",
                                    "title": "a, b, c\u{2068}spoof\u{2069}",
                                    "class": "foot" }])
            .to_string(),
            "j/monitors" | "j/workspaces" => "[]".to_owned(),
            "j/activewindow" => "{}".to_owned(),
            "j/getoption input:kb_layout" => json!({ "str": "" }).to_string(),
            "j/devices" => json!({ "keyboards": [] }).to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );

    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.windows[0].title.as_deref(), Some("a, b, cspoof"));
    assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
}
