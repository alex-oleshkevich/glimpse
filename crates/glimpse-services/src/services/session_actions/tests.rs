use super::*;

#[test]
fn capability_keeps_inhibitor_blocks_visible_and_disabled() {
    let mapped = capability("inhibitor-blocked");
    assert!(mapped.visible());
    assert!(!mapped.enabled());
    assert_eq!(mapped, Capability::Blocked);
}

#[test]
fn a_polkit_challenge_stays_available() {
    assert_eq!(capability("challenge"), Capability::Available);
    assert!(capability("challenge").enabled());
}

#[test]
fn unsupported_capability_is_absent() {
    assert!(!capability("na").visible());
    assert_eq!(capability("no"), Capability::Hidden);
}

#[test]
fn lock_does_not_confirm_power_actions_do() {
    assert!(!Action::Lock.confirm());
    assert!(Action::Reboot.confirm());
    assert!(Action::LogOut.confirm());
}

#[test]
fn inhibitors_are_limited_and_text_is_bounded() {
    let entries = (0..10)
        .map(|_| {
            (
                "sleep".to_owned(),
                "x".repeat(200),
                "reason".to_owned(),
                "block".to_owned(),
                0,
                0,
            )
        })
        .collect();
    let listed = inhibitors(entries);
    assert_eq!(listed.len(), INHIBITOR_MAX);
    assert!(listed[0].who.ends_with('…'));
}

#[test]
fn delay_inhibitors_are_kept_alongside_blocks() {
    let listed = inhibitors(vec![
        (
            "sleep".to_owned(),
            "player".to_owned(),
            "video".to_owned(),
            "delay".to_owned(),
            1000,
            1,
        ),
        (
            "shutdown".to_owned(),
            "updater".to_owned(),
            "install".to_owned(),
            "block".to_owned(),
            1000,
            2,
        ),
        (
            "idle".to_owned(),
            "other".to_owned(),
            "ignore".to_owned(),
            "none".to_owned(),
            1000,
            3,
        ),
    ]);
    assert_eq!(listed.len(), 2);
    assert!(listed[0].blocks("sleep"));
    assert!(listed[1].blocks("shutdown"));
}

#[test]
fn inhibitors_strip_hostile_text() {
    let listed = inhibitors(vec![(
        "sleep".to_owned(),
        "editor\u{202e}gpj.exe".to_owned(),
        "save\nwork".to_owned(),
        "block".to_owned(),
        1000,
        42,
    )]);

    assert_eq!(listed[0].who, "editor gpj.exe");
    assert_eq!(listed[0].why, "save work");
}

#[test]
fn a_window_count_does_not_clobber_logind_fields() {
    let mut state = SessionActionsState {
        user: Some("sam".into()),
        hibernate: Capability::Available,
        ..Default::default()
    };
    apply_logind(
        &mut state,
        LogindSnapshot {
            user: Some("sam".into()),
            signed_in_seconds: Some(12),
            inhibitors: Vec::new(),
            suspend: Capability::Available,
            hibernate: Capability::Hidden,
            reboot: Capability::Available,
            power_off: Capability::Available,
        },
    );
    state.windows = Some(4);
    assert_eq!(state.user.as_deref(), Some("sam"));
    assert_eq!(state.windows, Some(4));
    assert_eq!(state.hibernate, Capability::Hidden);
}

fn unit(name: &str, active_state: &str, before: &[&str]) -> BoundUnit {
    BoundUnit {
        name: name.to_owned(),
        active_state: active_state.to_owned(),
        before: before.iter().map(|target| (*target).to_owned()).collect(),
    }
}

#[test]
fn an_active_compositor_service_ordered_before_the_target_stops_it() {
    assert!(stops_graphical_session(&[unit(
        "niri.service",
        "active",
        &["graphical-session.target", "xdg-desktop-autostart.target"]
    )]));
}

#[test]
fn a_uwsm_compositor_unit_stops_the_target() {
    assert!(stops_graphical_session(&[unit(
        "wayland-wm@hyprland.desktop.service",
        "active",
        &["graphical-session.target"]
    )]));
}

#[test]
fn a_sidecar_service_alone_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "xwayland-satellite.service",
        "active",
        &[]
    )]));
}

#[test]
fn a_compositor_beside_a_sidecar_stops_the_target() {
    assert!(stops_graphical_session(&[
        unit("xwayland-satellite.service", "active", &[]),
        unit("niri.service", "active", &["graphical-session.target"]),
    ]));
}

#[test]
fn an_activating_compositor_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "niri.service",
        "activating",
        &["graphical-session.target"]
    )]));
}

#[test]
fn a_bound_target_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[unit(
        "hyprland-session.target",
        "active",
        &["graphical-session.target"]
    )]));
}

#[test]
fn nothing_bound_does_not_stop_the_target() {
    assert!(!stops_graphical_session(&[]));
}
