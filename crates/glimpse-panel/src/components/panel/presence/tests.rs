use super::{Hold, Presence, Step};

#[test]
fn hiding_a_pinned_panel_slides_out_before_it_releases_its_space() {
    let mut presence = Presence::new(true);
    assert_eq!(presence.pin(false), [Step::Settle, Step::SlideOut]);
    assert!(presence.reserved());
    assert_eq!(presence.slid(), [Step::Release, Step::Region]);
    assert!(!presence.reserved());
}

#[test]
fn showing_a_hidden_panel_reserves_its_space_before_it_slides_in() {
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
    assert!(!presence.reserved());
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
    assert_eq!(presence.pin(false), [Step::Settle, Step::SlideOut]);
    assert!(!presence.held(Hold::Command));
    assert!(!presence.held(Hold::Popover));
    assert!(!presence.held(Hold::Pointer));
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
fn a_reveal_during_a_slide_out_returns_without_releasing() {
    let mut presence = Presence::new(true);
    presence.pin(false);
    assert_eq!(
        presence.hold(Hold::Command, true),
        [Step::Settle, Step::SlideIn, Step::Region]
    );
    assert!(presence.slid().is_empty());
    assert!(presence.reserved());
    presence.hold(Hold::Command, false);
    assert_eq!(presence.expire(), [Step::SlideOut]);
    assert_eq!(presence.slid(), [Step::Release, Step::Region]);
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
