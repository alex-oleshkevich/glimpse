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
fn challenge_hides_because_polkit_cannot_prompt_through_a_lock() {
    assert_eq!(state("challenge", &[], Action::Suspend), hidden());
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
