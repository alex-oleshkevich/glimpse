use super::*;

#[tokio::test]
async fn only_a_pairing_this_service_was_waiting_on_trusts_and_connects() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    let settled = |busy| {
        Input::Event(Event::Settled {
            id: DeviceId(HEADSET.to_owned()),
            busy,
            failure: None,
        })
    };

    service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Connecting);
    service.handle(&ctx, settled(Busy::Pairing)).await;
    assert!(
        service.devices.get(HEADSET).expect("the headset").busy == Some(Busy::Connecting),
        "a pairing that answers a record the service is no longer waiting on must change \
             nothing — not the busy state it did not set, and not trust"
    );

    service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Pairing);
    service.handle(&ctx, settled(Busy::Pairing)).await;
    assert_eq!(
        service.devices.get(HEADSET).expect("the headset").busy,
        None,
        "the pairing it was waiting on settles the record"
    );
}

#[tokio::test]
async fn making_the_adapter_discoverable_reaches_the_bus_rather_than_the_state() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    assert!(
        !state
            .borrow()
            .adapter
            .as_ref()
            .expect("an adapter")
            .discoverable,
        "the adapter publishes what BlueZ says, and the fixture says it is not discoverable"
    );

    let (reply, outcome) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::SetDiscoverable { on: true, reply }),
        )
        .await;

    assert!(
        matches!(
            outcome.await,
            Ok(Err(BluetoothError::Service(CommandError::Unavailable(_))))
        ),
        "with no bus the command is refused rather than pretending the adapter changed"
    );
    assert!(
        !state
            .borrow()
            .adapter
            .as_ref()
            .expect("an adapter")
            .discoverable,
        "and nothing moves optimistically"
    );
}

#[tokio::test]
async fn the_adapter_going_discoverable_reaches_the_published_state() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: ADAPTER.to_owned(),
                interface: bluez::ADAPTER1.to_owned(),
                changed: properties(vec![("Discoverable", true.into())]),
                invalidated: Vec::new(),
            }),
        )
        .await;

    assert!(
        state
            .borrow()
            .adapter
            .as_ref()
            .expect("an adapter")
            .discoverable,
        "a merge that drops the flag leaves the switch claiming the machine is invisible"
    );
}

#[tokio::test]
async fn a_successful_command_clears_the_failure_the_last_one_left() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    service.devices.get_mut(HEADSET).expect("the headset").busy = Some(Busy::Connecting);
    service
        .devices
        .get_mut(HEADSET)
        .expect("the headset")
        .failure = Some(Failure::Unreachable);

    service
        .handle(
            &ctx,
            Input::Event(Event::Settled {
                id: DeviceId(HEADSET.to_owned()),
                busy: Busy::Connecting,
                failure: None,
            }),
        )
        .await;

    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.failure),
        None
    );
}
