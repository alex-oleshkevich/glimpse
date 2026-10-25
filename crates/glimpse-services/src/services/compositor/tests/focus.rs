use super::*;

fn windows(rig: &Rig) -> Vec<WindowInfo> {
    rig.mirror
        .handle
        .snapshot()
        .windows
        .expect("windows")
        .windows
}

#[tokio::test]
async fn a_window_that_opens_focused_takes_focus_from_whatever_had_it() {
    let mut rig = Rig::start(Snapshot {
        windows: vec![window(1, 4), window(2, 4)],
        focus: focus(None, Some(1)),
        ..Snapshot::default()
    })
    .await;

    rig.send(BackendEvent::WindowOpenedOrChanged {
        window: window(3, 4),
        focused: true,
    });
    rig.until("window 3 focused", |state| {
        state
            .windows
            .as_ref()
            .is_some_and(|shown| shown.windows.iter().any(|it| it.id == 3 && it.focused))
    })
    .await;

    assert_eq!(
        windows(&rig)
            .iter()
            .filter(|window| window.focused)
            .map(|window| window.id)
            .collect::<Vec<_>>(),
        [3],
        "a session focuses one window, so a popover listing them must never tick two rows"
    );
}

#[tokio::test]
async fn a_window_changing_without_focus_leaves_focus_where_it_is() {
    let mut rig = Rig::start(Snapshot {
        windows: vec![window(1, 4), window(2, 4)],
        focus: focus(None, Some(1)),
        ..Snapshot::default()
    })
    .await;
    let mut retitled = window(2, 4);
    retitled.title = Some("vim".to_owned());

    rig.send(BackendEvent::WindowOpenedOrChanged {
        window: retitled,
        focused: false,
    });
    rig.until("the retitle", |state| {
        state
            .windows
            .as_ref()
            .is_some_and(|shown| shown.windows.iter().any(|it| it.title.is_some()))
    })
    .await;

    let shown = windows(&rig);
    assert!(
        shown[0].focused,
        "a title arriving on an unfocused window must not move focus onto nothing"
    );
    assert!(!shown[1].focused);
}

#[tokio::test]
async fn focus_marks_only_its_window_its_output_and_the_active_workspace_there() {
    let mut on_dp1 = workspace(1, Some(1), "DP-1");
    on_dp1.active = true;
    let mut on_dp2 = workspace(2, Some(1), "DP-2");
    on_dp2.active = true;
    let rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true), output("DP-2", true)],
        workspaces: vec![on_dp1, on_dp2, workspace(3, Some(2), "DP-1")],
        windows: vec![window(6, 1), window(7, 1)],
        focus: focus(Some("DP-1"), Some(7)),
        ..Snapshot::default()
    })
    .await;

    let state = rig.mirror.handle.snapshot();
    let focused_windows: Vec<_> = windows(&rig)
        .into_iter()
        .filter(|window| window.focused)
        .map(|window| window.id)
        .collect();
    let focused_outputs: Vec<_> = state
        .outputs
        .expect("outputs")
        .outputs
        .into_iter()
        .filter(|output| output.focused)
        .map(|output| output.connector)
        .collect();
    let focused_workspaces: Vec<_> = state
        .workspaces
        .expect("workspaces")
        .workspaces
        .into_iter()
        .filter(|workspace| workspace.focused)
        .map(|workspace| workspace.id)
        .collect();

    assert_eq!(focused_windows, [7]);
    assert_eq!(focused_outputs, ["DP-1"]);
    assert_eq!(focused_workspaces, [1]);
}

#[tokio::test]
async fn a_window_is_focusable_by_the_pid_of_the_process_that_owns_it() {
    let mut owned = window(9, 1);
    owned.pid = Some(4265);
    let rig = Rig::start(snapshot_of(
        vec![workspace(1, Some(1), "DP-1")],
        vec![window(2, 1), owned],
    ))
    .await;

    let result = rig
        .mirror
        .handle
        .focus_window(WindowRef::Pid { pid: 4265 })
        .await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["focus_window 9"]);
}

