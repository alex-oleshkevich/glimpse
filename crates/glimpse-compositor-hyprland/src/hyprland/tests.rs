use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use glimpse_compositors::event::Resync;
use glimpse_compositors::model::Cast;
use serde_json::json;

use super::event::GRACE;
use super::testing::FakeHyprland;
use super::*;

fn snapshot_server() -> FakeHyprland {
    snapshot_server_with(Vec::new())
}

fn snapshot_server_with(events: Vec<String>) -> FakeHyprland {
    FakeHyprland::spawn(snapshot_reply, events)
}

fn snapshot_reply(command: &str) -> String {
    match command {
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
              "workspace": { "id": -1337, "name": "chat" }, "floating": true,
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
    }
}

fn empty_server(clients: serde_json::Value) -> FakeHyprland {
    FakeHyprland::spawn(
        move |command| match command {
            "j/clients" => clients.to_string(),
            "j/monitors all" | "j/workspaces" => "[]".to_owned(),
            "j/activewindow" => "{}".to_owned(),
            "j/devices" => json!({ "keyboards": [] }).to_string(),
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

    assert_eq!(snapshot.focus.output.as_deref(), Some("eDP-1"));
    assert_eq!(snapshot.focus.window, Some(WindowId(9)));
    assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
    assert_eq!(snapshot.windows[0].workspace, Some(WorkspaceId(5)));
    assert_eq!(snapshot.windows[0].order, None);
    assert!(!snapshot.windows[0].floating && snapshot.windows[1].floating);
    assert_eq!(snapshot.workspaces[0].name, None);
    assert_eq!(snapshot.workspaces[0].index, None);
    assert!(snapshot.workspaces[0].active);
    assert!(
        snapshot.workspaces[1].active,
        "chat is the active workspace of the unfocused DP-3"
    );
    assert_eq!(
        snapshot.workspaces.len(),
        2,
        "the special workspace is not an ordinary pager workspace"
    );
    assert_eq!(snapshot.workspaces[1].id, WorkspaceId((-1337_i64) as u64));
    assert_eq!(snapshot.workspaces[1].name.as_deref(), Some("chat"));
    assert_eq!(
        snapshot.windows[1].workspace,
        Some(WorkspaceId((-1337_i64) as u64))
    );
}

#[test]
fn a_workspace_named_after_its_id_is_unnamed_and_any_other_name_is_kept() {
    let numbered: WireWorkspace =
        serde_json::from_value(json!({"id": 5, "name": "5"})).expect("workspace");
    let named: WireWorkspace =
        serde_json::from_value(json!({"id": 5, "name": "web"})).expect("workspace");

    assert_eq!(numbered.into_model(&[]).name, None);
    assert_eq!(named.into_model(&[]).name.as_deref(), Some("web"));
}

#[tokio::test]
async fn an_empty_title_is_absent_rather_than_an_empty_string() {
    let server = empty_server(json!([{ "address": "0x1", "title": "", "class": "" }]));

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

    assert!(built_in.enabled);
    assert_eq!(
        built_in.mode,
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
    assert_eq!(external.mode, None);
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
        .step_window(true)
        .await
        .expect_err("refused");

    assert!(matches!(failure, Error::Refused(reason) if reason.contains("Invalid")));
}

#[tokio::test]
async fn an_empty_reply_is_a_closed_connection() {
    let server = FakeHyprland::spawn(|_| String::new(), Vec::new());

    let failure = Hyprland::at(&server.dir)
        .step_window(true)
        .await
        .expect_err("closed");

    assert!(matches!(failure, Error::Unavailable(reason) if reason.contains("closed")));
}

#[tokio::test]
async fn a_missing_socket_names_the_path_it_tried() {
    let failure = Hyprland::at("/nonexistent/hypr")
        .step_window(true)
        .await
        .expect_err("no socket");

    assert!(
        matches!(&failure, Error::Unavailable(reason)
            if reason.contains("/nonexistent/hypr/.socket.sock")),
        "expected the path in {failure:?}"
    );
}

#[tokio::test]
async fn what_hyprland_cannot_do_is_unsupported_before_anything_is_sent() {
    let hyprland = Hyprland::at("/nonexistent");

    assert!(matches!(
        hyprland.stop_screencast(2).await,
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        hyprland
            .screenshot_window(WindowId(9), PathBuf::from("/tmp/shot.png"))
            .await,
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        hyprland.power_off_monitors().await,
        Err(Error::Unsupported(_))
    ));
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
            "j/clients" => json!([
                { "address": "0x9", "title": "a terminal", "class": "foot",
                  "monitor": 0, "at": [150, 90], "size": [800, 600] },
                { "address": "0xa", "title": "orphan", "class": "foot",
                  "monitor": 7, "at": [150, 90], "size": [800, 600] },
                { "address": "0xb", "title": "unplaced", "class": "foot", "monitor": 0 }
            ])
            .to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );

    let geometries = Hyprland::at(&server.dir)
        .window_geometries()
        .await
        .expect("geometries");

    assert_eq!(
        geometries.get(&WindowId(9)),
        Some(&Rect {
            x: 50.0,
            y: 40.0,
            width: 800.0,
            height: 600.0
        }),
        "at is global layout coordinates; rect must be relative to the window's own monitor"
    );
    assert_eq!(
        geometries.get(&WindowId(0xa)),
        None,
        "a window whose monitor is unknown has no rect rather than a global one"
    );
    assert_eq!(geometries.get(&WindowId(0xb)), None);
}

#[tokio::test]
async fn a_layout_switch_is_a_request_rather_than_a_dispatch() {
    let received: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeHyprland::spawn(
        move |command| {
            sink.lock().expect("lock").push(command.to_owned());
            "ok".to_owned()
        },
        Vec::new(),
    );
    let hyprland = Hyprland::at(&server.dir);

    hyprland.step_keyboard_layout(true).await.expect("next");
    hyprland.step_keyboard_layout(false).await.expect("prev");
    hyprland.switch_keyboard_layout(2).await.expect("index");

    assert_eq!(
        *received.lock().expect("lock"),
        [
            "switchxkblayout all next",
            "switchxkblayout all prev",
            "switchxkblayout all 2"
        ],
        "switchxkblayout is a top-level request; a dispatch of it is an invalid dispatcher"
    );
}

fn event_reply(command: &str) -> String {
    match command {
        "j/devices" => json!({ "keyboards": [
            { "main": true, "layout": "us,gb,us", "variant": ",,intl",
              "active_layout_index": 1, "active_keymap": "English (UK)" }
        ] })
        .to_string(),
        other => panic!("the fake was asked for {other}"),
    }
}

fn owned(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

async fn events_from(lines: &[&str]) -> Vec<Event> {
    let server = FakeHyprland::spawn(event_reply, owned(lines));

    Hyprland::at(&server.dir)
        .events()
        .await
        .expect("subscribed")
        .collect()
        .await
}

async fn timed_events(lines: &[&str], gap: Duration) -> Vec<(u128, Event)> {
    let server = FakeHyprland::spawn_live(event_reply, owned(lines), gap);
    let start = tokio::time::Instant::now();
    let mut stream = Hyprland::at(&server.dir)
        .events()
        .await
        .expect("subscribed");
    let mut events = Vec::new();
    while let Ok(Some(event)) =
        tokio::time::timeout_at(start + Duration::from_secs(30), stream.next()).await
    {
        events.push(((start.elapsed().as_millis() + 50) / 100 * 100, event));
    }
    events
}

async fn grace_events(lines: &[&str]) -> Vec<Event> {
    timed_events(lines, Duration::ZERO)
        .await
        .into_iter()
        .map(|(_, event)| event)
        .collect()
}

async fn next_within_grace(stream: &mut BoxStream<'static, Event>) -> Option<Event> {
    tokio::time::timeout(GRACE * 2, stream.next())
        .await
        .expect("an event within twice the grace")
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

fn cast(id: u64, output: Option<&str>) -> Cast {
    Cast {
        id,
        active: true,
        output: output.map(ToOwned::to_owned),
        session: None,
        pw_node: None,
    }
}

#[tokio::test(start_paused = true)]
async fn a_screencast_start_then_stop_leaves_one_cast_then_none() {
    let events = grace_events(&[
        "screencastv2>>1,monitor,DP-2",
        "screencastv2>>0,monitor,DP-2",
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(0, Some("DP-2"))]),
            Event::CastsChanged(Vec::new()),
        ]
    );
}

#[tokio::test]
async fn a_window_share_names_no_output() {
    let events = events_from(&["screencastv2>>1,window,my secret, document"]).await;

    assert_eq!(events, [Event::CastsChanged(vec![cast(0, None)])]);
}

#[tokio::test]
async fn a_region_share_names_its_monitor() {
    let events = events_from(&["screencastv2>>1,region,eDP-1"]).await;

    assert_eq!(events, [Event::CastsChanged(vec![cast(0, Some("eDP-1"))])]);
}

#[tokio::test(start_paused = true)]
async fn the_captured_wf_recorder_region_pair_names_virtual_1() {
    let events = grace_events(&[
        "screencastv2>>1,region,Virtual-1",
        "screencastv2>>0,region,Virtual-1",
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(0, Some("Virtual-1"))]),
            Event::CastsChanged(Vec::new()),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn one_stop_ends_only_the_share_it_names() {
    let events = grace_events(&[
        "screencastv2>>1,monitor,DP-2",
        "screencastv2>>1,window,a title",
        "screencastv2>>0,window,a title",
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(0, Some("DP-2"))]),
            Event::CastsChanged(vec![cast(0, Some("DP-2")), cast(1, None)]),
            Event::CastsChanged(vec![cast(0, Some("DP-2"))]),
        ]
    );
}

#[tokio::test]
async fn the_v1_screencast_event_is_not_counted_beside_v2() {
    let events = events_from(&["screencast>>1,1", "screencastv2>>1,monitor,DP-2"]).await;

    assert_eq!(events, [Event::CastsChanged(vec![cast(0, Some("DP-2"))])]);
}

#[tokio::test]
async fn a_malformed_screencast_line_changes_nothing() {
    let events = events_from(&[
        "screencastv2>>1,monitor",
        "screencastv2>>7,monitor,DP-2",
        "screencastv2>>1,screen,DP-2",
    ])
    .await;

    assert!(events.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_stop_with_a_changed_name_removes_the_oldest_share_of_its_type() {
    let events = grace_events(&[
        "screencastv2>>1,window,old title",
        "screencastv2>>1,window,other",
        "screencastv2>>0,window,new title",
    ])
    .await;

    assert_eq!(
        events.last(),
        Some(&Event::CastsChanged(vec![cast(1, None)]))
    );

    let events = grace_events(&[
        "screencastv2>>1,window,old title",
        "screencastv2>>0,window,new title",
    ])
    .await;
    assert_eq!(events.last(), Some(&Event::CastsChanged(Vec::new())));
}

#[tokio::test(start_paused = true)]
async fn a_stop_with_an_exact_match_wins_over_the_oldest_share_of_its_type() {
    let events = grace_events(&[
        "screencastv2>>1,monitor,DP-2",
        "screencastv2>>1,monitor,eDP-1",
        "screencastv2>>0,monitor,eDP-1",
    ])
    .await;

    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(0, Some("DP-2"))]),
            Event::CastsChanged(vec![cast(0, Some("DP-2")), cast(1, Some("eDP-1"))]),
            Event::CastsChanged(vec![cast(0, Some("DP-2"))]),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_restart_within_grace_keeps_the_same_cast_without_an_event() {
    let events = timed_events(
        &[
            "screencastv2>>1,monitor,DP-2",
            "screencastv2>>0,monitor,DP-2",
            "screencastv2>>1,monitor,DP-2",
        ],
        Duration::from_millis(600),
    )
    .await;

    assert_eq!(
        events,
        [(600, Event::CastsChanged(vec![cast(0, Some("DP-2"))]))]
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_never_picks_a_share_that_is_already_stopping() {
    let events = grace_events(&[
        "screencastv2>>1,monitor,DP-2",
        "screencastv2>>1,monitor,DP-2",
        "screencastv2>>0,monitor,DP-2",
        "screencastv2>>0,monitor,DP-2",
    ])
    .await;
    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(0, Some("DP-2"))]),
            Event::CastsChanged(vec![cast(0, Some("DP-2")), cast(1, Some("DP-2"))]),
            Event::CastsChanged(Vec::new()),
        ]
    );

    let events = grace_events(&[
        "screencastv2>>1,window,a",
        "screencastv2>>0,window,a",
        "screencastv2>>1,window,b",
        "screencastv2>>0,window,c",
    ])
    .await;
    assert_eq!(
        events,
        [
            Event::CastsChanged(vec![cast(0, None)]),
            Event::CastsChanged(vec![cast(0, None), cast(1, None)]),
            Event::CastsChanged(Vec::new()),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_with_no_share_of_its_type_returns_no_event() {
    let events = timed_events(
        &[
            "screencastv2>>1,monitor,DP-2",
            "screencastv2>>0,window,a title",
        ],
        Duration::ZERO,
    )
    .await;

    assert_eq!(
        events,
        [(0, Event::CastsChanged(vec![cast(0, Some("DP-2"))]))]
    );
}

#[tokio::test(start_paused = true)]
async fn flapping_pairs_publish_once_at_start_and_once_after_grace() {
    let mut lines = Vec::new();
    for _ in 0..13 {
        lines.push("screencastv2>>1,monitor,DP-2");
        lines.push("screencastv2>>0,monitor,DP-2");
    }

    let events = timed_events(&lines, Duration::from_millis(600)).await;

    assert_eq!(
        events,
        [
            (600, Event::CastsChanged(vec![cast(0, Some("DP-2"))])),
            (15_600 + GRACE.as_millis(), Event::CastsChanged(Vec::new())),
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
    let snapshot = snapshot_after(&["screencastv2>>1,monitor,eDP-1", "workspacev2>>5,5"]).await;

    assert_eq!(snapshot.casts, [cast(0, Some("eDP-1"))]);
}

#[tokio::test(start_paused = true)]
async fn a_snapshot_lists_a_stopping_cast_until_grace_expires() {
    let server = FakeHyprland::spawn_live(
        snapshot_reply,
        owned(&[
            "screencastv2>>1,monitor,eDP-1",
            "screencastv2>>0,monitor,eDP-1",
        ]),
        Duration::ZERO,
    );
    let hyprland = Hyprland::at(&server.dir);
    let mut stream = hyprland.events().await.expect("subscribed");

    assert_eq!(
        next_within_grace(&mut stream).await,
        Some(Event::CastsChanged(vec![cast(0, Some("eDP-1"))]))
    );
    assert!(
        tokio::time::timeout(GRACE / 2, stream.next())
            .await
            .is_err()
    );

    let snapshot = hyprland.snapshot().await.expect("snapshot");
    assert_eq!(snapshot.casts, [cast(0, Some("eDP-1"))]);

    assert_eq!(
        next_within_grace(&mut stream).await,
        Some(Event::CastsChanged(Vec::new()))
    );
    let snapshot = hyprland.snapshot().await.expect("snapshot");
    assert!(snapshot.casts.is_empty());
}

#[tokio::test]
async fn an_urgent_window_stays_urgent_across_a_resync_until_it_is_focused() {
    let snapshot = snapshot_after(&["urgent>>0x9", "windowtitlev2>>9,a title"]).await;
    assert!(snapshot.windows[0].urgent);
    assert!(!snapshot.windows[1].urgent);

    let snapshot = snapshot_after(&["urgent>>0x9", "activewindowv2>>9"]).await;
    assert!(!snapshot.windows[0].urgent);

    let snapshot = snapshot_after(&["urgent>>0x9", "closewindow>>0x9"]).await;
    assert!(!snapshot.windows[0].urgent);
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
    let server = empty_server(json!([{ "address": "0x1",
                                       "title": "a, b, c\u{2068}spoof\u{2069}",
                                       "class": "foot" }]));

    let snapshot = Hyprland::at(&server.dir)
        .snapshot()
        .await
        .expect("snapshot");

    assert_eq!(snapshot.windows[0].title.as_deref(), Some("a, b, cspoof"));
    assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
}

fn dispatch_server(lua: bool) -> (FakeHyprland, Arc<Mutex<Vec<String>>>) {
    let received: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = received.clone();
    let respond = move |command: &str| {
        sink.lock().expect("lock").push(command.to_owned());
        match command {
            "j/workspaces" => json!([
                { "id": -1337, "name": "chat", "monitor": "eDP-1", "lastwindow": "0x0" }
            ])
            .to_string(),
            _ => "ok".to_owned(),
        }
    };
    let server = match lua {
        true => FakeHyprland::spawn_lua(respond, Vec::new()),
        false => FakeHyprland::spawn(respond, Vec::new()),
    };
    (server, received)
}

async fn drive_every_dispatch(hyprland: &Hyprland) {
    hyprland.focus_workspace(WorkspaceId(2)).await.expect("id");
    hyprland.step_workspace(true).await.expect("next");
    hyprland.step_workspace(false).await.expect("prev");
    hyprland
        .focus_workspace(WorkspaceId((-1337_i64) as u64))
        .await
        .expect("negative id");
    hyprland
        .rename_workspace(WorkspaceId(3), "mail".to_owned())
        .await
        .expect("rename");
    hyprland
        .rename_workspace(WorkspaceId(3), String::new())
        .await
        .expect("clear");
    hyprland
        .rename_workspace(WorkspaceId(3), "a \"b\"\nc\\d".to_owned())
        .await
        .expect("hostile name");
    hyprland
        .focus_window(WindowId(0x5591_e8b2_f5a0))
        .await
        .expect("address");
    hyprland.step_window(true).await.expect("cycle next");
    hyprland.step_window(false).await.expect("cycle prev");
}

#[tokio::test]
async fn a_legacy_config_gets_the_legacy_dispatchers() {
    let (server, received) = dispatch_server(false);

    drive_every_dispatch(&Hyprland::at(&server.dir)).await;

    assert_eq!(
        *received.lock().expect("lock"),
        [
            "dispatch workspace 2",
            "dispatch workspace e+1",
            "dispatch workspace e-1",
            "j/workspaces",
            "dispatch workspace name:chat",
            "dispatch renameworkspace 3 mail",
            "dispatch renameworkspace 3",
            "dispatch renameworkspace 3 a \"b\"\nc\\d",
            "dispatch focuswindow address:0x5591e8b2f5a0",
            "dispatch cyclenext",
            "dispatch cyclenext prev",
        ]
    );
}

#[tokio::test]
async fn a_lua_config_gets_the_hl_dsp_forms() {
    let (server, received) = dispatch_server(true);

    drive_every_dispatch(&Hyprland::at(&server.dir)).await;

    assert_eq!(
        *received.lock().expect("lock"),
        [
            "dispatch hl.dsp.focus({ workspace = \"2\" })",
            "dispatch hl.dsp.focus({ workspace = \"e+1\" })",
            "dispatch hl.dsp.focus({ workspace = \"e-1\" })",
            "j/workspaces",
            "dispatch hl.dsp.focus({ workspace = \"name:chat\" })",
            "dispatch hl.dsp.workspace.rename({ workspace = \"3\", name = \"mail\" })",
            "dispatch hl.dsp.workspace.rename({ workspace = \"3\" })",
            "dispatch hl.dsp.workspace.rename({ workspace = \"3\", name = \"a \\\"b\\\"\\nc\\\\d\" })",
            "dispatch hl.dsp.focus({ window = \"address:0x5591e8b2f5a0\" })",
            "dispatch hl.dsp.window.cycle_next()",
            "dispatch hl.dsp.window.cycle_next({ next = false })",
        ]
    );
}

#[tokio::test]
async fn a_lua_refusal_carries_hyprland_s_text() {
    let server = FakeHyprland::spawn_lua(|_| "error: no such workspace".to_owned(), Vec::new());

    let failure = Hyprland::at(&server.dir)
        .focus_workspace(WorkspaceId(2))
        .await
        .expect_err("refused");

    assert!(matches!(failure, Error::Refused(reason) if reason.contains("no such")));
}

fn monitor_server(reply: &'static str) -> (FakeHyprland, Arc<Mutex<Vec<String>>>) {
    let received: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = received.clone();
    let server = FakeHyprland::spawn_lua(
        move |command| {
            sink.lock().expect("lock").push(command.to_owned());
            reply.to_owned()
        },
        Vec::new(),
    );
    (server, received)
}

#[tokio::test]
async fn a_lua_monitor_disable_changes_only_the_disabled_field() {
    let (server, received) = monitor_server("ok");

    Hyprland::at(&server.dir)
        .set_output_enabled("DP-3".to_owned(), false)
        .await
        .expect("disabled through eval");

    assert_eq!(
        *received.lock().expect("lock"),
        ["eval hl.monitor({ output = \"DP-3\", disabled = true })"]
    );
}

#[tokio::test]
async fn a_legacy_session_without_its_wayland_socket_never_overwrites_a_monitor_rule() {
    let server = FakeHyprland::spawn(
        |command| panic!("must not change monitor rules: {command}"),
        Vec::new(),
    );

    let outcome = Hyprland::at(&server.dir)
        .set_output_enabled("DP-3".to_owned(), false)
        .await;

    assert!(matches!(outcome, Err(Error::Failed(_))));
}

#[tokio::test]
async fn a_refused_disable_is_an_error() {
    let (server, _received) = monitor_server("invalid monitor syntax");

    let outcome = Hyprland::at(&server.dir)
        .set_output_enabled("DP-3".to_owned(), false)
        .await;

    assert!(matches!(outcome, Err(Error::Refused(_))));
}

#[tokio::test]
async fn enabling_a_monitor_targets_only_its_connector() {
    let (server, received) = monitor_server("ok");

    Hyprland::at(&server.dir)
        .set_output_enabled("DP-3".to_owned(), true)
        .await
        .expect("enabled");

    assert_eq!(
        *received.lock().expect("lock"),
        ["eval hl.monitor({ output = \"DP-3\", disabled = false })"]
    );
}

#[tokio::test]
async fn a_refused_enable_carries_hyprland_s_text() {
    let (server, _received) = monitor_server("config error: bad rule");

    let outcome = Hyprland::at(&server.dir)
        .set_output_enabled("DP-3".to_owned(), true)
        .await;

    assert!(matches!(
        outcome,
        Err(Error::Refused(reason)) if reason == "config error: bad rule"
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

async fn session_lock_restore(value: Value) -> Option<bool> {
    let server = FakeHyprland::spawn(
        move |command| match command {
            "j/getoption misc:allow_session_lock_restore" => value.to_string(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );
    Hyprland::at(&server.dir)
        .session_lock_restore()
        .await
        .expect("an answer")
}

#[tokio::test]
async fn the_session_lock_restore_option_reads_its_bool() {
    assert_eq!(
        session_lock_restore(json!({"bool": false})).await,
        Some(false)
    );
    assert_eq!(
        session_lock_restore(json!({"bool": true})).await,
        Some(true)
    );
}

#[tokio::test]
async fn the_session_lock_restore_option_reads_its_int() {
    assert_eq!(session_lock_restore(json!({"int": 0})).await, Some(false));
    assert_eq!(session_lock_restore(json!({"int": 1})).await, Some(true));
}

#[tokio::test]
async fn an_undecodable_lock_restore_reply_is_unknown_not_an_error() {
    let server = FakeHyprland::spawn(|_| "no such option".to_owned(), Vec::new());

    assert_eq!(
        Hyprland::at(&server.dir).session_lock_restore().await,
        Ok(None)
    );
    assert!(matches!(
        Hyprland::at("/nonexistent").session_lock_restore().await,
        Err(Error::Unavailable(_))
    ));
}

#[tokio::test]
async fn the_probe_is_true_only_when_the_version_request_is_answered() {
    let server = FakeHyprland::spawn(
        |command| match command {
            "version" => "Hyprland 0.56.2".to_owned(),
            other => panic!("the fake was asked for {other}"),
        },
        Vec::new(),
    );
    assert!(Hyprland::at(&server.dir).probe().await);

    let silent = FakeHyprland::spawn(|_| String::new(), Vec::new());
    assert!(!Hyprland::at(&silent.dir).probe().await);
}

#[tokio::test]
async fn the_probe_is_false_when_a_socket_file_exists_but_nothing_listens() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    drop(std::os::unix::net::UnixListener::bind(dir.path().join(CONTROL_SOCKET)).expect("bind"));

    assert!(!Hyprland::at(dir.path()).probe().await);
}

#[tokio::test(start_paused = true)]
async fn the_probe_gives_up_on_a_listener_that_never_replies() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let listener = tokio::net::UnixListener::bind(dir.path().join(CONTROL_SOCKET)).expect("bind");
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
        }
    });

    let start = tokio::time::Instant::now();
    let probe = tokio::time::timeout(PROBE_DEADLINE * 2, Hyprland::at(dir.path()).probe()).await;

    assert!(
        probe.is_ok(),
        "the probe did not give up within {PROBE_DEADLINE:?}"
    );
    assert!(
        !probe.expect("the probe already finished"),
        "a silent listener must report no Hyprland"
    );
    assert_eq!(start.elapsed(), PROBE_DEADLINE);
}
