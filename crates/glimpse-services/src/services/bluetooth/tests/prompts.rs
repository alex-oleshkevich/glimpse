use super::*;

#[tokio::test]
async fn a_prompt_reaches_the_state_and_an_answer_clears_it() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    let (answer, reply) = oneshot::channel();

    service
        .handle(
            &ctx,
            Input::Event(Event::Prompt(
                Prompt::Confirm {
                    device: DeviceId(HEADSET.to_owned()),
                    passkey: 123_456,
                },
                Some(answer),
            )),
        )
        .await;
    assert!(state.borrow().pairing.is_some());

    let (sent, outcome) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::AnswerPairing {
                answer: Answer::Confirm,
                reply: sent,
            }),
        )
        .await;

    assert_eq!(state.borrow().pairing, None);
    assert!(matches!(outcome.await, Ok(Ok(()))));
    assert_eq!(reply.await, Ok(Answer::Confirm));
}

#[tokio::test]
async fn an_incoming_service_is_authorized_by_the_bond_and_by_nothing_else() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;

    for (path, allowed) in [
        (HEADSET, true),
        (NEARBY, false),
        ("/org/bluez/hci0/dev_0", false),
    ] {
        let (reply, answer) = oneshot::channel();
        service
            .handle(
                &ctx,
                Input::Event(Event::AuthorizeService {
                    device: DeviceId(path.to_owned()),
                    reply,
                }),
            )
            .await;

        assert_eq!(
            answer.await,
            Ok(allowed),
            "bluez asks this for an untrusted device, so refusing every one of them strands                  a pairing the user already made"
        );
    }
}

#[tokio::test]
async fn an_answer_with_nothing_waiting_is_refused() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    let (sent, outcome) = oneshot::channel();

    service
        .handle(
            &ctx,
            Input::Command(Command::AnswerPairing {
                answer: Answer::Confirm,
                reply: sent,
            }),
        )
        .await;

    assert!(matches!(
        outcome.await,
        Ok(Err(BluetoothError::Service(CommandError::Unavailable(_))))
    ));
}

#[tokio::test]
async fn a_second_prompt_replaces_the_first_and_cancels_its_call() {
    let (mut service, ctx, _state, _health) = bluetooth().await;
    let (first, canceled) = oneshot::channel();
    let (second, _still_open) = oneshot::channel();

    service
        .handle(
            &ctx,
            Input::Event(Event::Prompt(
                Prompt::Authorize(DeviceId(HEADSET.to_owned())),
                Some(first),
            )),
        )
        .await;
    service
        .handle(
            &ctx,
            Input::Event(Event::Prompt(
                Prompt::RequestPin(DeviceId(NEARBY.to_owned())),
                Some(second),
            )),
        )
        .await;

    assert!(
        canceled.await.is_err(),
        "the superseded call must not be left parked forever"
    );
}

#[tokio::test]
async fn a_bond_clears_the_prompt_that_only_told_the_user_what_to_type() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    service
        .handle(
            &ctx,
            Input::Event(Event::Prompt(
                Prompt::DisplayPasskey {
                    device: DeviceId(NEARBY.to_owned()),
                    passkey: 418_209,
                    entered: 3,
                },
                None,
            )),
        )
        .await;
    assert!(state.borrow().pairing.is_some());

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: NEARBY.to_owned(),
                interface: bluez::DEVICE1.to_owned(),
                changed: properties(vec![("Bonded", true.into()), ("Paired", true.into())]),
                invalidated: Vec::new(),
            }),
        )
        .await;

    assert_eq!(
        state.borrow().pairing,
        None,
        "bluez never cancels a display-only prompt, so the bond is what retires it"
    );
}

#[tokio::test]
async fn a_prompt_waiting_on_an_answer_is_not_retired_by_the_bond() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    let (answer, mut still_open) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Event(Event::Prompt(
                Prompt::Confirm {
                    device: DeviceId(NEARBY.to_owned()),
                    passkey: 418_209,
                },
                Some(answer),
            )),
        )
        .await;

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: NEARBY.to_owned(),
                interface: bluez::DEVICE1.to_owned(),
                changed: properties(vec![("Bonded", true.into()), ("Paired", true.into())]),
                invalidated: Vec::new(),
            }),
        )
        .await;

    assert!(state.borrow().pairing.is_some());
    assert!(
        matches!(
            still_open.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ),
        "dropping the sender would answer a question bluez is still waiting on"
    );
}

#[tokio::test]
async fn mirror_only_mode_never_enables_agent_registration() {
    let (mut service, ctx, _state, _health) = started(Dependencies { agent: false }).await;
    assert!(
        !service.agent_enabled,
        "starting must not attempt to register a pairing agent"
    );

    enumerated(&mut service, &ctx, session()).await;
    assert!(!service.agent_enabled);

    service
        .handle(
            &ctx,
            Input::Event(Event::NameOwner(Some(":1.7".to_owned()))),
        )
        .await;
    assert!(
        !service.agent_enabled,
        "coming back onto the bus must not attempt registration in mirror-only mode"
    );

    let (sent, outcome) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::Pair {
                id: DeviceId(HEADSET.to_owned()),
                reply: sent,
            }),
        )
        .await;
    assert!(matches!(
        outcome.await,
        Ok(Err(BluetoothError::Failed(Failure::NoAgent)))
    ));
}

#[tokio::test]
async fn the_panel_path_still_enables_agent_registration() {
    let (mut service, ctx, _state, _health) = started(Dependencies { agent: true }).await;
    assert!(service.agent_enabled);

    service
        .handle(
            &ctx,
            Input::Event(Event::NameOwner(Some(":1.7".to_owned()))),
        )
        .await;
    assert!(service.agent_enabled);
}

#[tokio::test]
async fn pairing_without_an_agent_is_refused_in_its_own_words() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    assert!(!service.agent, "there is no bus in tests to register on");

    let (sent, outcome) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::Pair {
                id: DeviceId(HEADSET.to_owned()),
                reply: sent,
            }),
        )
        .await;

    assert!(
        matches!(
            outcome.await,
            Ok(Err(BluetoothError::Failed(Failure::NoAgent)))
        ),
        "a pairing with no agent is refused as a typed failure the applet can word"
    );
    assert_eq!(
        state
            .borrow()
            .device(&DeviceId(HEADSET.to_owned()))
            .and_then(|device| device.failure),
        Some(Failure::NoAgent)
    );
}
