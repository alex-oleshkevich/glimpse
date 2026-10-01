use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use serde_json::json;

use super::testing::FakeNiri;
use super::*;
use crate::event::Resync;
use crate::model::{Cast, CastKind, CastTarget, WindowId};

/// Captured from a live niri 26.04 and re-wrapped: `niri msg -j` prints the payload without the
/// `{"Ok": …}` envelope the socket actually carries.
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

#[tokio::test]
async fn a_snapshot_reads_every_part_of_the_model() {
    let server = snapshot_server();
    let snapshot = Niri::at(&server.socket).snapshot().await.expect("snapshot");

    assert_eq!(snapshot.focused_output.as_deref(), Some("eDP-1"));
    assert_eq!(
        snapshot
            .casts
            .iter()
            .map(|cast| cast.stream_id)
            .collect::<Vec<_>>(),
        [8]
    );
    assert!(snapshot.casts[0].active);
    assert_eq!(snapshot.focused_window, Some(WindowId(9)));
    assert_eq!(snapshot.workspaces[0].id, WorkspaceId(5));
    assert_eq!(snapshot.workspaces[0].idx, Some(1));
    assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
    assert_eq!(snapshot.windows[0].layout_order, Some(2));
    assert_eq!(
        snapshot.windows[0].output.as_deref(),
        Some("eDP-1"),
        "a window's output is filled from its workspace once the snapshot is assembled"
    );
    assert_eq!(snapshot.keyboard.names, ["Polish", "Russian"]);
    assert_eq!(snapshot.keyboard.codes, ["PL", "RU"]);
    assert_eq!(snapshot.keyboard.current, Some(1));
}

#[test]
fn a_serial_decodes_when_present_null_or_absent() {
    let with_serial: WireOutput = serde_json::from_value(json!({
        "name": "DP-2", "serial": "69QC174", "current_mode": null
    }))
    .expect("wire output");
    assert_eq!(with_serial.into_model().serial, Some("69QC174".to_owned()));

    let null_serial: WireOutput = serde_json::from_value(json!({
        "name": "eDP-1", "serial": null, "current_mode": null
    }))
    .expect("wire output");
    assert_eq!(null_serial.into_model().serial, None);

    let absent_serial: WireOutput = serde_json::from_value(json!({
        "name": "eDP-1", "current_mode": null
    }))
    .expect("wire output");
    assert_eq!(absent_serial.into_model().serial, None);
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

/// Niri has no `enabled` field, so a null `current_mode` is the only thing that says an output
/// is switched off. Reading it as "no mode known" would leave a dead monitor in the list.
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
    assert!(built_in.built_in);
    assert_eq!(
        built_in.current_mode,
        Some(Mode {
            width: 2880,
            height: 1800,
            refresh_mhz: 120_000
        })
    );
    assert!(!external.enabled);
    assert!(!external.built_in);
    assert_eq!(external.current_mode, None);
}

#[tokio::test]
async fn a_refusal_carries_the_reason_niri_gave() {
    let server = FakeNiri::spawn(|_| vec![json!({ "Err": "no such workspace" }).to_string()]);

    let failure = Niri::at(&server.socket)
        .focus_workspace(WorkspaceTarget::Index(9))
        .await
        .expect_err("refused");

    assert!(matches!(failure, CompositorError::Refused(reason) if reason.contains("no such")));
}

#[tokio::test]
async fn a_connection_closed_before_a_reply_is_not_a_protocol_error() {
    let server = FakeNiri::silent();

    let failure = Niri::at(&server.socket)
        .focus_window(WindowTarget::Next)
        .await
        .expect_err("closed");

    assert!(matches!(failure, CompositorError::Closed));
}

#[tokio::test]
async fn an_undecodable_reply_is_a_protocol_error() {
    let server = FakeNiri::spawn(|_| vec!["not json at all".to_owned()]);

    let failure = Niri::at(&server.socket)
        .focus_window(WindowTarget::Next)
        .await
        .expect_err("garbled");

    assert!(matches!(failure, CompositorError::Protocol(_)));
}

#[tokio::test]
async fn a_missing_socket_names_the_path_it_tried() {
    let failure = Niri::at("/nonexistent/niri.sock")
        .snapshot()
        .await
        .expect_err("no socket");

    assert!(
        failure.to_string().contains("/nonexistent/niri.sock"),
        "expected the path in {failure}"
    );
}

/// The one test that catches an upstream rename: every target, against the literal JSON niri
/// 26.04 accepts.
#[test]
fn every_addon_names_its_subject_rather_than_acting_on_the_focused_one() {
    assert_eq!(
        move_to_output_action(WorkspaceId(4), "DP-3"),
        json!({ "MoveWorkspaceToMonitor": { "output": "DP-3", "reference": { "Id": 4 } } })
    );
    assert_eq!(
        reorder_action(WorkspaceId(4), 2),
        json!({ "MoveWorkspaceToIndex": { "index": 2, "reference": { "Id": 4 } } })
    );
    assert_eq!(
        close_action(WindowId(9)),
        json!({ "CloseWindow": { "id": 9 } })
    );
    assert_eq!(
        focus_output_action("DP-3"),
        json!({ "FocusMonitor": { "output": "DP-3" } })
    );
}

