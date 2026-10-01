use super::*;

#[tokio::test]
async fn a_local_disconnect_leaves_no_failure_and_a_timeout_does() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::Disconnected {
                path: HEADSET.to_owned(),
                reason: DisconnectReason::Local,
            }),
        )
        .await;
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.failure),
        None,
        "the user pressed disconnect; there is nothing to report"
    );

    service
        .handle(
            &ctx,
            Input::Event(Event::Disconnected {
                path: HEADSET.to_owned(),
                reason: DisconnectReason::Timeout,
            }),
        )
        .await;
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.failure),
        Some(Failure::Dropped)
    );
}

#[tokio::test]
async fn a_failure_is_cleared_by_the_device_connecting() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    service
        .handle(
            &ctx,
            Input::Event(Event::Disconnected {
                path: HEADSET.to_owned(),
                reason: DisconnectReason::Remote,
            }),
        )
        .await;

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: HEADSET.to_owned(),
                interface: bluez::DEVICE1.to_owned(),
                changed: properties(vec![("Connected", true.into())]),
                invalidated: Vec::new(),
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
