use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use serde_json::json;

use super::testing::FakeHyprland;
use super::*;
use crate::event::Resync;
use crate::model::{Cast, CastKind, CastTarget, WindowId};

fn snapshot_server() -> FakeHyprland {
    snapshot_server_with(Vec::new())
}

fn snapshot_server_with(events: Vec<String>) -> FakeHyprland {
    FakeHyprland::spawn(
        |command| match command {
            "j/monitors all" => json!([
                { "id": 0, "name": "eDP-1", "description": "Samsung 14\"", "make": "Samsung",
                  "model": "ATNA60CL10", "width": 2880, "height": 1800,
                  "refreshRate": 120.001, "x": 0, "y": 0, "scale": 1.25, "transform": 0,
                  "focused": true, "disabled": false, "activeWorkspace": { "id": 5, "name": "5" },
                  "specialWorkspace": { "id": -98, "name": "special:magic" } },
                { "id": 1, "name": "DP-3", "description": "", "width": 0, "height": 0,
                  "refreshRate": 60.0, "x": 2880, "y": 0, "scale": 1, "transform": 0,
                  "focused": false, "disabled": true,
                  "activeWorkspace": { "id": -1337, "name": "chat" } }
            ])
            .to_string(),
            "j/workspaces" => json!([
                { "id": 5, "name": "5", "monitor": "eDP-1", "lastwindow": "0x9" },
                { "id": -1337, "name": "chat", "monitor": "DP-3", "lastwindow": "0x0" },
                { "id": -98, "name": "special:magic", "monitor": "eDP-1", "lastwindow": "0x0" }
            ])
            .to_string(),
            "j/clients" => json!([
                { "address": "0x9", "title": "a terminal", "class": "foot", "pid": 4265,
                  "workspace": { "id": 5, "name": "5" }, "floating": false,
                  "monitor": 0, "at": [0, 0], "size": [2880, 1800] },
                { "address": "0xa", "title": "a chat", "class": "foot", "pid": 4266,
                  "workspace": { "id": -1337, "name": "chat" }, "floating": false,
                  "monitor": 1, "at": [2880, 0], "size": [800, 600] }
            ])
            .to_string(),
            "j/activewindow" => json!({ "address": "0x9" }).to_string(),
            "j/devices" => json!({ "keyboards": [
                { "main": false, "layout": "us", "active_layout_index": 0,
                  "active_keymap": "English (US)" },
                { "main": true, "layout": "us,gb,us", "variant": ",,intl",
                  "active_layout_index": 2,
                  "active_keymap": "English (US, intl., with dead keys)" }
            ] })
            .to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        events,
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
    assert!(
        snapshot.workspaces[1].is_active && !snapshot.workspaces[1].is_focused,
        "chat is the active workspace of the unfocused DP-3"
    );
    assert_eq!(snapshot.workspaces[0].active_window_id, Some(WindowId(9)));
    assert_eq!(snapshot.workspaces[1].active_window_id, None);
    assert_eq!(
        snapshot.workspaces.len(),
        2,
        "the special workspace is not an ordinary pager workspace"
    );
    assert_eq!(snapshot.workspaces[1].id, WorkspaceId((-1337_i64) as u64));
    assert_eq!(snapshot.workspaces[1].name.as_deref(), Some("chat"));
    assert_eq!(
        snapshot.windows[1].workspace_id,
        Some(WorkspaceId((-1337_i64) as u64))
    );
}

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

#[tokio::test]
async fn an_empty_title_is_absent_rather_than_an_empty_string() {
    let server = FakeHyprland::spawn(
        |command| match command {
            "j/clients" => json!([{ "address": "0x1", "title": "", "class": "" }]).to_string(),
            "j/monitors all" | "j/workspaces" => "[]".to_owned(),
            "j/activewindow" => "{}".to_owned(),
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

#[tokio::test]
async fn reading_the_layouts_asks_for_nothing_else() {
    let received: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeHyprland::spawn(
        move |command| {
            sink.lock().expect("lock").push(command.to_owned());
            json!({ "keyboards": [
                { "main": true, "layout": "us,gb", "active_layout_index": 1,
                  "active_keymap": "English (UK)" }
            ] })
            .to_string()
        },
        Vec::new(),
    );

    let layouts = Hyprland::at(&server.dir)
        .keyboard_layouts()
        .await
        .expect("layouts");

    assert_eq!(layouts.codes, ["us", "gb"]);
    assert_eq!(layouts.current, Some(1));
    assert_eq!(*received.lock().expect("lock"), ["j/devices"]);
}

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

#[tokio::test]
async fn both_names_and_codes_are_populated() {
    let server = snapshot_server();
    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.keyboard.codes, ["us", "gb", "us"]);
    assert_eq!(
        snapshot.keyboard.names,
        ["us", "gb", "English (US, intl., with dead keys)"]
    );
    assert_eq!(snapshot.keyboard.current, Some(2));
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
            "j/monitors all" => json!([
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
async fn power_off_monitors_is_unavailable_and_sends_nothing() {
    let received: Arc<Mutex<Option<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeHyprland::spawn(
        move |command| {
            *sink.lock().expect("lock") = Some(command.to_owned());
            "ok".to_owned()
        },
        Vec::new(),
    );

    let outcome = Hyprland::at(&server.dir).power_off_monitors().await;

    assert!(matches!(outcome, Err(CompositorError::Unavailable(_))));
    assert_eq!(received.lock().expect("lock").as_deref(), None);
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
        plain_selector(&WorkspaceTarget::Id(WorkspaceId(5))).as_deref(),
        Some("5")
    );
    assert_eq!(
        plain_selector(&WorkspaceTarget::Index(3)).as_deref(),
        Some("3")
    );
    assert_eq!(
        plain_selector(&WorkspaceTarget::Name("chat".to_owned())).as_deref(),
        Some("name:chat")
    );
    assert_eq!(
        plain_selector(&WorkspaceTarget::Next).as_deref(),
        Some("+1")
    );
    assert_eq!(
        plain_selector(&WorkspaceTarget::Prev).as_deref(),
        Some("-1")
    );
    assert_eq!(
        plain_selector(&WorkspaceTarget::Id(WorkspaceId((-1337_i64) as u64))),
        None,
        "a bare negative selector would read as a relative move"
    );
    assert_eq!(workspace_command("5"), "dispatch workspace 5");
    assert_eq!(
        workspace_command("name:chat"),
        "dispatch workspace name:chat"
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
        rename_command("3", Some("chat")),
        "dispatch renameworkspace 3 chat"
    );
    assert_eq!(rename_command("3", None), "dispatch renameworkspace 3");
}

#[test]
fn the_lua_disable_form_quotes_the_connector() {
    assert_eq!(
        disable_eval("DP-3"),
        "eval hl.monitor({ output = \"DP-3\", disabled = true })"
    );
}

async fn events_from(lines: &[&str]) -> Vec<Event> {
    let server = FakeHyprland::spawn(
        |command| match command {
            "j/devices" => json!({ "keyboards": [
                { "main": true, "layout": "us,gb,us", "variant": ",,intl",
                  "active_layout_index": 1, "active_keymap": "English (UK)" }
            ] })
            .to_string(),
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

async fn snapshot_after(lines: &[&str]) -> Snapshot {
    let server = snapshot_server_with(lines.iter().map(|line| (*line).to_owned()).collect());
    let hyprland = Hyprland::at(&server.dir);
    let _: Vec<Event> = hyprland.events().await.expect("subscribed").collect().await;
    hyprland.snapshot().await.expect("snapshot")
}

#[tokio::test]
async fn a_snapshot_keeps_the_screencasts_the_stream_has_seen() {
    let snapshot = snapshot_after(&["screencast>>1,monitor", "workspacev2>>5,5"]).await;

    assert_eq!(
        snapshot.casts,
        [Cast {
            stream_id: 0,
            session_id: None,
            kind: CastKind::Unknown,
            target: CastTarget::Unknown,
            pw_node_id: None,
            active: true,
        }]
    );
}

#[tokio::test]
async fn a_snapshot_after_the_last_screencast_stops_holds_no_casts() {
    let snapshot = snapshot_after(&["screencast>>1,monitor", "screencast>>0,monitor"]).await;

    assert!(snapshot.casts.is_empty());
}

#[tokio::test]
async fn an_urgent_window_stays_urgent_across_a_resync_until_it_is_focused() {
    let snapshot = snapshot_after(&["urgent>>0x9", "windowtitlev2>>9,a title"]).await;
    assert!(snapshot.windows[0].is_urgent);
    assert!(!snapshot.windows[1].is_urgent);

    let snapshot = snapshot_after(&["urgent>>0x9", "activewindowv2>>9"]).await;
    assert!(!snapshot.windows[0].is_urgent);

    let snapshot = snapshot_after(&["urgent>>0x9", "closewindow>>0x9"]).await;
    assert!(!snapshot.windows[0].is_urgent);
}

#[tokio::test]
async fn a_layout_switch_takes_its_index_from_devices_not_the_display_name() {
    let events = events_from(&[
        "activelayout>>AT Translated Set 2, keyboard,English (UK)",
        "activelayout>>kbd,English (US, intl., with dead keys)",
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::KeyboardLayoutSwitched(1),
            Event::KeyboardLayoutSwitched(1),
        ]
    );
}

#[tokio::test]
async fn a_title_containing_a_comma_does_not_shift_the_other_fields() {
    let server = FakeHyprland::spawn(
        |command| match command {
            "j/clients" => json!([{ "address": "0x1",
                                    "title": "a, b, c\u{2068}spoof\u{2069}",
                                    "class": "foot" }])
            .to_string(),
            "j/monitors all" | "j/workspaces" => "[]".to_owned(),
            "j/activewindow" => "{}".to_owned(),
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

#[tokio::test]
async fn a_negative_workspace_id_is_dispatched_by_name() {
    let received: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeHyprland::spawn(
        move |command| {
            sink.lock().expect("lock").push(command.to_owned());
            match command {
                "j/workspaces" => json!([
                    { "id": -1337, "name": "chat", "monitor": "eDP-1", "lastwindow": "0x0" }
                ])
                .to_string(),
                _ => "ok".to_owned(),
            }
        },
        Vec::new(),
    );

    Hyprland::at(&server.dir)
        .focus_workspace(WorkspaceTarget::Id(WorkspaceId((-1337_i64) as u64)))
        .await
        .expect("dispatched");

    assert_eq!(
        *received.lock().expect("lock"),
        ["j/workspaces", "dispatch workspace name:chat"]
    );
}

fn monitor_server(reply: &'static str) -> (FakeHyprland, Arc<Mutex<Vec<String>>>) {
    let received: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeHyprland::spawn(
        move |command| {
            sink.lock().expect("lock").push(command.to_owned());
            match command {
                _ if command.starts_with("keyword ") => reply.to_owned(),
                "reload" => reply.to_owned(),
                _ => "ok".to_owned(),
            }
        },
        Vec::new(),
    );
    (server, received)
}

#[tokio::test]
async fn a_lua_config_refusal_retries_the_monitor_disable_through_eval() {
    let (server, received) =
        monitor_server("keyword can't work with non-legacy parsers. Use eval.");

    Hyprland::at(&server.dir)
        .set_output_enabled("DP-3", false)
        .await
        .expect("disabled through eval");

    assert_eq!(
        *received.lock().expect("lock"),
        [
            "keyword monitor DP-3,disable",
            "eval hl.monitor({ output = \"DP-3\", disabled = true })",
        ]
    );
}

#[tokio::test]
async fn a_legacy_config_never_sees_the_eval_form() {
    let (server, received) = monitor_server("ok");

    Hyprland::at(&server.dir)
        .set_output_enabled("DP-3", false)
        .await
        .expect("disabled");

    assert_eq!(
        *received.lock().expect("lock"),
        ["keyword monitor DP-3,disable"]
    );
}

#[tokio::test]
async fn a_refused_disable_is_an_error() {
    let (server, _received) = monitor_server("invalid monitor syntax");

    let outcome = Hyprland::at(&server.dir)
        .set_output_enabled("DP-3", false)
        .await;

    assert!(matches!(outcome, Err(CompositorError::Refused(_))));
}

#[tokio::test]
async fn enabling_a_monitor_sends_only_a_reload() {
    let (server, received) = monitor_server("ok");

    Hyprland::at(&server.dir)
        .set_output_enabled("DP-3", true)
        .await
        .expect("enabled");

    assert_eq!(*received.lock().expect("lock"), ["reload"]);
}

#[tokio::test]
async fn a_refused_reload_carries_hyprland_s_text() {
    let (server, _received) = monitor_server("config error: bad rule");

    let outcome = Hyprland::at(&server.dir)
        .set_output_enabled("DP-3", true)
        .await;

    assert!(matches!(
        outcome,
        Err(CompositorError::Refused(reason)) if reason == "config error: bad rule"
    ));
}

#[tokio::test]
async fn a_non_utf8_title_is_decoded_lossily_and_the_snapshot_continues() {
    let server = FakeHyprland::spawn_raw(
        |command| match command {
            "j/clients" => {
                b"[{\"address\":\"0x1\",\"title\":\"bad \xff title\",\"class\":\"foot\"}]".to_vec()
            }
            "j/monitors all" | "j/workspaces" => b"[]".to_vec(),
            "j/activewindow" => b"{}".to_vec(),
            "j/devices" => b"{\"keyboards\":[]}".to_vec(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );

    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(
        snapshot.windows[0].title.as_deref(),
        Some("bad \u{fffd} title")
    );
}

#[tokio::test]
async fn a_non_utf8_event_byte_does_not_end_the_stream() {
    let server = FakeHyprland::spawn_raw(
        |command| match command {
            "j/devices" => b"{\"keyboards\":[]}".to_vec(),
            other => panic!("the fake was asked for {other}"),
        },
        vec![
            b"windowtitlev2>>9,bad \xff title\n".to_vec(),
            b"closewindow>>0x4\n".to_vec(),
        ],
    );

    let events: Vec<Event> = Hyprland::at(&server.dir)
        .events()
        .await
        .expect("subscribed")
        .collect()
        .await;

    assert_eq!(
        events,
        [
            Event::Resync(Resync::Structure),
            Event::WindowClosed(WindowId(4)),
        ]
    );
}

#[tokio::test]
async fn only_the_second_title_event_asks_for_a_resync() {
    let events = events_from(&["windowtitle>>9", "windowtitlev2>>9,a title"]).await;

    assert_eq!(events, [Event::Resync(Resync::Structure)]);
}

async fn session_lock_restore(int: i64) -> bool {
    let server = FakeHyprland::spawn(
        move |command| match command {
            "j/getoption misc:allow_session_lock_restore" => json!({
                "option": "misc:allow_session_lock_restore", "int": int, "set": false
            })
            .to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );
    Hyprland::at(&server.dir)
        .allow_session_lock_restore()
        .await
        .expect("an answer")
}

#[tokio::test]
async fn the_session_lock_restore_option_reads_its_int() {
    assert!(!session_lock_restore(0).await);
    assert!(session_lock_restore(1).await);
}
