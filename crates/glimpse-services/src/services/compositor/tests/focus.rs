use super::*;

#[test]
fn a_window_that_opens_focused_takes_focus_from_whatever_had_it() {
    let mut had_focus = window(1, 4);
    had_focus.is_focused = true;
    let mut opening = window(3, 4);
    opening.is_focused = true;

    let mut state = Snapshot {
        windows: vec![had_focus, window(2, 4)],
        focused_window: Some(WindowId(1)),
        ..Snapshot::default()
    };

    apply(&mut state, Change::WindowOpenedOrChanged(opening));

    assert_eq!(
        state
            .windows
            .iter()
            .filter(|window| window.is_focused)
            .map(|window| window.id)
            .collect::<Vec<WindowId>>(),
        [WindowId(3)],
        "a session focuses one window, so a popover listing them must never tick two rows"
    );
    assert_eq!(state.focused_window, Some(WindowId(3)));
}

#[test]
fn a_window_changing_without_focus_leaves_focus_where_it_is() {
    let mut had_focus = window(1, 4);
    had_focus.is_focused = true;
    let mut retitled = window(2, 4);
    retitled.title = Some("vim".to_owned());

    let mut state = Snapshot {
        windows: vec![had_focus, window(2, 4)],
        focused_window: Some(WindowId(1)),
        ..Snapshot::default()
    };

    apply(&mut state, Change::WindowOpenedOrChanged(retitled));

    assert_eq!(state.focused_window, Some(WindowId(1)));
    assert!(
        state.windows[0].is_focused,
        "a title arriving on an unfocused window must not move focus onto nothing"
    );
}

#[test]
fn a_window_is_focusable_by_the_pid_of_the_process_that_owns_it() {
    let mut owned = window(9, 1);
    owned.pid = Some(4265);
    let state = Snapshot {
        windows: vec![window(2, 1), owned],
        ..Snapshot::default()
    };

    assert_eq!(
        window_target(WindowRef::Pid { pid: 4265 }, Some(&state)),
        Some(WindowTarget::Id(WindowId(9)))
    );
}

#[test]
fn a_process_owning_several_windows_always_resolves_to_the_same_one() {
    let mut later = window(9, 1);
    later.pid = Some(4265);
    let mut earlier = window(3, 1);
    earlier.pid = Some(4265);
    let state = Snapshot {
        windows: vec![later, earlier],
        ..Snapshot::default()
    };

    assert_eq!(
        window_target(WindowRef::Pid { pid: 4265 }, Some(&state)),
        Some(WindowTarget::Id(WindowId(3)))
    );
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

#[test]
fn a_pid_with_no_window_is_refused_and_so_is_one_asked_before_the_first_snapshot() {
    let state = Snapshot {
        windows: vec![window(2, 1)],
        ..Snapshot::default()
    };

    assert_eq!(
        window_target(WindowRef::Pid { pid: 4265 }, Some(&state)),
        None
    );
    assert_eq!(window_target(WindowRef::Pid { pid: 4265 }, None), None);
    assert_eq!(
        window_target(WindowRef::Id { id: 2 }, None),
        Some(WindowTarget::Id(WindowId(2))),
        "a reference that needs no window list still resolves without one"
    );
}
