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
                    Some(entry) => gettext("Blocked by {who}")
                        .replace("{who}", &super::super::app_label(&entry.who)),
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
        .map(|entry| format!("{}: {}", super::super::app_label(&entry.who), entry.why))
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
mod tests;
