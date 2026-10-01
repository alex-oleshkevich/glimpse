use super::*;

#[test]
fn a_workspace_is_urgent_when_a_window_on_it_is() {
    let mut urgent = window(1, 7);
    urgent.is_urgent = true;
    let state = Snapshot {
        workspaces: vec![workspace(7, Some(1), "DP-1"), workspace(8, Some(2), "DP-1")],
        windows: vec![urgent, window(2, 8)],
        ..Snapshot::default()
    };

    let published = workspaces_of(&state);

    assert!(
        published[0].urgent,
        "Hyprland never marks a workspace urgent, so deriving it from the windows is the \
             only thing that makes it work there at all"
    );
    assert!(!published[1].urgent);
}

#[test]
fn focusing_a_window_clears_the_urgency_hyprland_never_clears() {
    let mut urgent = window(1, 7);
    urgent.is_urgent = true;
    let mut state = Snapshot {
        workspaces: vec![workspace(7, Some(1), "DP-1")],
        windows: vec![urgent],
        ..Snapshot::default()
    };

    apply(&mut state, Change::WindowFocusChanged(Some(WindowId(1))));

    assert!(!state.windows[0].is_urgent);
    assert!(state.windows[0].is_focused);
    assert!(
        !workspaces_of(&state)[0].urgent,
        "the workspace stops being urgent with the window, since it is derived rather than \
             cached"
    );
}

#[test]
fn activating_a_workspace_leaves_the_other_output_alone() {
    let mut here = workspace(1, Some(1), "DP-1");
    here.is_active = true;
    let mut there = workspace(9, Some(1), "HDMI-A-1");
    there.is_active = true;
    let mut state = Snapshot {
        workspaces: vec![here, workspace(2, Some(2), "DP-1"), there],
        ..Snapshot::default()
    };

    apply(
        &mut state,
        Change::WorkspaceActivated {
            id: WorkspaceId(2),
            focused: true,
        },
    );

    assert!(!state.workspaces[0].is_active);
    assert!(state.workspaces[1].is_active);
    assert!(
        state.workspaces[2].is_active,
        "each output has its own current workspace; activating one must not clear the other"
    );
    assert!(state.workspaces[1].is_focused);
    assert!(!state.workspaces[2].is_focused);
    assert_eq!(state.focused_output.as_deref(), Some("DP-1"));
}

#[test]
fn workspaces_are_ordered_by_output_then_by_position() {
    let state = Snapshot {
        workspaces: vec![
            workspace(30, Some(2), "HDMI-A-1"),
            workspace(20, Some(2), "DP-1"),
            workspace(10, Some(1), "DP-1"),
            workspace(40, Some(1), "HDMI-A-1"),
        ],
        ..Snapshot::default()
    };

    let published = workspaces_of(&state);

    assert_eq!(
        published.iter().map(|it| it.id).collect::<Vec<_>>(),
        [10, 20, 40, 30]
    );
}

#[test]
fn a_workspace_without_an_index_is_ordered_by_its_id() {
    let state = Snapshot {
        workspaces: vec![workspace(9, None, "DP-1"), workspace(3, None, "DP-1")],
        ..Snapshot::default()
    };

    assert_eq!(
        workspaces_of(&state)
            .iter()
            .map(|it| it.id)
            .collect::<Vec<_>>(),
        [3, 9],
        "Hyprland fills no idx, and its id is the ordering"
    );
}

#[test]
fn an_activation_naming_a_workspace_we_have_not_seen_asks_for_a_resync() {
    let mut focused = workspace(1, Some(1), "DP-1");
    focused.is_focused = true;
    focused.is_active = true;
    let mut state = Snapshot {
        workspaces: vec![focused],
        ..Snapshot::default()
    };

    assert!(
        apply(
            &mut state,
            Change::WorkspaceActivated {
                id: WorkspaceId(99),
                focused: true,
            }
        ),
        "the list is behind the compositor, and re-reading it is the only way to catch up"
    );
    assert!(
        state.workspaces[0].is_focused,
        "clearing focus everywhere and setting it nowhere would leave the strip with no \
             current workspace at all"
    );
}

#[test]
fn a_structural_resync_asks_for_a_new_snapshot_and_a_keyboard_one_does_not() {
    let mut state = Snapshot::default();

    assert!(apply(&mut state, Change::Resync(Resync::Structure)));
    assert!(apply(&mut state, Change::Resync(Resync::Outputs)));
    assert!(
        !apply(&mut state, Change::Resync(Resync::Keyboard)),
        "the keyboard belongs to another service, and re-reading the whole snapshot for it \
             would be a round trip per layout switch"
    );
}
