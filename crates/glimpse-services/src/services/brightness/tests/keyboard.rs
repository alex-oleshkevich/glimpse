use super::*;

#[test]
fn only_the_kbd_backlight_led_passes_the_filter() {
    let names = [
        "input17::capslock",
        "input17::compose",
        "input17::kana",
        "input17::numlock",
        "input17::scrolllock",
        "input35::capslock",
        "input35::compose",
        "input35::kana",
        "input35::numlock",
        "input35::scrolllock",
        "platform::micmute",
    ];
    for name in names {
        assert!(
            !is_keyboard_backlight_led(name),
            "{name} should be excluded"
        );
    }
    assert!(is_keyboard_backlight_led("acme::kbd_backlight"));
    assert!(is_keyboard_backlight_led("acme:white:kbd_backlight"));
}

#[test]
fn keyboard_led_selection_is_sorted_by_name() {
    let leds = vec![
        led("zzz::kbd_backlight", 1, 3),
        led("aaa::kbd_backlight", 2, 3),
    ];

    let selected = select_keyboard_led(leds).expect("a led is selected");

    assert_eq!(selected.name, "aaa::kbd_backlight");
}

#[test]
fn a_keyboard_signal_clamps_a_negative_value_to_zero() {
    let event = keyboard_rescan_event(-5, 3);

    match event {
        Event::Rescanned { id, entry } => {
            assert_eq!(id, KEYBOARD_ID);
            assert_eq!(entry, Some(Entry::keyboard(KEYBOARD_ID, 0, 3)));
        }
        _ => panic!("expected a Rescanned event"),
    }
}

#[test]
fn a_keyboard_signal_carries_the_max_brightness_from_discovery() {
    let event = keyboard_rescan_event(2, 3);

    match event {
        Event::Rescanned { id, entry } => {
            assert_eq!(id, KEYBOARD_ID);
            assert_eq!(entry, Some(Entry::keyboard(KEYBOARD_ID, 2, 3)));
        }
        _ => panic!("expected a Rescanned event"),
    }
}

#[test]
fn the_keyboard_floor_is_always_zero() {
    assert_eq!(floor_for(Kind::Keyboard, 3, 50), 0);
    assert_eq!(floor_for(Kind::Display, 3, 50), floor(3, 50));
}

#[tokio::test]
async fn a_keyboard_source_can_be_set_all_the_way_to_zero() {
    let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 2, 3)], 50).await;
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: KEYBOARD_ID.to_owned(),
            value: 0,
            reply,
        }))
        .await;
    harness.settle_next().await;

    assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 0)]);
    assert!(result.await.expect("answered").is_ok());
    assert_eq!(harness.state.borrow().sources[0].floor, 0);
}

#[tokio::test]
async fn a_keyboard_signal_reconciles_state_and_never_causes_a_second_write() {
    let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 1, 3)], 0).await;
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::SetLevel {
            id: KEYBOARD_ID.to_owned(),
            value: 3,
            reply,
        }))
        .await;
    harness.settle_next().await;
    assert!(result.await.expect("answered").is_ok());
    assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 3)]);

    harness
        .feed(Input::Event(Event::Rescanned {
            id: KEYBOARD_ID.to_owned(),
            entry: Some(Entry::keyboard(KEYBOARD_ID, 2, 3)),
        }))
        .await;

    assert_eq!(harness.state.borrow().sources[0].current, 2);
    assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 3)]);
}

#[tokio::test]
async fn no_upower_and_no_led_yields_no_keyboard_source_without_degrading_health() {
    let harness = harness(Vec::new(), 3).await;

    assert!(harness.state.borrow().sources.is_empty());
    assert!(!matches!(
        &*harness.health.borrow(),
        ServiceState::Degraded { .. }
    ));
}

#[tokio::test]
async fn adjust_level_with_a_negative_delta_lowers_a_keyboard_source_without_underflow() {
    let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 2, 3)], 50).await;
    let (reply, result) = oneshot::channel();

    harness
        .feed(Input::Command(Command::AdjustLevel {
            id: KEYBOARD_ID.to_owned(),
            delta: -10,
            reply,
        }))
        .await;
    harness.settle_next().await;

    assert_eq!(harness.backend.writes(), vec![(KEYBOARD_ID.to_owned(), 0)]);
    assert!(result.await.expect("answered").is_ok());
}

#[tokio::test]
async fn raising_the_minimum_does_not_lift_a_keyboard_off_zero() {
    let mut harness = harness(vec![Entry::keyboard(KEYBOARD_ID, 0, 3)], 3).await;

    harness.feed(Input::Config(Config { minimum: 50 })).await;

    assert!(harness.backend.writes().is_empty());
    assert_eq!(harness.state.borrow().sources[0].current, 0);
}
