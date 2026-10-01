use std::collections::HashMap;

use gettextrs::gettext;
use glimpse_dbus::login1::Login1InhibitorEntry;
use glimpse_widgets::SessionActionState;

const SUBTITLE_MAX_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Suspend,
    Reboot,
    PowerOff,
}

pub const ACTIONS: [Action; 3] = [Action::Suspend, Action::Reboot, Action::PowerOff];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed,
    TimedOut,
}

impl Action {
    pub fn key(self) -> &'static str {
        match self {
            Self::Suspend => glimpse_widgets::SUSPEND,
            Self::Reboot => glimpse_widgets::REBOOT,
            Self::PowerOff => glimpse_widgets::POWER_OFF,
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        ACTIONS.into_iter().find(|action| action.key() == key)
    }

    fn inhibits_what(self) -> &'static str {
        match self {
            Self::Suspend => "sleep",
            Self::Reboot | Self::PowerOff => "shutdown",
        }
    }

    pub fn error_text(self) -> String {
        match self {
            Self::Suspend => gettext("Couldn't suspend the system"),
            Self::Reboot => gettext("Couldn't restart the system"),
            Self::PowerOff => gettext("Couldn't power off the system"),
        }
    }
}

impl From<glimpse_config::LockSessionAction> for Action {
    fn from(action: glimpse_config::LockSessionAction) -> Self {
        match action {
            glimpse_config::LockSessionAction::Suspend => Self::Suspend,
            glimpse_config::LockSessionAction::Reboot => Self::Reboot,
            glimpse_config::LockSessionAction::PowerOff => Self::PowerOff,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answers {
    pub suspend: String,
    pub reboot: String,
    pub power_off: String,
    pub inhibitors: Vec<Login1InhibitorEntry>,
}

impl Answers {
    fn answer(&self, action: Action) -> &str {
        match action {
            Action::Suspend => &self.suspend,
            Action::Reboot => &self.reboot,
            Action::PowerOff => &self.power_off,
        }
    }
}

pub fn state(
    answer: &str,
    inhibitors: &[Login1InhibitorEntry],
    action: Action,
) -> SessionActionState {
    match answer {
        "yes" => visible(),
        "inhibited" | "inhibitor-blocked" | "challenge-inhibitor-blocked" => {
            blocked(inhibitors, action)
        }
        _ => hidden(),
    }
}

pub fn states(
    config: &glimpse_config::LockSession,
    answers: &Answers,
) -> HashMap<&'static str, SessionActionState> {
    let allowed: Vec<Action> = if config.enabled {
        config.actions.iter().copied().map(Action::from).collect()
    } else {
        Vec::new()
    };
    ACTIONS
        .into_iter()
        .map(|action| {
            let value = if allowed.contains(&action) {
                state(answers.answer(action), &answers.inhibitors, action)
            } else {
                hidden()
            };
            (action.key(), value)
        })
        .collect()
}

fn visible() -> SessionActionState {
    SessionActionState {
        visible: true,
        enabled: true,
        subtitle: None,
    }
}

fn hidden() -> SessionActionState {
    SessionActionState {
        visible: false,
        enabled: false,
        subtitle: None,
    }
}

fn blocked(inhibitors: &[Login1InhibitorEntry], action: Action) -> SessionActionState {
    let subtitle = blocking_inhibitor(inhibitors, action).map(|(who, why)| {
        gettext("Blocked by {who}: {why}")
            .replace("{who}", &who)
            .replace("{why}", &why)
    });
    SessionActionState {
        visible: true,
        enabled: false,
        subtitle,
    }
}

fn blocking_inhibitor(
    inhibitors: &[Login1InhibitorEntry],
    action: Action,
) -> Option<(String, String)> {
    let needed = action.inhibits_what();
    inhibitors
        .iter()
        .find(|(what, _, _, mode, _, _)| mode == "block" && what.split(':').any(|w| w == needed))
        .map(|(_, who, why, _, _, _)| {
            (
                glimpse_utils::clean(who, SUBTITLE_MAX_CHARS),
                glimpse_utils::clean(why, SUBTITLE_MAX_CHARS),
            )
        })
}

#[cfg(test)]
mod tests;
