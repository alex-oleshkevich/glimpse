use super::*;

fn state(group: &gio::SimpleActionGroup, name: &str) -> glib::Variant {
    group.action_state(name).expect("state")
}

#[test]
fn install_publishes_version_and_status() {
    let group = gio::SimpleActionGroup::new();
    ComponentActions::install(&group, "1.2.3", Ok(()));
    assert_eq!(state(&group, "version"), "1.2.3".to_variant());
    assert_eq!(state(&group, "config-status"), (true, "").to_variant());
}

#[test]
fn install_publishes_initial_failure() {
    let group = gio::SimpleActionGroup::new();
    ComponentActions::install(&group, "1.2.3", Err("broken".into()));
    assert_eq!(
        state(&group, "config-status"),
        (false, "broken").to_variant()
    );
}

#[test]
fn report_replaces_status() {
    let group = gio::SimpleActionGroup::new();
    let actions = ComponentActions::install(&group, "1.2.3", Ok(()));
    actions.report(Err("x".into()));
    assert_eq!(state(&group, "config-status"), (false, "x").to_variant());
    actions.report(Ok(()));
    assert_eq!(state(&group, "config-status"), (true, "").to_variant());
}

#[test]
fn remote_state_changes_are_ignored() {
    let group = gio::SimpleActionGroup::new();
    ComponentActions::install(&group, "1.2.3", Ok(()));
    group.change_action_state("config-status", &(true, "forged").to_variant());
    group.change_action_state("version", &"9.9.9".to_variant());
    assert_eq!(state(&group, "version"), "1.2.3".to_variant());
    assert_eq!(state(&group, "config-status"), (true, "").to_variant());
}
