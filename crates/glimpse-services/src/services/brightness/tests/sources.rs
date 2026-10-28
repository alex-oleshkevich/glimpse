use super::*;

#[test]
fn a_card_symlink_target_yields_the_connector_after_the_first_dash() {
    assert_eq!(
        connector(Some("../../card1-eDP-1")),
        Some("eDP-1".to_owned())
    );
    assert_eq!(connector(Some("../../card0-DP-2")), Some("DP-2".to_owned()));
}

#[test]
fn a_backlight_with_no_device_link_still_produces_a_source() {
    let records = sources_from(vec![entry("ddcci1", "raw", 50, 100, None)]);

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].connector, None);
}

#[test]
fn several_backlights_on_the_same_connector_collapse_to_the_preferred_one() {
    let records = sources_from(vec![
        entry("amdgpu_bl1", "raw", 100, 100, Some("../../card1-eDP-1")),
        entry(
            "intel_backlight",
            "firmware",
            50,
            100,
            Some("../../card1-eDP-1"),
        ),
    ]);

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, "intel_backlight");
}

#[test]
fn the_floor_never_reaches_zero_through_rounding_on_a_small_max() {
    assert_eq!(floor(400_000, 3), 12_000);
    assert_eq!(floor(3, 3), 1);
    assert_eq!(floor(400_000, 0), 0);
}

#[test]
fn a_zero_max_brightness_floors_to_zero_rather_than_panicking() {
    assert_eq!(floor(0, 3), 0);
}

#[test]
fn several_backlights_are_ordered_by_type_preference() {
    let records = sources_from(vec![
        entry("raw1", "raw", 1, 1, None),
        entry("firmware1", "firmware", 1, 1, None),
        entry("platform1", "platform", 1, 1, None),
    ]);

    let ids: Vec<&str> = records.iter().map(|record| record.id.as_str()).collect();
    assert_eq!(ids, vec!["firmware1", "platform1", "raw1"]);
}

#[tokio::test]
async fn a_floor_at_a_device_maximum_is_reported_once_per_source() {
    let mut harness = harness(
        vec![
            entry("coarse", "raw", 1, 1, None),
            entry("fine", "raw", 50, 100, None),
        ],
        50,
    )
    .await;

    assert_eq!(harness.service.inert, HashSet::from(["coarse".to_owned()]));
    assert!(harness.service.newly_inert().is_empty());

    harness.feed(Input::Config(Config { minimum: 0 })).await;
    assert!(harness.service.inert.is_empty());

    harness.service.minimum = 50;
    assert_eq!(harness.service.newly_inert(), vec!["coarse".to_owned()]);
    assert!(harness.service.newly_inert().is_empty());
}