#[test]
fn moving_a_window_to_a_workspace_does_not_follow_it_there() {
    let action = move_window_action(WindowId(9), &WorkspaceTarget::Id(WorkspaceId(4)))
        .expect("an id is a reference");
    assert_eq!(
        action,
        json!({
            "MoveWindowToWorkspace": {
                "reference": { "Id": 4 },
                "window_id": 9,
                "focus": false
            }
        }),
        "niri defaults focus to true, which would drag the user after the window"
    );
}

#[test]
fn a_relative_workspace_is_not_a_reference_a_window_can_be_moved_to() {
    assert!(matches!(
        move_window_action(WindowId(9), &WorkspaceTarget::Next),
        Err(CompositorError::Unavailable(_))
    ));
}

#[test]
fn every_target_serializes_to_the_action_niri_expects() {
    assert_eq!(
        layout_action(LayoutTarget::Index(2)),
        json!({ "SwitchLayout": { "layout": { "Index": 2 } } })
    );
    assert_eq!(
        layout_action(LayoutTarget::Next),
        json!({ "SwitchLayout": { "layout": "Next" } })
    );
    assert_eq!(
        layout_action(LayoutTarget::Prev),
        json!({ "SwitchLayout": { "layout": "Prev" } })
    );

    assert_eq!(
        workspace_action(WorkspaceTarget::Id(WorkspaceId(5))),
        json!({ "FocusWorkspace": { "reference": { "Id": 5 } } })
    );
    assert_eq!(
        workspace_action(WorkspaceTarget::Index(3)),
        json!({ "FocusWorkspace": { "reference": { "Index": 3 } } })
    );
    assert_eq!(
        workspace_action(WorkspaceTarget::Name("chat".to_owned())),
        json!({ "FocusWorkspace": { "reference": { "Name": "chat" } } })
    );
    assert_eq!(
        workspace_action(WorkspaceTarget::Next),
        json!({ "FocusWorkspaceDown": {} })
    );
    assert_eq!(
        workspace_action(WorkspaceTarget::Prev),
        json!({ "FocusWorkspaceUp": {} })
    );

    assert_eq!(
        window_action(WindowTarget::Id(WindowId(9))),
        json!({ "FocusWindow": { "id": 9 } })
    );
    assert_eq!(
        window_action(WindowTarget::Next),
        json!({ "FocusWindowDownOrColumnRight": {} })
    );
    assert_eq!(
        window_action(WindowTarget::Prev),
        json!({ "FocusWindowUpOrColumnLeft": {} })
    );

    assert_eq!(
        rename_action(WorkspaceId(3), Some("chat")),
        json!({ "SetWorkspaceName": { "name": "chat", "workspace": { "Id": 3 } } })
    );
    assert_eq!(
        rename_action(WorkspaceId(3), None),
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
        power_off_monitors_action(),
        json!({ "PowerOffMonitors": {} })
    );

    assert_eq!(
        stop_cast_action(2),
        json!({ "StopCast": { "session_id": 2 } })
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

#[tokio::test]
async fn power_off_monitors_sends_the_action_envelope() {
    let received: Arc<Mutex<Option<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeNiri::spawn(move |request| {
        *sink.lock().expect("lock") = Some(request.to_owned());
        vec![json!({ "Ok": null }).to_string()]
    });

    Niri::at(&server.socket)
        .power_off_monitors()
        .await
        .expect("dispatched");

    assert_eq!(
        received.lock().expect("lock").as_deref(),
        Some(r#"{"Action":{"PowerOffMonitors":{}}}"#)
    );
}

#[tokio::test]
async fn stop_screencast_sends_the_session_id() {
    let received: Arc<Mutex<Option<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeNiri::spawn(move |request| {
        *sink.lock().expect("lock") = Some(request.to_owned());
        vec![json!({ "Ok": null }).to_string()]
    });

    Niri::at(&server.socket)
        .stop_screencast(2)
        .await
        .expect("dispatched");

    assert_eq!(
        received.lock().expect("lock").as_deref(),
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
        .screenshot_window(WindowId(9), &path)
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
        .screenshot_window(WindowId(9), &path)
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

    assert!(matches!(failure, CompositorError::NoFile(_)));
}

#[test]
fn a_close_write_watch_ignores_an_unrelated_file_closing_in_the_same_directory() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let watch = arm_close_write_watch(dir.path()).expect("watch armed");
    std::fs::write(dir.path().join("other.png"), b"decoy").expect("write a decoy file");

    let failure = wait_for_close_write(watch, OsStr::new("shot.png"), Duration::from_millis(30))
        .expect_err("the decoy must not satisfy the watch");

    assert!(matches!(failure, CompositorError::NoFile(_)));
}

#[tokio::test]
async fn a_relative_screenshot_path_is_rejected_before_anything_is_armed() {
    let failure = Niri::at("/nonexistent/niri.sock")
        .screenshot_window(WindowId(9), Path::new("shot.png"))
        .await
        .expect_err("a relative path must be refused");

    assert!(matches!(failure, CompositorError::InvalidPath(_)));
}

#[test]
fn a_live_chrome_cast_decodes_its_target_and_kind() {
    let wire: WireCast = serde_json::from_value(json!({
        "stream_id": 2, "session_id": 2, "kind": "PipeWire",
        "target": { "Output": { "name": "DP-2" } },
        "is_active": true, "pid": null, "pw_node_id": 107
    }))
    .expect("wire cast");

    let cast = wire.into_model();

    assert_eq!(cast.stream_id, 2);
    assert_eq!(cast.session_id, Some(2));
    assert_eq!(cast.kind, CastKind::PipeWire);
    assert_eq!(cast.target, CastTarget::Output("DP-2".to_owned()));
    assert_eq!(cast.pw_node_id, Some(107));
    assert!(cast.active);
}

#[test]
fn a_wlr_screencopy_cast_decodes_with_no_app_attribution() {
    let wire: WireCast = serde_json::from_value(json!({
        "stream_id": 4, "is_active": true, "kind": "WlrScreencopy",
    }))
    .expect("wire cast");

    let cast = wire.into_model();

    assert_eq!(cast.kind, CastKind::Screencopy);
    assert_eq!(cast.pw_node_id, None);
}

#[test]
fn a_cast_with_only_a_stream_id_decodes_as_unknown() {
    let wire: WireCast = serde_json::from_value(json!({ "stream_id": 9 })).expect("wire cast");

    let cast = wire.into_model();

    assert_eq!(cast.kind, CastKind::Unknown);
    assert_eq!(cast.target, CastTarget::Unknown);
    assert_eq!(cast.session_id, None);
    assert!(!cast.active);
}

#[test]
fn a_cast_kind_niri_added_later_decodes_as_unknown_instead_of_failing() {
    let wire: WireCast =
        serde_json::from_value(json!({ "stream_id": 1, "kind": "SomeFutureKind" }))
            .expect("wire cast");

    assert_eq!(wire.into_model().kind, CastKind::Unknown);
}

#[test]
fn a_cast_target_niri_added_later_decodes_as_unknown_instead_of_failing() {
    let wire: WireCast =
        serde_json::from_value(json!({ "stream_id": 1, "target": { "SomethingNew": {} } }))
            .expect("wire cast");

    assert_eq!(wire.into_model().target, CastTarget::Unknown);
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

/// A variant added by a future niri must not end the subscription.
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
async fn a_layout_switch_is_named_from_the_layouts_last_announced() {
    let events = events_from(vec![
        json!({ "KeyboardLayoutsChanged": {
            "keyboard_layouts": { "names": ["Polish", "Russian"], "current_idx": 0 } } }),
        json!({ "KeyboardLayoutSwitched": { "idx": 1 } }),
    ])
    .await;

    assert_eq!(
        events[1],
        Event::KeyboardLayoutSwitched {
            idx: 1,
            name: Some("Russian".to_owned()),
        }
    );
}

/// Niri emits no output event. A monitor leaving shows up as its workspaces being reassigned,
/// and without noticing that shift the daemon keeps a dead monitor in its list forever.
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
        matches!(events[1], Event::WorkspacesChanged(_)),
        "the first list is also delivered, got {:?}",
        events[1]
    );
    assert!(
        matches!(events[2], Event::WorkspacesChanged(_)),
        "an unchanged monitor set must not resync, got {:?}",
        events[2]
    );
    assert_eq!(events[3], Event::Resync(Resync::Outputs));
}

/// Three events `_old` never saw, each feeding a model field that would otherwise only ever
/// update on a full refresh.
#[tokio::test]
async fn urgency_and_layout_order_arrive_incrementally() {
    let events = events_from(vec![
        json!({ "WindowUrgencyChanged": { "id": 9, "urgent": true } }),
        json!({ "WorkspaceUrgencyChanged": { "id": 5, "urgent": false } }),
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
            Event::WorkspaceUrgencyChanged {
                id: WorkspaceId(5),
                urgent: false
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

    let cast = |stream_id, active| Cast {
        stream_id,
        session_id: None,
        kind: CastKind::Unknown,
        target: CastTarget::Unknown,
        pw_node_id: None,
        active,
    };

    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(3, true), cast(5, false)]),
            Event::CastStartedOrChanged {
                cast: cast(7, true)
            },
            Event::CastStopped(3),
        ]
    );
}

/// Niri reloads its own configuration in place, and `kb_layout` is the part of it this crate
/// caches — without the resync the panel shows a layout list the compositor has discarded.
#[tokio::test]
async fn a_successful_config_reload_asks_for_a_keyboard_resync() {
    let events = events_from(vec![
        json!({ "ConfigLoaded": { "failed": true } }),
        json!({ "ConfigLoaded": { "failed": false } }),
    ])
    .await;

    assert_eq!(events, [Event::Resync(Resync::Keyboard)]);
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
