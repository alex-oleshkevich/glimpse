use super::*;

#[tokio::test]
async fn an_unconfirmed_forget_executes_nothing_and_asks_instead() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    let outcome = command(&mut service, &ctx, |reply| Command::Forget {
        id: DeviceId(HEADSET.to_owned()),
        confirmed: false,
        reply,
    })
    .await;

    assert!(outcome.is_ok());
    assert_eq!(
        state.borrow().confirm,
        Some(Confirmation::Forget {
            device: DeviceId(HEADSET.to_owned()),
            connected: true,
        })
    );
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.busy),
        None,
        "a confirmed action cannot be optimistic: nothing may move before the answer"
    );
}

#[tokio::test]
async fn an_unconfirmed_forget_of_a_disconnected_device_says_so() {
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

    let _ = command(&mut service, &ctx, |reply| Command::Forget {
        id: DeviceId(HEADSET.to_owned()),
        confirmed: false,
        reply,
    })
    .await;

    assert_eq!(
        state.borrow().confirm,
        Some(Confirmation::Forget {
            device: DeviceId(HEADSET.to_owned()),
            connected: false,
        })
    );
}

#[tokio::test]
async fn connect_disconnect_and_the_power_toggle_never_confirm() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    for build in [
        &(|reply| Command::Connect {
            id: DeviceId(HEADSET.to_owned()),
            reply,
        }) as &dyn Fn(Reply) -> Command,
        &|reply| Command::Disconnect {
            id: DeviceId(HEADSET.to_owned()),
            reply,
        },
        &|reply| Command::SetPowered {
            powered: false,
            reply,
        },
    ] {
        let (reply, _outcome) = oneshot::channel();
        service.handle(&ctx, Input::Command(build(reply))).await;
        assert_eq!(state.borrow().confirm, None);
    }
}

#[tokio::test]
async fn dismissing_a_confirmation_clears_it_and_calls_nothing() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    let _ = command(&mut service, &ctx, |reply| Command::Forget {
        id: DeviceId(HEADSET.to_owned()),
        confirmed: false,
        reply,
    })
    .await;

    let outcome = command(&mut service, &ctx, |reply| Command::DismissConfirmation {
        reply,
    })
    .await;

    assert!(outcome.is_ok());
    assert_eq!(state.borrow().confirm, None);
    assert_eq!(state.borrow().devices.len(), 2, "nothing was forgotten");
}

#[tokio::test]
async fn a_confirmed_forget_clears_the_confirmation_and_reaches_the_backend() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    let _ = command(&mut service, &ctx, |reply| Command::Forget {
        id: DeviceId(HEADSET.to_owned()),
        confirmed: false,
        reply,
    })
    .await;

    let outcome = command(&mut service, &ctx, |reply| Command::Forget {
        id: DeviceId(HEADSET.to_owned()),
        confirmed: true,
        reply,
    })
    .await;

    assert_eq!(state.borrow().confirm, None);
    assert!(
        matches!(
            outcome,
            Err(BluetoothError::Service(CommandError::Unavailable(_)))
        ),
        "it went to the backend, which is unreachable in tests"
    );
}
