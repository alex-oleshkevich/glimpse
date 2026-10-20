use super::*;

#[test]
fn only_applets_seen_running_can_end() {
    let mut registry = AttachedRegistry::default();
    registry.applets.push(crate::attached::AttachedApplet {
        handle: "me.x.A#1".to_owned(),
        sender: ":1.1".to_owned(),
        desktop_id: "me.x.A".to_owned(),
        dev_key: "me.x.A".to_owned(),
        panel: 0,
        output: String::new(),
        zone: glimpse_services::Zone::Right,
        stop: tokio_util::sync::CancellationToken::new(),
    });
    let mut state = ExecState::default();
    let mut seen = std::collections::BTreeSet::new();
    assert!(ended_dev(&state, &registry, &mut seen).is_empty());
    state.dev.insert("me.x.A".to_owned());
    assert!(ended_dev(&state, &registry, &mut seen).is_empty());
    state.dev.clear();
    assert_eq!(ended_dev(&state, &registry, &mut seen), ["me.x.A#1"]);
}

#[test]
fn a_reattached_applet_is_not_ended_by_its_predecessor() {
    let applet = |handle: &str| crate::attached::AttachedApplet {
        handle: handle.to_owned(),
        sender: ":1.1".to_owned(),
        desktop_id: "me.x.A".to_owned(),
        dev_key: "me.x.A".to_owned(),
        panel: 0,
        output: String::new(),
        zone: glimpse_services::Zone::Right,
        stop: tokio_util::sync::CancellationToken::new(),
    };
    let mut registry = AttachedRegistry::default();
    registry.applets.push(applet("me.x.A#1"));
    let mut state = ExecState::default();
    state.dev.insert("me.x.A".to_owned());
    let mut seen = std::collections::BTreeSet::new();
    assert!(ended_dev(&state, &registry, &mut seen).is_empty());
    registry.applets.clear();
    registry.applets.push(applet("me.x.A#2"));
    assert!(ended_dev(&state, &registry, &mut seen).is_empty());
    assert_eq!(seen.iter().collect::<Vec<_>>(), ["me.x.A#2"]);
    state.dev.clear();
    assert_eq!(ended_dev(&state, &registry, &mut seen), ["me.x.A#2"]);
}

#[test]
fn session_dialog_accepts_only_the_destructive_response() {
    assert!(!session_accepted("cancel"));
    assert!(!session_accepted("close"));
    assert!(session_accepted("accept"));
}

#[test]
fn bar_wire_retains_empty_surfaces_and_their_configured_indices() {
    let mut config = Config::default();
    let mut empty = glimpse_config::Panel::default();
    empty.left.clear();
    empty.center.clear();
    empty.right.clear();
    empty.position = glimpse_config::Position::Left;
    config.panels = vec![glimpse_config::Panel::default(), empty];
    let keys = [
        Key {
            index: 1,
            monitor: "DP-2".into(),
        },
        Key {
            index: 1,
            monitor: "DP-1".into(),
        },
    ];
    let state = bars_state(&config, keys.iter());
    assert_eq!(state.type_().as_str(), "a(uss)");
    assert_eq!(
        state.get::<Vec<(u32, String, String)>>().unwrap(),
        [
            (1, "DP-2".into(), "left".into()),
            (1, "DP-1".into(), "left".into())
        ]
    );
    let empty = bars_state(&config, std::iter::empty());
    assert_eq!(empty.type_().as_str(), "a(uss)");
    assert!(
        empty
            .get::<Vec<(u32, String, String)>>()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "needs a display"]
fn a_closed_prompt_dialog_is_not_open() {
    gtk4::init().expect("display");
    adw::init().expect("adwaita");
    glimpse_widgets::register_resources().expect("resources");

    let dialog = PairingDialog::new();
    assert!(!is_open(&dialog));
    dialog.present(None::<&gtk4::Widget>);
    assert!(is_open(&dialog));
    dialog.force_close();
    assert!(!is_open(&dialog));
}
