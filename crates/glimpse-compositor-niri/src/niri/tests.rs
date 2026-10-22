use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use glimpse_compositors::event::{LineDecoder, Resync};
use glimpse_compositors::model::Cast;
use serde_json::json;

use super::event::Wire;
use super::testing::FakeNiri;
use super::*;

fn reply(name: &str, payload: Value) -> Vec<String> {
    vec![json!({ "Ok": { name: payload } }).to_string()]
}

fn outputs() -> Value {
    json!({
        "eDP-1": {
            "name": "eDP-1", "make": "Samsung Display Corp.", "model": "ATNA60CL10-0",
            "modes": [{ "width": 2880, "height": 1800, "refresh_rate": 120000 }],
            "current_mode": 0,
            "logical": { "x": 0, "y": 0, "width": 2304, "height": 1440,
                         "scale": 1.25, "transform": "Normal" }
        },
        "DP-3": {
            "name": "DP-3", "make": "Dell", "model": "U2720Q",
            "modes": [{ "width": 3840, "height": 2160, "refresh_rate": 60000 }],
            "current_mode": null, "logical": null
        }
    })
}

fn snapshot_server() -> FakeNiri {
    snapshot_server_with_casts(reply(
        "Casts",
        json!([{ "stream_id": 8, "is_active": true }]),
    ))
}

fn snapshot_server_with_casts(casts: Vec<String>) -> FakeNiri {
    FakeNiri::spawn(move |request| match request {
        "\"Outputs\"" => reply("Outputs", outputs()),
        "\"Workspaces\"" => reply(
            "Workspaces",
            json!([{ "id": 5, "idx": 1, "name": null, "output": "eDP-1",
                     "is_urgent": false, "is_active": true, "is_focused": true,
                     "active_window_id": 9 }]),
        ),
        "\"Windows\"" => reply(
            "Windows",
            json!([{ "id": 9, "title": "a terminal", "app_id": "foot", "pid": 4265,
                     "workspace_id": 5, "is_focused": true, "is_floating": false,
                     "is_urgent": false,
                     "layout": { "pos_in_scrolling_layout": [2, 0] } }]),
        ),
        "\"KeyboardLayouts\"" => reply(
            "KeyboardLayouts",
            json!({ "names": ["Polish", "Russian"], "current_idx": 1 }),
        ),
        "\"FocusedOutput\"" => reply("FocusedOutput", json!({ "name": "eDP-1" })),
        "\"Casts\"" => casts.clone(),
        other => panic!("the fake was asked for {other}"),
    })
}

fn is_refused(failure: &Error, needle: &str) -> bool {
    matches!(failure, Error::Refused(reason) if reason.contains(needle))
}

#[tokio::test]
async fn a_snapshot_reads_every_part_of_the_model() {
    let server = snapshot_server();
    let snapshot = Niri::at(&server.socket).snapshot().await.expect("snapshot");

    assert_eq!(snapshot.focus.output.as_deref(), Some("eDP-1"));
    assert_eq!(
        snapshot
            .casts
            .iter()
            .map(|cast| cast.id)
            .collect::<Vec<_>>(),
        [8]
    );
    assert!(snapshot.casts[0].active);
    assert_eq!(snapshot.focus.window, Some(WindowId(9)));
    assert_eq!(snapshot.workspaces[0].id, WorkspaceId(5));
    assert_eq!(snapshot.workspaces[0].index, Some(1));
    assert!(snapshot.workspaces[0].active);
    assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
    assert_eq!(snapshot.windows[0].order, Some(2));
    assert_eq!(snapshot.windows[0].workspace, Some(WorkspaceId(5)));
    assert_eq!(snapshot.keyboard.names, ["Polish", "Russian"]);
    assert!(snapshot.keyboard.codes.is_empty());
    assert_eq!(snapshot.keyboard.current, Some(1));
}

#[test]
fn a_model_with_the_edid_trailing_space_is_trimmed() {
    let output: WireOutput = serde_json::from_value(json!({
        "name": "eDP-1", "model": "ATNA60CL10-0 ", "current_mode": null
    }))
    .expect("wire output");

    assert_eq!(output.into_model().model.as_deref(), Some("ATNA60CL10-0"));
}

