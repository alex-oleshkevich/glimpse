use super::*;

fn published(rig: &Rig) -> Vec<WorkspaceInfo> {
    rig.mirror
        .handle
        .snapshot()
        .workspaces
        .expect("workspaces")
        .workspaces
}

#[tokio::test]
async fn a_workspace_is_urgent_when_a_window_on_it_is() {
    let mut urgent = window(1, 7);
    urgent.urgent = true;
    let rig = Rig::start(snapshot_of(
        vec![workspace(7, Some(1), "DP-1"), workspace(8, Some(2), "DP-1")],
        vec![urgent, window(2, 8)],
    ))
    .await;

    let published = published(&rig);

    assert!(
        published[0].urgent,
        "the provider carries no workspace urgency, so deriving it from the windows is the \
             only thing that makes it work at all"
    );
    assert!(!published[1].urgent);
}

#[tokio::test]
async fn focusing_a_window_clears_the_urgency_the_compositor_never_clears() {
    let mut urgent = window(1, 7);
    urgent.urgent = true;
    let mut rig = Rig::start(snapshot_of(
        vec![workspace(7, Some(1), "DP-1")],
        vec![urgent],
    ))
    .await;

    rig.send(BackendEvent::WindowFocusChanged(Some(WindowId(1))));
    rig.until("the window focused", |state| {
        state
            .windows
            .as_ref()
            .is_some_and(|windows| windows.windows[0].focused)
    })
    .await;

    let windows = rig
        .mirror
        .handle
        .snapshot()
        .windows
        .expect("windows")
        .windows;
    assert!(!windows[0].urgent);
    assert!(windows[0].focused);
    assert!(
        !published(&rig)[0].urgent,
        "the workspace stops being urgent with the window, since it is derived rather than \
             cached"
    );
}

#[tokio::test]
async fn activating_a_workspace_leaves_the_other_output_alone() {
    let mut here = workspace(1, Some(1), "DP-1");
    here.active = true;
    let mut there = workspace(9, Some(1), "HDMI-A-1");
    there.active = true;
    let mut rig = Rig::start(snapshot_of(
        vec![here, workspace(2, Some(2), "DP-1"), there],
        Vec::new(),
    ))
    .await;

    rig.send(BackendEvent::WorkspaceActivated {
        id: WorkspaceId(2),
        focused: true,
    });
    rig.until("workspace 2 active", |state| {
        state
            .workspaces
            .as_ref()
            .is_some_and(|shown| shown.workspaces.iter().any(|it| it.id == 2 && it.focused))
    })
    .await;

    let shown = published(&rig);
    let by_id = |id: u64| shown.iter().find(|it| it.id == id).expect("workspace");
    assert!(!by_id(1).active);
    assert!(by_id(2).active);
    assert!(
        by_id(9).active,
        "each output has its own current workspace; activating one must not clear the other"
    );
    assert!(by_id(2).focused);
    assert!(!by_id(9).focused);
}

#[tokio::test]
async fn workspaces_are_ordered_by_output_then_by_position() {
    let rig = Rig::start(snapshot_of(
        vec![
            workspace(30, Some(2), "HDMI-A-1"),
            workspace(20, Some(2), "DP-1"),
            workspace(10, Some(1), "DP-1"),
            workspace(40, Some(1), "HDMI-A-1"),
        ],
        Vec::new(),
    ))
    .await;

    assert_eq!(
        published(&rig).iter().map(|it| it.id).collect::<Vec<_>>(),
        [10, 20, 40, 30]
    );
}

#[tokio::test]
async fn a_workspace_without_an_index_is_ordered_by_its_id() {
    let rig = Rig::start(snapshot_of(
        vec![workspace(9, None, "DP-1"), workspace(3, None, "DP-1")],
        Vec::new(),
    ))
    .await;

    assert_eq!(
        published(&rig).iter().map(|it| it.id).collect::<Vec<_>>(),
        [3, 9],
        "Hyprland fills no idx, and its id is the ordering"
    );
}

#[tokio::test]
async fn an_activation_naming_a_workspace_we_have_not_seen_keeps_the_focused_one() {
    let mut here = workspace(1, Some(1), "DP-1");
    here.active = true;
    let mut elsewhere = workspace(2, Some(1), "DP-2");
    elsewhere.active = true;
    let mut snapshot = snapshot_of(vec![here, elsewhere], Vec::new());
    snapshot.focus = focus(Some("DP-2"), None);
    let mut rig = Rig::start(snapshot).await;
    let before = rig.snapshots();

    rig.send(BackendEvent::WorkspaceActivated {
        id: WorkspaceId(99),
        focused: true,
    });
    rig.send(BackendEvent::CastsChanged(vec![cast(1, true)]));
    rig.until("the marker cast", |state| {
        state.privacy.as_ref().is_some_and(|privacy| privacy.active)
    })
    .await;

    assert_eq!(rig.snapshots(), before + 1);
    let shown = published(&rig);
    assert!(
        shown
            .iter()
            .any(|workspace| workspace.id == 2 && workspace.focused)
    );
    assert!(
        shown
            .iter()
            .any(|workspace| workspace.id == 1 && !workspace.focused)
    );
}
