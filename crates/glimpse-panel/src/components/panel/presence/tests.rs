use super::{Hold, Presence, Step};

#[test]
fn hiding_a_pinned_panel_releases_its_space_as_it_slides_out() {
    let mut presence = Presence::new(true);
    assert_eq!(
        presence.pin(false),
        [Step::Settle, Step::Release, Step::SlideOut]
    );
    assert!(!presence.pinned());
    assert!(!presence.visible());
    assert_eq!(presence.slid(), [Step::Region]);
}

#[test]
fn showing_a_hidden_panel_reserves_its_space_as_it_slides_in() {
    let mut presence = Presence::new(false);
    assert_eq!(
        presence.pin(true),
        [Step::Settle, Step::Reserve, Step::SlideIn, Step::Region]
    );
    assert!(presence.visible());
}

#[test]
fn a_reveal_overlays_without_reserving_and_lingers_after_its_last_hold() {
    let mut presence = Presence::new(false);
    assert_eq!(
        presence.hold(Hold::Command, true),
        [Step::Settle, Step::SlideIn, Step::Region]
    );
    assert!(!presence.pinned());
    assert_eq!(presence.hold(Hold::Pointer, true), [Step::Settle]);
    assert!(presence.hold(Hold::Command, false).is_empty());
    assert_eq!(presence.hold(Hold::Pointer, false), [Step::Linger]);
    assert_eq!(presence.expire(), [Step::SlideOut]);
    assert_eq!(presence.slid(), [Step::Region]);
}

#[test]
fn a_hold_arriving_during_the_linger_keeps_the_panel() {
    let mut presence = Presence::new(false);
    presence.hold(Hold::Pointer, true);
    assert_eq!(presence.hold(Hold::Pointer, false), [Step::Linger]);
    assert_eq!(presence.hold(Hold::Popover, true), [Step::Settle]);
    assert!(presence.expire().is_empty());
    assert!(presence.visible());
}

#[test]
fn hide_drops_every_hold_so_the_panel_always_leaves() {
    let mut presence = Presence::new(true);
    presence.hold(Hold::Command, true);
    presence.hold(Hold::Popover, true);
    presence.hold(Hold::Pointer, true);
    assert_eq!(
        presence.pin(false),
        [Step::Settle, Step::Release, Step::SlideOut]
    );
    assert_eq!(
        presence.hold(Hold::Command, true),
        [Step::Settle, Step::SlideIn, Step::Region]
    );
    assert_eq!(presence.hold(Hold::Popover, true), [Step::Settle]);
    assert_eq!(presence.hold(Hold::Pointer, true), [Step::Settle]);
}

#[test]
fn pinning_a_revealed_panel_only_reserves() {
    let mut presence = Presence::new(false);
    presence.hold(Hold::Command, true);
    assert_eq!(presence.pin(true), [Step::Settle, Step::Reserve]);
    assert!(presence.hold(Hold::Command, false).is_empty());
    assert!(presence.expire().is_empty());
}

#[test]
fn a_reveal_during_a_slide_out_returns_as_an_overlay() {
    let mut presence = Presence::new(true);
    presence.pin(false);
    assert_eq!(
        presence.hold(Hold::Command, true),
        [Step::Settle, Step::SlideIn, Step::Region]
    );
    assert!(presence.slid().is_empty());
    assert!(!presence.pinned());
    assert_eq!(presence.hold(Hold::Command, false), [Step::Linger]);
    assert_eq!(presence.expire(), [Step::SlideOut]);
    assert_eq!(presence.slid(), [Step::Region]);
}

#[test]
fn a_command_hold_mid_slide_out_slides_in_without_reserving() {
    let mut presence = Presence::new(true);
    assert_eq!(
        presence.pin(false),
        [Step::Settle, Step::Release, Step::SlideOut]
    );
    let steps = presence.hold(Hold::Command, true);
    assert_eq!(steps, [Step::Settle, Step::SlideIn, Step::Region]);
    assert!(!steps.contains(&Step::Reserve));
    assert!(!presence.pinned());
    assert_eq!(presence.pin(true), [Step::Settle, Step::Reserve]);
    assert!(presence.pinned());
}

#[test]
fn repeated_inputs_change_nothing() {
    let mut presence = Presence::new(false);
    assert!(presence.pin(false).is_empty());
    assert!(presence.hold(Hold::Popover, false).is_empty());
    assert!(presence.expire().is_empty());
    presence.hold(Hold::Popover, true);
    assert!(presence.hold(Hold::Popover, true).is_empty());
}