#[tokio::test]
async fn a_failed_cast_snapshot_fails_closed() {
    let server = snapshot_server_with_casts(vec![
        json!({ "Err": "casts are temporarily unavailable" }).to_string(),
    ]);

    assert!(Niri::at(&server.socket).snapshot().await.is_err());
}

#[tokio::test]
async fn an_output_with_no_current_mode_is_disabled() {
    let server = snapshot_server();
    let snapshot = Niri::at(&server.socket).snapshot().await.expect("snapshot");

    let built_in = snapshot
        .outputs
        .iter()
        .find(|output| output.connector == "eDP-1")
        .expect("eDP-1");
    let external = snapshot
        .outputs
        .iter()
        .find(|output| output.connector == "DP-3")
        .expect("DP-3");

    assert!(built_in.enabled);
    assert_eq!(
        built_in.mode,
        Some(Mode {
            width: 2880,
            height: 1800,
            refresh_mhz: 120_000
        })
    );
    assert_eq!(built_in.logical.map(|logical| logical.scale), Some(1.25));
    assert!(!external.enabled);
    assert_eq!(external.mode, None);
}

#[tokio::test]
async fn a_refusal_carries_the_reason_niri_gave() {
    let server = FakeNiri::spawn(|_| vec![json!({ "Err": "no such workspace" }).to_string()]);

    let failure = Niri::at(&server.socket)
        .focus_workspace(WorkspaceId(9))
        .await
        .expect_err("refused");

    assert!(is_refused(&failure, "no such"));
}

#[tokio::test]
async fn a_disconnected_output_is_refused_naming_the_connector() {
    let server = FakeNiri::spawn(|_| {
        vec![json!({ "Ok": { "OutputConfigChanged": "OutputWasMissing" } }).to_string()]
    });

    let failure = Niri::at(&server.socket)
        .set_output_enabled("DP-9".to_owned(), false)
        .await
        .expect_err("missing");

    assert!(is_refused(&failure, "DP-9"));
}

#[tokio::test]
async fn an_applied_output_change_succeeds() {
    let server = FakeNiri::spawn(|_| {
        vec![json!({ "Ok": { "OutputConfigChanged": "Applied" } }).to_string()]
    });

    Niri::at(&server.socket)
        .set_output_enabled("DP-3".to_owned(), true)
        .await
        .expect("applied");
}

#[tokio::test]
async fn a_connection_closed_before_a_reply_is_unavailable() {
    let server = FakeNiri::silent();

    let failure = Niri::at(&server.socket)
        .step_window(true)
        .await
        .expect_err("closed");

    assert!(matches!(failure, Error::Unavailable(_)));
}

#[tokio::test]
async fn an_undecodable_reply_is_a_failure() {
    let server = FakeNiri::spawn(|_| vec!["not json at all".to_owned()]);

    let failure = Niri::at(&server.socket)
        .step_window(true)
        .await
        .expect_err("garbled");

    assert!(matches!(failure, Error::Failed(_)));
}

#[tokio::test]
async fn a_missing_socket_names_the_path_it_tried() {
    let failure = Niri::at("/nonexistent/niri.sock")
        .snapshot()
        .await
        .expect_err("no socket");

    assert!(
        format!("{failure:?}").contains("/nonexistent/niri.sock"),
        "expected the path in {failure:?}"
    );
}

#[test]
fn every_target_serializes_to_the_action_niri_expects() {
    assert_eq!(
        layout_action(json!({ "Index": 2 })),
        json!({ "SwitchLayout": { "layout": { "Index": 2 } } })
    );
    assert_eq!(
        layout_action(json!("Next")),
        json!({ "SwitchLayout": { "layout": "Next" } })
    );
    assert_eq!(
        layout_action(json!("Prev")),
        json!({ "SwitchLayout": { "layout": "Prev" } })
    );

    assert_eq!(
        step_workspace_action(true),
        json!({ "FocusWorkspaceDown": {} })
    );
    assert_eq!(
        step_workspace_action(false),
        json!({ "FocusWorkspaceUp": {} })
    );

    assert_eq!(
        step_window_action(true),
        json!({ "FocusWindowDownOrColumnRight": {} })
    );
    assert_eq!(
        step_window_action(false),
        json!({ "FocusWindowUpOrColumnLeft": {} })
    );

    assert_eq!(
        rename_action(WorkspaceId(3), "chat"),
        json!({ "SetWorkspaceName": { "name": "chat", "workspace": { "Id": 3 } } })
    );
    assert_eq!(
        rename_action(WorkspaceId(3), ""),
        json!({ "UnsetWorkspaceName": { "reference": { "Id": 3 } } })
    );

    assert_eq!(
        output_request("eDP-1", true),
        json!({ "Output": { "output": "eDP-1", "action": "On" } })
    );
    assert_eq!(
        output_request("eDP-1", false),
        json!({ "Output": { "output": "eDP-1", "action": "Off" } })
    );

    assert_eq!(
        screenshot_action(WindowId(9), "/tmp/shot.png"),
        json!({
            "ScreenshotWindow": {
                "id": 9,
                "write_to_disk": true,
                "show_pointer": false,
                "path": "/tmp/shot.png"
            }
        })
    );
}

