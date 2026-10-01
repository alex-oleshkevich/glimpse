use super::*;

#[tokio::test]
async fn busy_is_cleared_by_the_command_completing_and_never_by_the_property() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Disconnecting);
    service.publish();

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
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.busy),
        Some(Busy::Disconnecting),
        "a backend that answers success without moving the property would strand the row"
    );

    service
        .handle(
            &ctx,
            Input::Event(Event::Settled {
                id: DeviceId(HEADSET.to_owned()),
                busy: Busy::Disconnecting,
                failure: None,
            }),
        )
        .await;
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.busy),
        None
    );
}

#[tokio::test]
async fn a_settled_event_for_a_command_that_was_replaced_leaves_the_new_one_running() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Connecting);
    service.publish();

    service
        .handle(
            &ctx,
            Input::Event(Event::Settled {
                id: DeviceId(HEADSET.to_owned()),
                busy: Busy::Disconnecting,
                failure: None,
            }),
        )
        .await;

    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.busy),
        Some(Busy::Connecting)
    );
}

#[tokio::test]
async fn a_command_with_no_bus_is_refused_rather_than_queued() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::Connect {
                id: DeviceId(HEADSET.to_owned()),
                reply,
            }),
        )
        .await;

    assert!(matches!(
        result.await,
        Ok(Err(BluetoothError::Service(CommandError::Unavailable(_))))
    ));
}
