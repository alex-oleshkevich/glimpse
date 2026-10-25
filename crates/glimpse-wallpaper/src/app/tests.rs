use super::*;

#[test]
fn backdrop_is_created_only_where_the_compositor_has_an_overview_backdrop() {
    assert!(wants_role(Role::Wallpaper, false));
    assert!(wants_role(Role::Wallpaper, true));
    assert!(!wants_role(Role::Backdrop, false));
    assert!(wants_role(Role::Backdrop, true));
}

fn served(capabilities: Vec<Capability>) -> CompositorState {
    CompositorState {
        capabilities,
        unavailable: None,
        ..Default::default()
    }
}

#[test]
fn backdrop_turns_on_once_when_a_provider_with_overview_backdrop_appears() {
    let mut last = None;
    let provider = served(vec![Capability::OverviewBackdrop]);
    assert_eq!(backdrop(&mut last, &provider), Some(true));
    assert_eq!(backdrop(&mut last, &provider), None);
}

#[test]
fn backdrop_stays_off_for_a_compositor_without_overview_backdrop() {
    let mut last = None;
    let hyprland = served(Vec::new());
    assert_eq!(backdrop(&mut last, &hyprland), Some(false));
    assert_eq!(backdrop(&mut last, &hyprland), None);
}

#[test]
fn backdrop_keeps_its_value_across_a_provider_restart() {
    let mut last = None;
    let provider = served(vec![Capability::OverviewBackdrop]);
    assert_eq!(backdrop(&mut last, &provider), Some(true));
    let lost = CompositorState {
        unavailable: Some("provider has no bus owner".to_owned()),
        ..Default::default()
    };
    assert_eq!(backdrop(&mut last, &lost), None);
    assert_eq!(backdrop(&mut last, &provider), None);
}

#[test]
fn backdrop_waits_for_a_provider_without_emitting() {
    let mut last = None;
    let pending = CompositorState {
        unavailable: Some("waiting for the provider".to_owned()),
        ..Default::default()
    };
    assert_eq!(backdrop(&mut last, &pending), None);
    assert_eq!(last, None);
}
