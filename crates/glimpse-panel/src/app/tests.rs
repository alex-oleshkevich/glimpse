use super::*;

#[test]
fn only_completed_offers_can_end() {
    let mut registry = AttachedRegistry::default();
    registry.applets.push(crate::attached::AttachedApplet {
        handle: "dev-1".to_owned(),
        sender: ":1.1".to_owned(),
        desktop_id: "me.x.A".to_owned(),
        panel: 0,
        output: String::new(),
        zone: glimpse_services::Zone::Right,
        offered: false,
    });
    let mut state = ExecState::default();
    assert!(ended_offers(&state, &registry).is_empty());
    registry.applets[0].offered = true;
    assert_eq!(ended_offers(&state, &registry), ["dev-1"]);
    state.offers.insert("me.x.A".to_owned());
    assert!(ended_offers(&state, &registry).is_empty());
}

#[test]
fn session_dialog_accepts_only_the_destructive_response() {
    assert!(!session_accepted("cancel"));
    assert!(!session_accepted("close"));
    assert!(session_accepted("accept"));
}
