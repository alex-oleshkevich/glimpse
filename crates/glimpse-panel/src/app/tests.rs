use super::*;

#[test]
fn only_applets_seen_running_can_end() {
    let mut registry = AttachedRegistry::default();
    registry.applets.push(crate::attached::AttachedApplet {
        handle: "me.x.A".to_owned(),
        sender: ":1.1".to_owned(),
        desktop_id: "me.x.A".to_owned(),
        panel: 0,
        output: String::new(),
        zone: glimpse_services::Zone::Right,
    });
    let mut state = ExecState::default();
    let mut seen = std::collections::BTreeSet::new();
    assert!(ended_dev(&state, &registry, &mut seen).is_empty());
    state.dev.insert("me.x.A".to_owned());
    assert!(ended_dev(&state, &registry, &mut seen).is_empty());
    state.dev.clear();
    assert_eq!(ended_dev(&state, &registry, &mut seen), ["me.x.A"]);
}

#[test]
fn session_dialog_accepts_only_the_destructive_response() {
    assert!(!session_accepted("cancel"));
    assert!(!session_accepted("close"));
    assert!(session_accepted("accept"));
}