#[tokio::test]
async fn a_process_owning_several_windows_always_resolves_to_the_same_one() {
    let mut later = window(9, 1);
    later.pid = Some(4265);
    let mut earlier = window(3, 1);
    earlier.pid = Some(4265);
    let rig = Rig::start(snapshot_of(
        vec![workspace(1, Some(1), "DP-1")],
        vec![later, earlier],
    ))
    .await;

    let result = rig
        .mirror
        .handle
        .focus_window(WindowRef::Pid { pid: 4265 })
        .await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["focus_window 3"]);
}

#[test]
fn an_application_whose_window_belongs_to_another_process_is_found_by_its_app_id() {
    let info = |id: u64, app_id: Option<&str>| WindowInfo {
        id,
        title: None,
        app_id: app_id.map(str::to_owned),
        workspace: None,
        focused: false,
        floating: false,
        urgent: false,
        order: None,
    };
    let windows = [
        info(22, Some("Chatgpt")),
        info(4, Some("org.telegram.desktop")),
        info(9, Some("org.telegram.desktop")),
        info(5, None),
    ];

    assert_eq!(app_window(&windows, &["chatgpt", "ChatGPT"]), Some(22));
    assert_eq!(
        app_window(&windows, &["org.telegram.desktop.desktop", "Telegram"]),
        Some(4)
    );
    assert_eq!(app_window(&windows, &["name:ChatGPT", ""]), None);
    assert_eq!(app_window(&windows, &[]), None);
}

#[tokio::test]
async fn a_pid_with_no_window_is_refused_and_a_plain_id_needs_no_window_list() {
    let rig = Rig::start(snapshot_of(
        vec![workspace(1, Some(1), "DP-1")],
        vec![window(2, 1)],
    ))
    .await;

    let by_pid = rig
        .mirror
        .handle
        .focus_window(WindowRef::Pid { pid: 4265 })
        .await;
    let by_id = rig
        .mirror
        .handle
        .focus_window(WindowRef::Id { id: 2 })
        .await;

    assert!(matches!(by_pid, Err(CommandError::InvalidArgument(_))));
    assert_eq!(by_id, Ok(()));
    assert_eq!(rig.calls(), ["focus_window 2"]);
}

#[tokio::test]
async fn focus_app_by_app_id_focuses_that_window() {
    let mut telegram = window(4, 1);
    telegram.app_id = Some("org.telegram.desktop".to_owned());
    let mut later = window(9, 1);
    later.app_id = Some("org.telegram.desktop".to_owned());
    let rig = Rig::start(snapshot_of(
        vec![workspace(1, Some(1), "DP-1")],
        vec![later, telegram],
    ))
    .await;

    let result = rig
        .mirror
        .handle
        .focus_app(None, &["org.telegram.desktop"])
        .await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["focus_window 4"]);
}

#[tokio::test]
async fn focus_app_falls_through_to_the_app_id_when_the_pid_matches_nothing() {
    let mut telegram = window(4, 1);
    telegram.app_id = Some("org.telegram.desktop".to_owned());
    let rig = Rig::start(snapshot_of(
        vec![workspace(1, Some(1), "DP-1")],
        vec![telegram],
    ))
    .await;

    let result = rig
        .mirror
        .handle
        .focus_app(Some(999), &["org.telegram.desktop"])
        .await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["focus_window 4"]);
}

#[tokio::test]
async fn focus_app_on_a_lost_provider_returns_unavailable() {
    let mut owned = window(3, 1);
    owned.pid = Some(4265);
    owned.app_id = Some("firefox".to_owned());
    let rig = Rig::start(snapshot_of(
        vec![workspace(1, Some(1), "DP-1")],
        vec![owned],
    ))
    .await;
    let mut health = rig.mirror.handle.health();
    until(&mut health, "running health", |health| {
        *health == ServiceState::Running
    })
    .await;

    rig.served.task.abort();
    until(&mut health, "degraded health", |health| {
        matches!(health, ServiceState::Degraded { .. })
    })
    .await;

    let result = rig.mirror.handle.focus_app(Some(4265), &["missing"]).await;

    assert!(
        matches!(result, Err(CommandError::Unavailable(_))),
        "{result:?}"
    );
}