async fn received_by(call: impl AsyncFnOnce(Niri) -> Result<(), Error>) -> Option<String> {
    let received: Arc<Mutex<Option<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeNiri::spawn(move |request| {
        *sink.lock().expect("lock") = Some(request.to_owned());
        vec![json!({ "Ok": null }).to_string()]
    });

    call(Niri::at(&server.socket)).await.expect("dispatched");

    received.lock().expect("lock").clone()
}

#[tokio::test]
async fn focus_workspace_and_window_send_their_ids() {
    assert_eq!(
        received_by(async |niri| niri.focus_workspace(WorkspaceId(5)).await)
            .await
            .as_deref(),
        Some(r#"{"Action":{"FocusWorkspace":{"reference":{"Id":5}}}}"#)
    );
    assert_eq!(
        received_by(async |niri| niri.focus_window(WindowId(9)).await)
            .await
            .as_deref(),
        Some(r#"{"Action":{"FocusWindow":{"id":9}}}"#)
    );
}

#[tokio::test]
async fn keyboard_layout_commands_send_the_native_action() {
    assert_eq!(
        received_by(async |niri| niri.switch_keyboard_layout(2).await)
            .await
            .as_deref(),
        Some(r#"{"Action":{"SwitchLayout":{"layout":{"Index":2}}}}"#)
    );
    assert_eq!(
        received_by(async |niri| niri.step_keyboard_layout(false).await)
            .await
            .as_deref(),
        Some(r#"{"Action":{"SwitchLayout":{"layout":"Prev"}}}"#)
    );
}

#[tokio::test]
async fn power_off_monitors_sends_the_action_envelope() {
    assert_eq!(
        received_by(async |niri| niri.power_off_monitors().await)
            .await
            .as_deref(),
        Some(r#"{"Action":{"PowerOffMonitors":{}}}"#)
    );
}

#[tokio::test]
async fn stop_screencast_sends_the_session_id() {
    assert_eq!(
        received_by(async |niri| niri.stop_screencast(2).await)
            .await
            .as_deref(),
        Some(r#"{"Action":{"StopCast":{"session_id":2}}}"#)
    );
}

#[tokio::test]
async fn the_watch_is_armed_before_the_action_is_sent() {
    let received: Arc<Mutex<Option<String>>> = Arc::default();
    let sink = received.clone();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("shot.png");
    let target = path.clone();
    let server = FakeNiri::spawn(move |request| {
        *sink.lock().expect("lock") = Some(request.to_owned());
        std::fs::write(&target, b"not really a png").expect("write the screenshot file");
        vec![json!({ "Ok": null }).to_string()]
    });

    Niri::at(&server.socket)
        .screenshot_window(WindowId(9), path.clone())
        .await
        .expect("screenshot");

    assert_eq!(
        received.lock().expect("lock").as_deref(),
        Some(
            json!({
                "Action": {
                    "ScreenshotWindow": {
                        "id": 9,
                        "write_to_disk": true,
                        "show_pointer": false,
                        "path": path.to_str().expect("utf-8 path"),
                    }
                }
            })
            .to_string()
            .as_str()
        )
    );
}

#[tokio::test]
async fn screenshot_window_returns_only_after_the_file_exists() {
    let server = FakeNiri::spawn(|_| vec![json!({ "Ok": null }).to_string()]);
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("shot.png");
    let target = path.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(&target, b"not really a png").expect("write the screenshot file");
    });

    Niri::at(&server.socket)
        .screenshot_window(WindowId(9), path.clone())
        .await
        .expect("screenshot");

    assert!(path.exists());
}

#[test]
fn a_close_write_watch_times_out_when_no_matching_file_ever_arrives() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let watch = arm_close_write_watch(dir.path()).expect("watch armed");

    let failure = wait_for_close_write(watch, OsStr::new("never.png"), Duration::from_millis(30))
        .expect_err("nothing ever closes that file");

    assert!(matches!(failure, Error::Failed(_)));
}

#[test]
fn a_close_write_watch_ignores_an_unrelated_file_closing_in_the_same_directory() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let watch = arm_close_write_watch(dir.path()).expect("watch armed");
    std::fs::write(dir.path().join("other.png"), b"decoy").expect("write a decoy file");

    let failure = wait_for_close_write(watch, OsStr::new("shot.png"), Duration::from_millis(30))
        .expect_err("the decoy must not satisfy the watch");

    assert!(matches!(failure, Error::Failed(_)));
}

#[test]
fn a_live_chrome_cast_decodes_its_output_and_session() {
    let wire: WireCast = serde_json::from_value(json!({
        "stream_id": 2, "session_id": 2, "kind": "PipeWire",
        "target": { "Output": { "name": "DP-2" } },
        "is_active": true, "pid": null, "pw_node_id": 107
    }))
    .expect("wire cast");

    assert_eq!(
        wire.into_model(),
        Cast {
            id: 2,
            active: true,
            output: Some("DP-2".to_owned()),
            session: Some(2),
            pw_node: Some(107),
        }
    );
}

#[test]
fn a_wlr_screencopy_cast_carries_no_session_and_no_attribution() {
    let wire: WireCast = serde_json::from_value(json!({
        "stream_id": 4, "session_id": 4, "is_active": true, "kind": "WlrScreencopy",
    }))
    .expect("wire cast");

    let cast = wire.into_model();

    assert_eq!(cast.session, None);
    assert_eq!(cast.pw_node, None);
}

#[test]
fn a_cast_with_only_a_stream_id_decodes_as_bare() {
    let wire: WireCast = serde_json::from_value(json!({ "stream_id": 9 })).expect("wire cast");

    let cast = wire.into_model();

    assert_eq!(cast.output, None);
    assert_eq!(cast.session, None);
    assert!(!cast.active);
}

#[test]
fn a_cast_kind_niri_added_later_has_no_session_instead_of_failing() {
    let wire: WireCast = serde_json::from_value(json!({
        "stream_id": 1, "session_id": 1, "kind": "SomeFutureKind"
    }))
    .expect("wire cast");

    assert_eq!(wire.into_model().session, None);
}

#[test]
fn a_cast_target_niri_added_later_has_no_output_instead_of_failing() {
    let wire: WireCast =
        serde_json::from_value(json!({ "stream_id": 1, "target": { "SomethingNew": {} } }))
            .expect("wire cast");

    assert_eq!(wire.into_model().output, None);
}

async fn events_from(lines: Vec<Value>) -> Vec<Event> {
    let scripted: Vec<String> = std::iter::once(json!({ "Ok": { "Handled": null } }))
        .chain(lines)
        .map(|line| line.to_string())
        .collect();
    let server = FakeNiri::spawn(move |_| scripted.clone());

    Niri::at(&server.socket)
        .events()
        .await
        .expect("subscribed")
        .collect()
        .await
}

#[tokio::test]
async fn the_stream_consumes_the_acknowledgement_and_decodes_what_follows() {
    let events = events_from(vec![
        json!({ "WindowClosed": { "id": 9 } }),
        json!({ "WindowFocusChanged": { "id": null } }),
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::WindowClosed(WindowId(9)),
            Event::WindowFocusChanged(None),
        ]
    );
}

#[tokio::test]
async fn an_unrecognized_event_is_skipped_and_the_stream_survives() {
    let events = events_from(vec![
        json!({ "SomethingNiriAddedLater": { "whatever": true } }),
        json!({ "WindowClosed": { "id": 4 } }),
    ])
    .await;

    assert_eq!(events, [Event::WindowClosed(WindowId(4))]);
}

#[tokio::test]
async fn a_layout_switch_carries_its_index() {
    let events = events_from(vec![
        json!({ "KeyboardLayoutsChanged": {
            "keyboard_layouts": { "names": ["Polish", "Russian"], "current_idx": 0 } } }),
        json!({ "KeyboardLayoutSwitched": { "idx": 1 } }),
    ])
    .await;

    assert_eq!(events[1], Event::KeyboardLayoutSwitched(1));
}

#[tokio::test]
async fn reading_the_layouts_asks_for_nothing_else() {
    let requests: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = requests.clone();
    let server = FakeNiri::spawn(move |request| {
        sink.lock().expect("lock").push(request.to_owned());
        reply(
            "KeyboardLayouts",
            json!({ "names": ["Polish", "Russian"], "current_idx": 1 }),
        )
    });

    let layouts = Niri::at(&server.socket)
        .keyboard_layouts()
        .await
        .expect("layouts");

    assert!(layouts.codes.is_empty());
    assert_eq!(layouts.current, Some(1));
    assert_eq!(*requests.lock().expect("lock"), ["\"KeyboardLayouts\""]);
}

#[tokio::test]
async fn losing_a_monitor_asks_for_an_output_resync() {
    let workspaces = |outputs: [&str; 2]| {
        json!({ "WorkspacesChanged": { "workspaces": [
            { "id": 1, "output": outputs[0] },
            { "id": 2, "output": outputs[1] },
        ] } })
    };

    let events = events_from(vec![
        workspaces(["eDP-1", "DP-3"]),
        workspaces(["eDP-1", "DP-3"]),
        workspaces(["eDP-1", "eDP-1"]),
    ])
    .await;

    assert_eq!(events[0], Event::Resync(Resync::Outputs));
    assert!(
        matches!(events[1], Event::WorkspacesChanged { .. }),
        "the first list is also delivered, got {:?}",
        events[1]
    );
    assert!(
        matches!(events[2], Event::WorkspacesChanged { .. }),
        "an unchanged monitor set must not resync, got {:?}",
        events[2]
    );
    assert_eq!(events[3], Event::Resync(Resync::Outputs));
}

#[tokio::test]
async fn the_focused_workspace_rides_along_with_the_list() {
    let events = events_from(vec![json!({ "WorkspacesChanged": { "workspaces": [
        { "id": 1, "output": "eDP-1", "is_active": true },
        { "id": 2, "output": "eDP-1", "is_focused": true },
    ] } })])
    .await;

    assert!(matches!(
        events[1],
        Event::WorkspacesChanged {
            focused: Some(WorkspaceId(2)),
            ..
        }
    ));
}

#[tokio::test]
async fn urgency_and_layout_order_arrive_incrementally() {
    let events = events_from(vec![
        json!({ "WindowUrgencyChanged": { "id": 9, "urgent": true } }),
        json!({ "WindowLayoutsChanged": { "changes": [
            [9, { "pos_in_scrolling_layout": [3, 1] }],
            [4, { "pos_in_scrolling_layout": null }],
        ] } }),
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::WindowUrgencyChanged {
                id: WindowId(9),
                urgent: true
            },
            Event::WindowLayoutsChanged(vec![(WindowId(9), Some(3)), (WindowId(4), None)]),
        ]
    );
}

#[tokio::test]
async fn screencasts_report_the_full_set_changes_and_stops() {
    let events = events_from(vec![
        json!({ "CastsChanged": { "casts": [
            { "stream_id": 3, "is_active": true },
            { "stream_id": 5, "is_active": false },
        ] } }),
        json!({ "CastStartedOrChanged": { "cast": {
            "stream_id": 7, "is_active": true,
        } } }),
        json!({ "CastStopped": { "stream_id": 3 } }),
    ])
    .await;

    let cast = |id, active| Cast {
        id,
        active,
        output: None,
        session: None,
        pw_node: None,
    };

    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(3, true), cast(5, false)]),
            Event::CastStartedOrChanged(cast(7, true)),
            Event::CastStopped(3),
        ]
    );
}

