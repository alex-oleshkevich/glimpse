use super::*;
use crate::model::{Cast, Window, WindowId, Workspace, WorkspaceId};

fn workspace(id: u64, output: &str, active: bool) -> Workspace {
    Workspace {
        id: WorkspaceId(id),
        index: None,
        name: None,
        output: Some(output.to_owned()),
        active,
    }
}

fn window(id: u64, urgent: bool) -> Window {
    Window {
        id: WindowId(id),
        title: None,
        app_id: None,
        pid: None,
        workspace: None,
        floating: false,
        urgent,
        order: None,
    }
}

fn state() -> Snapshot {
    Snapshot {
        workspaces: vec![
            workspace(1, "DP-1", true),
            workspace(2, "DP-1", false),
            workspace(3, "eDP-1", true),
        ],
        windows: vec![window(1, false), window(2, true)],
        ..Snapshot::default()
    }
}

#[test]
fn a_focused_activation_moves_the_focused_output_and_the_active_workspace_of_that_output_only() {
    let mut state = state();

    let resync = apply(
        &mut state,
        Event::WorkspaceActivated {
            id: WorkspaceId(2),
            focused: true,
        },
    );

    assert_eq!(resync, None);
    assert_eq!(state.focus.output.as_deref(), Some("DP-1"));
    let active: Vec<_> = state.workspaces.iter().map(|it| it.active).collect();
    assert_eq!(active, [false, true, true]);
}

#[test]
fn an_unfocused_activation_leaves_the_focused_output_alone() {
    let mut state = state();
    state.focus.output = Some("eDP-1".to_owned());

    apply(
        &mut state,
        Event::WorkspaceActivated {
            id: WorkspaceId(2),
            focused: false,
        },
    );

    assert_eq!(state.focus.output.as_deref(), Some("eDP-1"));
}

#[test]
fn activating_an_unknown_workspace_asks_for_a_structure_refetch() {
    let mut state = state();

    let resync = apply(
        &mut state,
        Event::WorkspaceActivated {
            id: WorkspaceId(9),
            focused: true,
        },
    );

    assert_eq!(resync, Some(Resync::Structure));
}

#[test]
fn a_new_workspace_list_carries_the_focused_output_with_it() {
    let mut state = state();

    apply(
        &mut state,
        Event::WorkspacesChanged {
            workspaces: vec![workspace(4, "HDMI-A-1", true)],
            focused: Some(WorkspaceId(4)),
        },
    );

    assert_eq!(state.focus.output.as_deref(), Some("HDMI-A-1"));
}

#[test]
fn focusing_a_window_clears_its_urgency() {
    let mut state = state();

    apply(&mut state, Event::WindowFocusChanged(Some(WindowId(2))));

    assert_eq!(state.focus.window, Some(WindowId(2)));
    assert!(!state.windows[1].urgent);
}

#[test]
fn a_focused_window_change_moves_focus_and_an_unfocused_one_does_not() {
    let mut state = state();

    apply(
        &mut state,
        Event::WindowOpenedOrChanged {
            window: window(3, false),
            focused: true,
        },
    );
    apply(
        &mut state,
        Event::WindowOpenedOrChanged {
            window: window(1, false),
            focused: false,
        },
    );

    assert_eq!(state.focus.window, Some(WindowId(3)));
    assert_eq!(state.windows.len(), 3);
}

#[test]
fn a_layout_switch_past_the_known_layouts_clears_the_current_one() {
    let mut state = state();
    state.keyboard.codes = vec!["us".to_owned(), "pl".to_owned()];

    apply(&mut state, Event::KeyboardLayoutSwitched(1));
    assert_eq!(state.keyboard.current, Some(1));

    apply(&mut state, Event::KeyboardLayoutSwitched(2));
    assert_eq!(state.keyboard.current, None);
}

#[test]
fn a_cast_update_replaces_the_cast_with_its_id() {
    let mut state = state();
    let cast = |active| Cast {
        id: 7,
        active,
        output: None,
        session: Some(1),
        pw_node: None,
    };

    apply(&mut state, Event::CastStartedOrChanged(cast(true)));
    apply(&mut state, Event::CastStartedOrChanged(cast(false)));
    assert_eq!(state.casts, [cast(false)]);

    apply(&mut state, Event::CastStopped(7));
    assert!(state.casts.is_empty());
}

#[test]
fn a_resync_is_handed_back_and_changes_nothing() {
    let mut state = state();
    let before = state.clone();

    assert_eq!(
        apply(&mut state, Event::Resync(Resync::Keyboard)),
        Some(Resync::Keyboard)
    );
    assert_eq!(state, before);
}
