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
mod tests {
    use super::*;

    fn inhibitor(what: &str, who: &str, why: &str, mode: &str) -> Login1InhibitorEntry {
        (
            what.to_owned(),
            who.to_owned(),
            why.to_owned(),
            mode.to_owned(),
            1000,
            1234,
        )
    }

    #[test]
    fn yes_is_visible_and_enabled() {
        assert_eq!(state("yes", &[], Action::Suspend), visible());
    }

    #[test]
    fn no_hides() {
        assert_eq!(state("no", &[], Action::Suspend), hidden());
    }

    #[test]
    fn na_hides() {
        assert_eq!(state("na", &[], Action::Suspend), hidden());
    }

    #[test]
    fn challenge_hides_because_polkit_cannot_prompt_through_a_lock() {
        assert_eq!(state("challenge", &[], Action::Suspend), hidden());
    }

    #[test]
    fn an_unknown_answer_hides() {
        assert_eq!(state("some-future-answer", &[], Action::Suspend), hidden());
    }

    #[test]
    fn inhibited_with_no_matching_inhibitor_still_disables() {
        let result = state("inhibited", &[], Action::Suspend);
        assert!(result.visible);
        assert!(!result.enabled);
        assert_eq!(result.subtitle, None);
    }

    #[test]
    fn inhibitor_blocked_takes_the_first_matching_block_inhibitors_who_and_why() {
        let inhibitors = vec![
            inhibitor("idle", "screensaver", "dim", "block"),
            inhibitor("sleep:shutdown", "backup", "syncing files", "block"),
            inhibitor("sleep", "second", "also blocking", "block"),
            inhibitor("sleep", "other", "irrelevant", "delay"),
        ];
        let result = state("inhibitor-blocked", &inhibitors, Action::Suspend);
        assert!(result.visible);
        assert!(!result.enabled);
        assert_eq!(
            result.subtitle.as_deref(),
            Some("Blocked by backup: syncing files"),
            "the first matching block inhibitor wins, not a later one"
        );
    }

    #[test]
    fn challenge_inhibitor_blocked_disables_with_the_shutdown_inhibitor() {
        let inhibitors = vec![inhibitor("shutdown", "updater", "installing", "block")];
        let result = state("challenge-inhibitor-blocked", &inhibitors, Action::Reboot);
        assert!(result.visible);
        assert!(!result.enabled);
        assert_eq!(
            result.subtitle.as_deref(),
            Some("Blocked by updater: installing")
        );
    }

    #[test]
    fn a_delay_inhibitor_never_supplies_the_subtitle() {
        let inhibitors = vec![inhibitor("sleep", "player", "playing", "delay")];
        let result = state("inhibited", &inhibitors, Action::Suspend);
        assert_eq!(result.subtitle, None);
    }

    #[test]
    fn who_and_why_are_cleaned() {
        let inhibitors = vec![inhibitor(
            "sleep",
            "editor\u{202e}gpj.exe",
            "save\nwork",
            "block",
        )];
        let result = state("inhibited", &inhibitors, Action::Suspend);
        assert_eq!(
            result.subtitle.as_deref(),
            Some("Blocked by editor gpj.exe: save work")
        );
    }

    #[test]
    fn suspend_is_blocked_only_by_a_sleep_inhibitor() {
        let inhibitors = vec![inhibitor("shutdown", "updater", "installing", "block")];
        let result = state("inhibited", &inhibitors, Action::Suspend);
        assert_eq!(result.subtitle, None, "shutdown does not cover suspend");
    }

    #[test]
    fn reboot_and_power_off_are_blocked_by_a_shutdown_inhibitor() {
        let inhibitors = vec![inhibitor("shutdown", "updater", "installing", "block")];
        assert!(
            state("inhibited", &inhibitors, Action::Reboot)
                .subtitle
                .is_some()
        );
        assert!(
            state("inhibited", &inhibitors, Action::PowerOff)
                .subtitle
                .is_some()
        );
    }

    fn config(
        enabled: bool,
        actions: &[glimpse_config::LockSessionAction],
    ) -> glimpse_config::LockSession {
        glimpse_config::LockSession {
            enabled,
            actions: actions.to_vec(),
        }
    }

    #[test]
    fn an_action_missing_from_config_hides_even_when_logind_says_yes() {
        let answers = Answers {
            suspend: "yes".to_owned(),
            reboot: "yes".to_owned(),
            power_off: "yes".to_owned(),
            inhibitors: Vec::new(),
        };
        let config = config(true, &[glimpse_config::LockSessionAction::Suspend]);
        let states = states(&config, &answers);
        assert_eq!(states[Action::Suspend.key()], visible());
        assert_eq!(states[Action::Reboot.key()], hidden());
        assert_eq!(states[Action::PowerOff.key()], hidden());
    }

    #[test]
    fn the_whole_table_disabled_hides_every_action() {
        let answers = Answers {
            suspend: "yes".to_owned(),
            reboot: "yes".to_owned(),
            power_off: "yes".to_owned(),
            inhibitors: Vec::new(),
        };
        let config = config(
            false,
            &[
                glimpse_config::LockSessionAction::Suspend,
                glimpse_config::LockSessionAction::Reboot,
                glimpse_config::LockSessionAction::PowerOff,
            ],
        );
        let states = states(&config, &answers);
        for action in ACTIONS {
            assert_eq!(states[action.key()], hidden());
        }
    }
}
