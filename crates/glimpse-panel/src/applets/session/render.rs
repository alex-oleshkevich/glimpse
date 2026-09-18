use gettextrs::{gettext, ngettext};
use glimpse_services::{
    SessionAction, SessionActionsState, SessionCapability, SessionEntry, SessionInhibitor,
    SessionUpdates,
};
use glimpse_widgets::{SessionActionState, SessionChoice};

pub fn heading(state: &SessionActionsState) -> (Option<&str>, Option<String>) {
    (state.user.as_deref(), state.signed_in_seconds.map(duration))
}

pub fn action_state(capability: SessionCapability) -> SessionActionState {
    SessionActionState {
        visible: capability.visible(),
        enabled: capability.enabled(),
        subtitle: match capability {
            SessionCapability::Blocked => Some(gettext("Blocked by an active inhibitor.")),
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

pub fn sessions(entries: &[SessionEntry]) -> Vec<SessionChoice> {
    entries
        .iter()
        .filter(|entry| !entry.active)
        .map(|entry| SessionChoice {
            id: entry.id.clone(),
            user: entry.user.clone(),
            subtitle: Some(kind_label(&entry.kind)),
        })
        .collect()
}

pub fn updates(updates: Option<&SessionUpdates>) -> Option<String> {
    updates.map(|updates| match updates.available {
        true => gettext("Updates available"),
        false => gettext("No updates available"),
    })
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
        SessionAction::Lock | SessionAction::Activate(_) => return None,
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

fn kind_label(kind: &str) -> String {
    match kind {
        "wayland" => gettext("Wayland"),
        "x11" => gettext("X11"),
        "tty" => gettext("Terminal"),
        other => other.to_owned(),
    }
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
        assert!(!action_state(SessionCapability::Hidden).visible);
    }

    #[test]
    fn a_blocked_action_stays_visible_with_a_reason() {
        let state = action_state(SessionCapability::Blocked);
        assert!(state.visible);
        assert!(!state.enabled);
        assert_eq!(
            state.subtitle.as_deref(),
            Some("Blocked by an active inhibitor.")
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
    fn other_sessions_drop_the_active_one_and_label_the_kind() {
        let listed = sessions(&[
            SessionEntry {
                id: "current".into(),
                user: "me".into(),
                kind: "wayland".into(),
                active: true,
            },
            SessionEntry {
                id: "other".into(),
                user: "you".into(),
                kind: "x11".into(),
                active: false,
            },
        ]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "other");
        assert_eq!(listed[0].subtitle.as_deref(), Some("X11"));
    }

    #[test]
    fn updates_are_absent_without_packagekit_and_do_not_invent_a_count() {
        assert_eq!(updates(None), None);
        assert_eq!(
            updates(Some(&SessionUpdates { available: true })).as_deref(),
            Some("Updates available")
        );
        assert_eq!(
            updates(Some(&SessionUpdates { available: false })).as_deref(),
            Some("No updates available")
        );
    }

    #[test]
    fn signed_in_today_is_not_a_day_count() {
        let state = SessionActionsState {
            user: Some("alex".into()),
            session_type: Some("wayland".into()),
            signed_in_seconds: Some(3_600),
            ..Default::default()
        };
        let (user, signed) = heading(&state);
        assert_eq!(user, Some("alex"));
        assert_eq!(signed.as_deref(), Some("Signed in today"));
    }
}
