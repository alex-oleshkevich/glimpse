use chrono::{TimeZone, Utc};
use glimpse_dbus::notifications::NotificationUrgency;

use super::*;

fn note(id: u32, summary: &str) -> NotificationRecord {
    NotificationRecord {
        id,
        app_id: "app".to_owned(),
        app_name: "App".to_owned(),
        app_pid: Some(7),
        summary: summary.to_owned(),
        body: None,
        icon: None,
        image: None,
        urgency: NotificationUrgency::Normal,
        actions: Vec::new(),
        progress: None,
        created: Utc.timestamp_opt(i64::from(id), 0).unwrap(),
        unread: true,
        resident: false,
        expire_timeout: -1,
    }
}

fn output(name: &str, focused: bool) -> OutputInfo {
    OutputInfo {
        connector: name.to_owned(),
        label: None,
        built_in: false,
        focused,
        make: None,
        model: None,
        serial: None,
        current_mode: None,
        logical: None,
        enabled: true,
    }
}

fn ready(state: &mut PopupState) {
    state.set_dnd(false);
    state.set_session(false, false);
}

#[test]
fn startup_and_reconnect_snapshots_never_replay_history() {
    let mut state = PopupState::new(Notifications::default());
    assert_eq!(state.update(vec![note(1, "old")]), Delta::default());
    assert!(state.visible().is_empty());
    ready(&mut state);
    assert_eq!(
        state.update(vec![note(2, "new"), note(1, "old")]).appeared,
        [2]
    );
    assert_eq!(state.disconnected().removed, [2]);
    ready(&mut state);
    assert_eq!(
        state.update(vec![note(3, "during reconnect"), note(2, "new")]),
        Delta::default()
    );
    assert!(state.visible().is_empty());
}

#[test]
fn replacements_keep_position_and_are_reported_without_entrance() {
    let mut state = PopupState::new(Notifications::default());
    state.update(Vec::new());
    ready(&mut state);
    state.update(vec![note(1, "one")]);
    let delta = state.update(vec![note(1, "changed")]);
    assert!(delta.appeared.is_empty());
    assert_eq!(delta.replaced, [1]);
    assert_eq!(state.visible(), [1]);
}

#[test]
fn external_removal_closes_the_matching_popup() {
    let mut state = PopupState::new(Notifications::default());
    state.update(Vec::new());
    ready(&mut state);
    state.update(vec![note(2, "two"), note(1, "one")]);

    assert_eq!(state.update(vec![note(2, "two")]).removed, [1]);
    assert_eq!(state.visible(), [2]);
}

#[test]
fn notifications_wait_for_every_startup_gate_without_replaying() {
    let mut state = PopupState::new(Notifications::default());
    state.update(Vec::new());
    assert!(
        state
            .update(vec![note(1, "before gates")])
            .appeared
            .is_empty()
    );
    state.set_dnd(false);
    assert!(
        state
            .update(vec![note(1, "before gates")])
            .appeared
            .is_empty()
    );
    state.set_session(false, false);
    assert!(
        state
            .update(vec![note(1, "before gates")])
            .appeared
            .is_empty()
    );
    assert_eq!(
        state
            .update(vec![note(2, "after gates"), note(1, "before gates")])
            .appeared,
        [2]
    );
}

#[test]
fn every_gate_clears_and_never_replays_what_arrived_behind_it() {
    for gate in 0..4 {
        let mut state = PopupState::new(Notifications::default());
        state.update(Vec::new());
        ready(&mut state);
        state.update(vec![note(1, "shown")]);
        let cleared = match gate {
            0 => {
                let cleared = state.set_dnd(true);
                state.update(vec![note(2, "quiet")]);
                state.set_dnd(false);
                cleared
            }
            1 => {
                let cleared = state.set_session(true, false);
                state.update(vec![note(2, "locked")]);
                state.set_session(false, false);
                cleared
            }
            2 => {
                let cleared = state.set_session(false, true);
                state.update(vec![note(2, "private")]);
                state.set_session(false, false);
                cleared
            }
            _ => {
                let mut settings = state.settings().clone();
                settings.enabled = false;
                let cleared = state.configure(settings.clone());
                state.update(vec![note(2, "disabled")]);
                settings.enabled = true;
                state.configure(settings);
                cleared
            }
        };
        assert_eq!(cleared.removed, [1]);
        assert!(state.visible().is_empty());
        assert_eq!(
            state
                .update(vec![note(3, "later"), note(2, "hidden")])
                .appeared,
            [3]
        );
    }
}

