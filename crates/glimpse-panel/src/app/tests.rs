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

fn panel(monitor: Option<&str>, right: &[&str]) -> glimpse_config::Panel {
    glimpse_config::Panel {
        monitor: monitor.map(str::to_owned),
        left: Vec::new(),
        center: Vec::new(),
        right: right.iter().map(|name| (*name).to_owned()).collect(),
        ..Default::default()
    }
}

fn on(index: usize, monitor: &str) -> Key {
    Key {
        index,
        monitor: monitor.into(),
    }
}

#[test]
fn a_renamed_network_applet_on_a_mapped_bar_answers_the_secret_request() {
    let config = Config {
        applets: [(
            "wifi".to_owned(),
            glimpse_config::AppletKind::Network(Default::default()).into(),
        )]
        .into(),
        panels: vec![panel(None, &["wifi"])],
        ..Config::default()
    };
    assert!(network_on_a_bar(&config, [on(0, "DP-1")].iter()));
}

#[test]
fn a_network_applet_only_on_an_unconnected_monitors_panel_leaves_the_host_to_ask() {
    let config = Config {
        panels: vec![
            panel(Some("DP-1"), &["clock"]),
            panel(Some("DP-9"), &["network"]),
        ],
        ..Config::default()
    };
    assert!(!network_on_a_bar(&config, [on(0, "DP-1")].iter()));
    assert!(network_on_a_bar(
        &config,
        [on(0, "DP-1"), on(1, "DP-9")].iter()
    ));
}

#[test]
fn no_panel_listing_network_leaves_the_host_to_ask() {
    let config = Config {
        panels: vec![panel(None, &["clock"])],
        ..Config::default()
    };
    assert!(!network_on_a_bar(&config, [on(0, "DP-1")].iter()));
    assert!(!network_on_a_bar(&config, std::iter::empty()));
}
