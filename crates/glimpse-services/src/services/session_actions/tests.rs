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
