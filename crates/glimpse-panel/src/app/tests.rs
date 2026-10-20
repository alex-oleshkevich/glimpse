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

fn bars(monitors: &[(usize, &str)]) -> Vec<Key> {
    monitors
        .iter()
        .map(|(index, monitor)| Key {
            index: *index,
            monitor: (*monitor).to_owned(),
        })
        .collect()
}

fn with_network(panels: &[bool]) -> Config {
    Config {
        panels: panels
            .iter()
            .map(|network| glimpse_config::Panel {
                right: if *network {
                    vec!["network".to_owned()]
                } else {
                    Vec::new()
                },
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn a_prompt_raises_on_exactly_one_hosting_bar() {
    let keys = bars(&[(0, "DP-1"), (0, "DP-2"), (1, "DP-1"), (1, "DP-2")]);
    let both = with_network(&[true, true]);
    let target = |config: &Config, focused| raise_target(config, keys.iter(), "network", focused);

    assert_eq!(target(&both, Some("DP-2")), Some(1));
    assert_eq!(target(&both, Some("DP-1")), Some(0));
    assert_eq!(target(&both, None), Some(0));
    assert_eq!(target(&both, Some("HDMI-1")), Some(0));
    assert_eq!(target(&with_network(&[false, true]), Some("DP-1")), Some(2));
    assert_eq!(target(&with_network(&[false, false]), Some("DP-1")), None);
}
