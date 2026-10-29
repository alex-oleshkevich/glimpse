use glimpse_services::{SessionCapability, SessionInhibitor};

use super::*;

#[test]
fn unsupported_actions_do_not_render() {
    assert!(
        !action_state(
            &SessionActionsState::default(),
            SessionAction::Suspend,
            SessionCapability::Hidden
        )
        .visible
    );
}

#[test]
fn a_blocked_action_stays_visible_and_names_who_blocks_it() {
    let state = SessionActionsState {
        inhibitors: vec![
            SessionInhibitor {
                what: "shutdown".into(),
                who: "Updater".into(),
                why: "install".into(),
                mode: "block".into(),
            },
            SessionInhibitor {
                what: "sleep".into(),
                who: "Steam".into(),
                why: "download".into(),
                mode: "block".into(),
            },
        ],
        ..Default::default()
    };
    let row = action_state(&state, SessionAction::Suspend, SessionCapability::Blocked);
    assert!(row.visible);
    assert!(!row.enabled);
    assert_eq!(row.subtitle.as_deref(), Some("Blocked by Steam"));
    let delayed_first = SessionActionsState {
        inhibitors: vec![
            SessionInhibitor {
                what: "sleep".into(),
                who: "glimpse-lock".into(),
                why: "locking".into(),
                mode: "delay".into(),
            },
            state.inhibitors[1].clone(),
        ],
        ..Default::default()
    };
    assert_eq!(
        action_state(
            &delayed_first,
            SessionAction::Suspend,
            SessionCapability::Blocked
        )
        .subtitle
        .as_deref(),
        Some("Blocked by Steam"),
        "a delay inhibitor is not the blocker"
    );
    let weak = SessionActionsState {
        inhibitors: vec![SessionInhibitor {
            what: "sleep".into(),
            who: "Steam".into(),
            why: "download".into(),
            mode: "block-weak".into(),
        }],
        ..Default::default()
    };
    assert_eq!(
        action_state(&weak, SessionAction::Suspend, SessionCapability::Blocked)
            .subtitle
            .as_deref(),
        Some("Blocked by Steam")
    );
    assert_eq!(
        action_state(
            &SessionActionsState::default(),
            SessionAction::Suspend,
            SessionCapability::Blocked
        )
        .subtitle
        .as_deref(),
        Some("Blocked by an active inhibitor."),
        "logind says blocked but lists no matching inhibitor"
    );
}

#[test]
fn action_mapping_refuses_unknown_widget_signals() {
    assert!(action_from("unknown").is_none());
    assert_eq!(
        action_from(glimpse_widgets::LOCK),
        Some(SessionAction::Lock)
    );
}

#[test]
fn only_hibernate_confirms_locally() {
    let state = SessionActionsState::default();
    assert!(confirm(&state, SessionAction::Lock).is_none());
    assert!(confirm(&state, SessionAction::Suspend).is_none());
    assert!(
        confirm(&state, SessionAction::LogOut).is_none(),
        "log out is confirmed by agents, the same dialog as restart"
    );
    assert!(confirm(&state, SessionAction::Reboot).is_none());
    assert!(confirm(&state, SessionAction::PowerOff).is_none());
    let (title, _, accept) = confirm(&state, SessionAction::Hibernate).expect("hibernate");
    assert_eq!(title, "Hibernate");
    assert_eq!(accept, "Hibernate");
}

#[test]
fn blockers_describe_open_windows_without_claiming_unsaved_work() {
    let state = SessionActionsState {
        windows: Some(2),
        ..Default::default()
    };

    assert_eq!(
        blockers(&state, SessionAction::LogOut),
        ["2 open windows are still running."]
    );
    assert!(blockers(&state, SessionAction::Suspend).is_empty());
}

#[test]
fn blockers_name_inhibitors_that_match_the_action() {
    let state = SessionActionsState {
        inhibitors: vec![
            SessionInhibitor {
                what: "sleep".into(),
                who: "Player".into(),
                why: "video".into(),
                mode: "delay".into(),
            },
            SessionInhibitor {
                what: "shutdown".into(),
                who: "Updater".into(),
                why: "install".into(),
                mode: "block".into(),
            },
            SessionInhibitor {
                what: "shutdown".into(),
                who: "Saver".into(),
                why: "save".into(),
                mode: "block-weak".into(),
            },
        ],
        ..Default::default()
    };

    assert!(
        blockers(&state, SessionAction::Suspend).is_empty(),
        "a delay inhibitor is not listed as blocking sleep"
    );
    assert_eq!(
        blockers(&state, SessionAction::PowerOff),
        ["Updater: install", "Saver: save"]
    );
    assert_eq!(
        blockers(&state, SessionAction::LogOut),
        ["Updater: install", "Saver: save"],
        "an app holding shutdown for unsaved work is named before logging out"
    );
}

#[test]
fn signed_in_today_is_not_a_day_count() {
    let state = SessionActionsState {
        user: Some("sam".into()),
        signed_in_seconds: Some(3_600),
        ..Default::default()
    };
    let (user, signed) = heading(&state);
    assert_eq!(user, Some("sam"));
    assert_eq!(signed.as_deref(), Some("Signed in today"));
}