#[test]
fn cap_keeps_newest_and_reload_trims_immediately() {
    let mut settings = Notifications {
        max_items: 2,
        ..Notifications::default()
    };
    let mut state = PopupState::new(settings.clone());
    state.update(Vec::new());
    ready(&mut state);
    let delta = state.update(vec![note(3, "three"), note(2, "two"), note(1, "one")]);
    assert_eq!(delta.appeared, [3, 2]);
    assert_eq!(delta.removed, [1]);
    assert_eq!(state.visible(), [3, 2]);
    settings.max_items = 1;
    assert_eq!(state.configure(settings).removed, [2]);
    assert_eq!(state.visible(), [3]);
}

#[test]
fn equal_timestamps_are_ordered_by_newer_id() {
    let mut state = PopupState::new(Notifications::default());
    state.update(Vec::new());
    ready(&mut state);
    let mut one = note(1, "one");
    let mut two = note(2, "two");
    two.created = one.created;
    assert_eq!(
        state.update(vec![one.clone(), two.clone()]).appeared,
        [2, 1]
    );
    assert_eq!(state.visible(), [2, 1]);

    one.summary = "one again".to_owned();
    two.summary = "two again".to_owned();
    assert_eq!(state.update(vec![one, two]).replaced, [2, 1]);
}

#[test]
fn placement_prefers_config_then_focus_and_defers_changes_until_empty() {
    let mut settings = Notifications {
        monitor: Some("DP-2".to_owned()),
        ..Notifications::default()
    };
    let mut state = PopupState::new(settings.clone());
    state.set_outputs(vec![output("eDP-1", true), output("DP-2", false)]);
    state.update(Vec::new());
    ready(&mut state);
    state.update(vec![note(1, "one")]);
    assert_eq!(
        state.placement().unwrap().connector.as_deref(),
        Some("DP-2")
    );
    settings.monitor = Some("missing".to_owned());
    settings.edge = NotificationEdge::BottomRight;
    state.configure(settings);
    assert_eq!(state.placement().unwrap().edge, NotificationEdge::TopRight);
    state.hide(1);
    state.update(vec![note(2, "two"), note(1, "one")]);
    assert_eq!(
        state.placement().unwrap().connector.as_deref(),
        Some("eDP-1")
    );
    assert_eq!(
        state.placement().unwrap().edge,
        NotificationEdge::BottomRight
    );
}

#[test]
fn disappearing_output_reselects_while_visible() {
    let mut state = PopupState::new(Notifications::default());
    state.set_outputs(vec![output("DP-2", true), output("eDP-1", false)]);
    state.update(Vec::new());
    ready(&mut state);
    state.update(vec![note(1, "one")]);
    assert!(state.set_outputs(vec![output("eDP-1", true)]));
    assert_eq!(
        state.placement().unwrap().connector.as_deref(),
        Some("eDP-1")
    );
}

#[test]
fn a_disabled_output_still_listed_reselects_the_placement() {
    let mut state = PopupState::new(Notifications::default());
    state.set_outputs(vec![output("DP-2", true), output("eDP-1", false)]);
    state.update(Vec::new());
    ready(&mut state);
    state.update(vec![note(1, "one")]);

    let mut disabled = output("DP-2", true);
    disabled.enabled = false;
    assert!(state.set_outputs(vec![disabled, output("eDP-1", false)]));
    assert_eq!(
        state.placement().unwrap().connector.as_deref(),
        Some("eDP-1")
    );
}

#[test]
fn a_late_output_baseline_places_an_already_visible_popup() {
    let mut state = PopupState::new(Notifications::default());
    state.update(Vec::new());
    ready(&mut state);
    state.update(vec![note(1, "one")]);
    assert_eq!(state.placement().unwrap().connector, None);

    assert!(state.set_outputs(vec![output("DP-2", true)]));
    assert_eq!(
        state.placement().unwrap().connector.as_deref(),
        Some("DP-2")
    );
}

#[test]
fn right_click_and_timeout_hide_only_the_popup() {
    let mut state = PopupState::new(Notifications::default());
    state.update(Vec::new());
    ready(&mut state);
    state.update(vec![note(1, "one")]);
    assert_eq!(state.hide(1).removed, [1]);
    assert!(state.record(1).is_some());
}
