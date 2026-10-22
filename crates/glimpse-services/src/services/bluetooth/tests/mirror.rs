use super::*;

#[tokio::test]
async fn a_connected_device_with_no_battery_interface_publishes_none() {
    let (mut service, ctx, state, _health) = bluetooth().await;

    enumerated(&mut service, &ctx, session()).await;

    let headset = state.borrow();
    let headset = headset
        .device(&DeviceId(HEADSET.to_owned()))
        .expect("the headset");
    assert!(headset.connected);
    assert_eq!(
        headset.battery, None,
        "a connected WH-1000XM4 carried no Battery1 at all; this is the ordinary case"
    );
}

#[tokio::test]
async fn a_blocking_command_publishes_busy_and_does_not_hold_the_handler() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    let (reply, mut outcome) = oneshot::channel();

    service.spawned(
        &ctx,
        DeviceId(HEADSET.to_owned()),
        Busy::Connecting,
        Action::Connect,
        reply,
        std::future::pending(),
    );

    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.busy),
        Some(Busy::Connecting),
        "the row shows a pending state before the call returns"
    );
    assert!(
        outcome.try_recv().is_err(),
        "the reply is still typed and still outstanding"
    );
}

#[tokio::test]
async fn an_absent_adapter_publishes_nothing_and_stays_running() {
    let (mut service, ctx, state, health) = bluetooth().await;

    enumerated(&mut service, &ctx, BTreeMap::new()).await;

    assert_eq!(*state.borrow(), BluetoothState::default());
    assert!(matches!(&*health.borrow(), crate::ServiceState::Running));
}

#[tokio::test]
async fn a_lost_bus_degrades_and_keeps_the_last_state() {
    let (mut service, ctx, state, health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::Unavailable("no system bus".to_owned())),
        )
        .await;

    assert_eq!(state.borrow().devices.len(), 2);
    assert!(matches!(
        &*health.borrow(),
        crate::ServiceState::Degraded { .. }
    ));
}

#[tokio::test]
async fn one_properties_subscription_serves_every_device() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    let mut objects = session();
    for index in 0..17u8 {
        objects.insert(
            format!("/org/bluez/hci0/dev_00_00_00_00_00_{index:02X}"),
            nearby(),
        );
    }

    enumerated(&mut service, &ctx, objects).await;

    let declared = service.subscriptions();
    let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();
    assert_eq!(
        keys,
        vec![
            &Watch::NameOwner,
            &Watch::Objects(0),
            &Watch::Properties(0),
            &Watch::Disconnects(0)
        ]
    );
}

#[tokio::test]
async fn regaining_the_name_restarts_the_object_sources_but_not_the_name_watch() {
    let (mut service, ctx, _state, _health) = bluetooth().await;

    service
        .handle(
            &ctx,
            Input::Event(Event::NameOwner(Some(":1.42".to_owned()))),
        )
        .await;

    let declared = service.subscriptions();
    let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();
    assert_eq!(
        keys,
        vec![
            &Watch::NameOwner,
            &Watch::Objects(1),
            &Watch::Properties(1),
            &Watch::Disconnects(1)
        ]
    );
}

#[tokio::test]
async fn a_partial_change_keeps_what_it_does_not_carry() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: HEADSET.to_owned(),
                interface: bluez::DEVICE1.to_owned(),
                changed: properties(vec![("Connected", false.into())]),
                invalidated: Vec::new(),
            }),
        )
        .await;

    let published = state.borrow();
    let headset = published
        .device(&DeviceId(HEADSET.to_owned()))
        .expect("the headset");
    assert_eq!(headset.name, "WH-1000XM4");
    assert!(!headset.connected);
    assert!(headset.bonded);
}

#[tokio::test]
async fn a_battery_interface_reaches_a_device_that_already_exists() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::InterfacesAdded {
                path: HEADSET.to_owned(),
                interfaces: HashMap::from([(
                    bluez::BATTERY1.to_owned(),
                    properties(vec![("Percentage", 80u8.into())]),
                )]),
            }),
        )
        .await;
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.battery),
        Some(80)
    );

    service
        .handle(
            &ctx,
            Input::Event(Event::InterfacesRemoved {
                path: HEADSET.to_owned(),
                interfaces: vec![bluez::BATTERY1.to_owned()],
            }),
        )
        .await;
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.battery),
        None
    );
}

#[tokio::test]
async fn a_removed_device_leaves_the_list() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::InterfacesRemoved {
                path: NEARBY.to_owned(),
                interfaces: vec![bluez::DEVICE1.to_owned()],
            }),
        )
        .await;

    assert_eq!(state.borrow().devices.len(), 1);
}

#[tokio::test]
async fn an_adapter_without_power_state_still_reports_its_power() {
    let (mut service, ctx, state, _health) = bluetooth().await;

    enumerated(
        &mut service,
        &ctx,
        BTreeMap::from([(
            ADAPTER.to_owned(),
            HashMap::from([(
                bluez::ADAPTER1.to_owned(),
                properties(vec![("Powered", false.into())]),
            )]),
        )]),
    )
    .await;

    assert_eq!(
        state.borrow().adapter.as_ref().map(|adapter| adapter.power),
        Some(Power::Off)
    );
}