#[tokio::test]
async fn a_successful_config_reload_asks_for_a_keyboard_and_output_resync() {
    let events = events_from(vec![
        json!({ "ConfigLoaded": { "failed": true } }),
        json!({ "ConfigLoaded": { "failed": false } }),
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::Resync(Resync::Keyboard),
            Event::Resync(Resync::Outputs),
        ]
    );
}

#[tokio::test]
async fn a_hostile_window_title_is_capped_before_it_leaves_the_crate() {
    let long = "x".repeat(600);
    let title = format!("\u{2068}spoof\u{2069}{long}");
    let server = FakeNiri::spawn(move |request| match request {
        "\"Windows\"" => reply(
            "Windows",
            json!([{ "id": 1, "title": title, "app_id": "a", "is_focused": false }]),
        ),
        "\"Outputs\"" => reply("Outputs", json!({})),
        "\"Workspaces\"" => reply("Workspaces", json!([])),
        "\"KeyboardLayouts\"" => reply("KeyboardLayouts", json!({ "names": [], "current_idx": 0 })),
        "\"FocusedOutput\"" => reply("FocusedOutput", json!(null)),
        "\"Casts\"" => reply("Casts", json!([])),
        other => panic!("the fake was asked for {other}"),
    });

    let snapshot = Niri::at(&server.socket).snapshot().await.expect("snapshot");
    let title = snapshot.windows[0].title.clone().expect("a title");

    assert!(!title.contains('\u{2068}'), "bidi overrides survived");
    assert_eq!(title.chars().count(), 512);
    assert!(title.starts_with("spoof"));
}

