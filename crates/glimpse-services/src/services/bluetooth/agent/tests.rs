use super::*;

const DEVICE: &str = "/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5";

fn agent() -> (Agent, mpsc::Receiver<Input<Bluetooth>>) {
    let (events, inbox) = mpsc::channel(8);
    (Agent::new(events), inbox)
}

fn parked(inbox: &mut mpsc::Receiver<Input<Bluetooth>>) -> oneshot::Sender<Answer> {
    match inbox.try_recv() {
        Ok(Input::Event(Event::Prompt(_, Some(answer)))) => answer,
        _ => panic!("the agent did not park a prompt"),
    }
}

#[tokio::test]
async fn a_prompt_whose_answer_is_dropped_is_canceled_rather_than_panicking() {
    let (agent, mut inbox) = agent();
    let device = ObjectPath::try_from(DEVICE).expect("a path");

    let asking = tokio::spawn(async move {
        let device = ObjectPath::try_from(DEVICE).expect("a path");
        agent.request_confirmation(device, 123_456).await
    });
    let _ = device;
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    drop(parked(&mut inbox));

    assert!(matches!(
        asking.await.expect("the task ran"),
        Err(AgentError::Canceled(_))
    ));
}

#[tokio::test]
async fn a_denied_confirmation_is_rejected_rather_than_canceled() {
    let (agent, mut inbox) = agent();

    let asking = tokio::spawn(async move {
        let device = ObjectPath::try_from(DEVICE).expect("a path");
        agent.request_confirmation(device, 123_456).await
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    let _ = parked(&mut inbox).send(Answer::Deny);

    assert!(matches!(
        asking.await.expect("the task ran"),
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test]
async fn a_pin_that_comes_back_as_a_passkey_is_refused() {
    let (agent, mut inbox) = agent();

    let asking = tokio::spawn(async move {
        let device = ObjectPath::try_from(DEVICE).expect("a path");
        agent.request_pin_code(device).await
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    let _ = parked(&mut inbox).send(Answer::Passkey(1));

    assert!(matches!(
        asking.await.expect("the task ran"),
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test]
async fn a_display_only_prompt_answers_at_once_and_parks_nothing() {
    let (agent, mut inbox) = agent();
    let device = ObjectPath::try_from(DEVICE).expect("a path");

    agent.display_passkey(device, 123_456, 3).await;

    match inbox.try_recv() {
        Ok(Input::Event(Event::Prompt(
            Prompt::DisplayPasskey {
                passkey, entered, ..
            },
            None,
        ))) => {
            assert_eq!((passkey, entered), (123_456, 3));
        }
        _ => panic!("a display prompt must not park"),
    }
}

fn asked(inbox: &mut mpsc::Receiver<Input<Bluetooth>>) -> oneshot::Sender<bool> {
    match inbox.try_recv() {
        Ok(Input::Event(Event::AuthorizeService { reply, .. })) => reply,
        _ => panic!("the agent did not ask about the service"),
    }
}

async fn authorizing(answer: Option<bool>) -> Result<(), AgentError> {
    let (agent, mut inbox) = agent();
    let asking = tokio::spawn(async move {
        let device = ObjectPath::try_from(DEVICE).expect("a path");
        agent
            .authorize_service(device, "0000110b-0000-1000-8000-00805f9b34fb".to_owned())
            .await
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    let reply = asked(&mut inbox);
    match answer {
        Some(answer) => {
            let _ = reply.send(answer);
        }
        None => drop(reply),
    }
    asking.await.expect("the task ran")
}

#[tokio::test]
async fn a_service_of_a_bonded_device_is_authorized() {
    assert!(authorizing(Some(true)).await.is_ok());
}

#[tokio::test]
async fn a_service_of_an_unbonded_device_is_refused() {
    assert!(matches!(
        authorizing(Some(false)).await,
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test]
async fn a_service_nobody_answers_is_refused_rather_than_hanging() {
    assert!(matches!(
        authorizing(None).await,
        Err(AgentError::Rejected(_))
    ));
}

#[tokio::test]
async fn release_tells_the_service_rather_than_unregistering() {
    let (agent, mut inbox) = agent();

    agent.release().await;

    assert!(matches!(
        inbox.try_recv(),
        Ok(Input::Event(Event::AgentReleased))
    ));
}

#[tokio::test]
async fn cancel_withdraws_the_prompt() {
    let (agent, mut inbox) = agent();

    agent.cancel().await;

    assert!(matches!(
        inbox.try_recv(),
        Ok(Input::Event(Event::PromptGone))
    ));
}
