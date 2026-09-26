use gettextrs::{gettext, ngettext};
use glimpse_services::{SessionAction, SessionActionsState, SessionCapability, SessionInhibitor};
use glimpse_widgets::SessionActionState;

pub fn heading(state: &SessionActionsState) -> (Option<&str>, Option<String>) {
    (state.user.as_deref(), state.signed_in_seconds.map(duration))
}

pub fn action_state(
    state: &SessionActionsState,
    action: SessionAction,
    capability: SessionCapability,
) -> SessionActionState {
    SessionActionState {
        visible: capability.visible(),
        enabled: capability.enabled(),
        subtitle: match capability {
            SessionCapability::Blocked => Some(
                match state
                    .inhibitors
                    .iter()
                    .find(|entry| matches_action(entry, &action))
                {
                    Some(entry) => gettext("Blocked by {who}").replace("{who}", &entry.who),
                    None => gettext("Blocked by an active inhibitor."),
                },
            ),
            SessionCapability::Hidden | SessionCapability::Available => None,
        },
    }
}

pub fn always() -> SessionActionState {
    SessionActionState {
        visible: true,
        enabled: true,
        subtitle: None,
    }
}

pub fn action_from(action: &str) -> Option<SessionAction> {
    match action {
        glimpse_widgets::LOCK => Some(SessionAction::Lock),
        glimpse_widgets::SUSPEND => Some(SessionAction::Suspend),
        glimpse_widgets::HIBERNATE => Some(SessionAction::Hibernate),
        glimpse_widgets::LOG_OUT => Some(SessionAction::LogOut),
        glimpse_widgets::REBOOT => Some(SessionAction::Reboot),
        glimpse_widgets::POWER_OFF => Some(SessionAction::PowerOff),
        _ => None,
    }
}

pub fn confirm(
    state: &SessionActionsState,
    action: SessionAction,
) -> Option<(String, String, String)> {
    let (title, body, accept) = match action {
        SessionAction::Suspend => (
            gettext("Suspend"),
            gettext("Suspend this session?"),
            gettext("Suspend"),
        ),
        SessionAction::Hibernate => (
            gettext("Hibernate"),
            gettext("Hibernate this session?"),
            gettext("Hibernate"),
        ),
        SessionAction::LogOut => (
            gettext("Log out"),
            gettext("This closes the current session. Applications may not shut down gracefully."),
            gettext("Log out"),
        ),
        SessionAction::Reboot => (
            gettext("Restart"),
            gettext("Restart this computer?"),
            gettext("Restart"),
        ),
        SessionAction::PowerOff => (
            gettext("Shut down"),
            gettext("Shut down this computer?"),
            gettext("Shut down"),
        ),
        SessionAction::Lock => return None,
    };
    let blockers = blockers(state, action);
    let body = match blockers.is_empty() {
        true => body,
        false => format!("{body}\n\n{}", blockers.join("\n")),
    };
    Some((title, body, accept))
}

pub fn blockers(state: &SessionActionsState, action: SessionAction) -> Vec<String> {
    let mut blockers = state
        .inhibitors
        .iter()
        .filter(|entry| matches_action(entry, &action))
        .map(|entry| format!("{}: {}", entry.who, entry.why))
        .collect::<Vec<_>>();
    if counts_windows(&action)
        && let Some(windows) = state.windows.filter(|windows| *windows > 0)
    {
        blockers.push(
            ngettext(
                "{windows} open window is still running.",
                "{windows} open windows are still running.",
                windows.min(u32::MAX as usize) as u32,
            )
            .replace("{windows}", &windows.to_string()),
        );
    }
    blockers
}

fn matches_action(entry: &SessionInhibitor, action: &SessionAction) -> bool {
    action.inhibits().is_some_and(|what| entry.blocks(what))
}

fn counts_windows(action: &SessionAction) -> bool {
    matches!(
        action,
        SessionAction::LogOut | SessionAction::Reboot | SessionAction::PowerOff
    )
}

fn duration(seconds: u64) -> String {
    match seconds / 86_400 {
        0 => gettext("Signed in today"),
        days => ngettext(
            "Signed in for {days} day",
            "Signed in for {days} days",
            days.min(u64::from(u32::MAX)) as u32,
        )
        .replace("{days}", &days.to_string()),
    }
}

#[cfg(test)]
mod tests {
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
    fn lock_does_not_confirm() {
        assert!(confirm(&SessionActionsState::default(), SessionAction::Lock).is_none());
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
            ],
            ..Default::default()
        };

        assert_eq!(blockers(&state, SessionAction::Suspend), ["Player: video"]);
        assert_eq!(
            blockers(&state, SessionAction::PowerOff),
            ["Updater: install"]
        );
    }

    #[test]
    fn signed_in_today_is_not_a_day_count() {
        let state = SessionActionsState {
            user: Some("alex".into()),
            signed_in_seconds: Some(3_600),
            ..Default::default()
        };
        let (user, signed) = heading(&state);
        assert_eq!(user, Some("alex"));
        assert_eq!(signed.as_deref(), Some("Signed in today"));
    }
}
