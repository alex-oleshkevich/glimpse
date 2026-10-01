use super::*;

#[tokio::test]
async fn a_signal_from_before_the_daemon_restarted_is_dropped() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::NameOwner(Some(":1.42".to_owned()))),
        )
        .await;
    service
        .handle(
            &ctx,
            Input::Event(Event::InterfacesRemoved {
                path: HEADSET.to_owned(),
                interfaces: vec![bluez::DEVICE1.to_owned()],
            }),
        )
        .await;

    assert!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .is_some(),
        "a queued signal from the previous owner must not edit what the next enumeration will \
             deliver"
    );
}

#[tokio::test]
async fn an_invalidated_property_is_forgotten_rather_than_kept() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(NEARBY.to_owned()))
            .and_then(|device| device.rssi),
        Some(-73)
    );

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: NEARBY.to_owned(),
                interface: bluez::DEVICE1.to_owned(),
                changed: properties(vec![]),
                invalidated: vec!["RSSI".to_owned()],
            }),
        )
        .await;

    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(NEARBY.to_owned()))
            .and_then(|device| device.rssi),
        None,
        "a stale signal strength would keep ordering the list it no longer describes"
    );
}