#[tokio::test]
async fn a_floating_window_has_a_geometry_and_a_tiled_one_has_none() {
    let server = FakeNiri::spawn(|_| {
        reply(
            "Windows",
            json!([
                { "id": 1, "is_floating": true, "layout": {
                    "pos_in_scrolling_layout": null,
                    "window_size": [400, 300],
                    "tile_pos_in_workspace_view": [100.0, 50.0],
                    "window_offset_in_tile": [2.0, 3.0] } },
                { "id": 2, "is_floating": false, "layout": {
                    "pos_in_scrolling_layout": [1, 1],
                    "window_size": [800, 600],
                    "tile_pos_in_workspace_view": null,
                    "window_offset_in_tile": [0.0, 0.0] } },
                { "id": 3, "layout": null },
            ]),
        )
    });

    let geometries = Niri::at(&server.socket)
        .window_geometries()
        .await
        .expect("geometries");

    assert_eq!(
        geometries,
        HashMap::from([(
            WindowId(1),
            Rect {
                x: 102.0,
                y: 53.0,
                width: 400.0,
                height: 300.0
            }
        )])
    );
}

#[tokio::test]
async fn probe_is_true_when_niri_answers_the_version_request() {
    let server = FakeNiri::spawn(|request| {
        assert_eq!(request, "\"Version\"");
        reply("Version", json!("25.08"))
    });

    assert!(Niri::at(&server.socket).probe().await);
}

