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

struct FakeKbdBacklight;

#[zbus::interface(name = "org.freedesktop.UPower.KbdBacklight")]
impl FakeKbdBacklight {
    fn get_brightness(&self) -> i32 {
        1
    }

    fn get_max_brightness(&self) -> i32 {
        3
    }

    #[zbus(signal)]
    async fn brightness_changed_with_source(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        value: i32,
        source: String,
    ) -> zbus::Result<()>;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_keyboard_watch_follows_upower_that_appears_after_start() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let monitor = bus.connection().await;
    let mut messages = zbus::MessageStream::from(&monitor);
    zbus::fdo::MonitoringProxy::new(&monitor)
        .await
        .unwrap()
        .become_monitor(&[], 0)
        .await
        .unwrap();
    let (events, _inbox) = mpsc::channel(32);
    let cancel = CancellationToken::new();
    let (health, _health) = watch::channel(ServiceState::Starting);
    let (published, _state) = watch::channel(BrightnessState::default());
    let ctx = Ctx::<Brightness>::new(
        events,
        &cancel,
        published,
        health,
        Buses::for_system(bus.connection().await),
    );
    let backend: Arc<dyn Backlight> =
        Arc::new(FakeBacklight::new(vec![Entry::keyboard(KEYBOARD_ID, 1, 3)]));
    let mut watcher = tokio::spawn(async move {
        let mut events = keyboard_watch(ctx, backend).await;
        (events.next().await, events.next().await)
    });
    let failed = keyboard_bus_reply(&mut messages, "GetMaxBrightness", None).await;
    assert_eq!(failed.message_type(), zbus::message::Type::Error);

    let upower = bus.connection().await;
    upower
        .object_server()
        .at("/org/freedesktop/UPower/KbdBacklight", FakeKbdBacklight)
        .await
        .unwrap();
    glimpse_dbus::own_name(&upower, "org.freedesktop.UPower")
        .await
        .unwrap();
    let emitter =
        zbus::object_server::SignalEmitter::new(&upower, "/org/freedesktop/UPower/KbdBacklight")
            .unwrap();

    tokio::select! {
        reply = keyboard_bus_reply(&mut messages, "AddMatch", Some("BrightnessChangedWithSource")) => {
            assert_eq!(reply.message_type(), zbus::message::Type::MethodReturn);
        }
        _ = &mut watcher => panic!("the watch stopped before subscribing"),
    }
    FakeKbdBacklight::brightness_changed_with_source(&emitter, 2, String::new())
        .await
        .unwrap();
    let (first, second) = tokio::time::timeout(std::time::Duration::from_secs(10), watcher)
        .await
        .expect("the watch delivers an enumeration and the change")
        .unwrap();

    assert!(
        matches!(&first, Some(Event::Enumerated(found)) if found.entries.iter().any(|entry| entry.id == KEYBOARD_ID)),
        "a keyboard that appears after start is enumerated, since a rescan of an unknown id is dropped"
    );
    assert!(matches!(second, Some(Event::Rescanned { .. })));
}

async fn keyboard_bus_reply(
    messages: &mut zbus::MessageStream,
    member: &str,
    match_contains: Option<&str>,
) -> zbus::Message {
    let request = loop {
        let message = messages.next().await.unwrap().unwrap();
        if message.message_type() == zbus::message::Type::MethodCall
            && message
                .header()
                .member()
                .is_some_and(|name| name.as_str() == member)
            && match_contains.is_none_or(|needle| {
                message
                    .body()
                    .deserialize::<String>()
                    .is_ok_and(|rule| rule.contains(needle))
            })
        {
            break message;
        }
    };
    loop {
        let message = messages.next().await.unwrap().unwrap();
        if message.header().reply_serial() == Some(request.primary_header().serial_num())
            && message.header().destination().map(|name| name.as_str())
                == request.header().sender().map(|name| name.as_str())
        {
            return message;
        }
    }
}