#[tokio::test]
async fn probe_is_false_when_nothing_listens_on_the_socket() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let socket = dir.path().join("stale.sock");
    drop(tokio::net::UnixListener::bind(&socket).expect("bind"));

    assert!(socket.exists());
    assert!(!Niri::at(&socket).probe().await);
}

#[tokio::test(start_paused = true)]
async fn probe_gives_up_on_a_listener_that_never_replies() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let socket = dir.path().join("silent.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });

    let start = tokio::time::Instant::now();
    let probe = tokio::time::timeout(PROBE_DEADLINE * 2, Niri::at(&socket).probe()).await;

    assert!(
        probe.is_ok(),
        "the probe did not give up within {PROBE_DEADLINE:?}"
    );
    assert!(
        !probe.expect("the probe already finished"),
        "a silent listener must report no niri"
    );
    assert_eq!(start.elapsed(), PROBE_DEADLINE);
}

#[tokio::test]
async fn events_niri_reports_but_the_model_drops_decode_as_known_no_ops() {
    for line in [
        json!({ "WorkspaceUrgencyChanged": { "id": 5, "urgent": true } }).to_string(),
        json!({ "WorkspaceActiveWindowChanged": { "workspace_id": 5, "active_window_id": 7 } })
            .to_string(),
    ] {
        serde_json::from_str::<Wire>(&line).expect("a known niri event");
        assert!(EventState::default().decode(&line).await.is_empty());
    }
}

#[tokio::test]
async fn a_refused_event_stream_is_refused() {
    let server = FakeNiri::spawn(|request| {
        assert_eq!(request, "\"EventStream\"");
        vec![json!({ "Err": "denied" }).to_string()]
    });

    let Err(failure) = Niri::at(&server.socket).events().await else {
        panic!("the event stream was not refused");
    };

    assert!(is_refused(&failure, "denied"));
}
