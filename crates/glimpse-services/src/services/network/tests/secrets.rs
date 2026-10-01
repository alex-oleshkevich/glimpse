use super::service::*;
use super::*;
use zeroize::Zeroizing;

#[tokio::test]
async fn a_second_secret_request_is_refused_rather_than_queued() {
    let mut harness = harness().await;
    let (first, _first_reply) = tokio::sync::oneshot::channel();
    let (second, second_reply) = tokio::sync::oneshot::channel();

    let ask = |name: &str| Request {
        name: name.to_owned(),
        path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
        setting: "802-11-wireless-security".to_owned(),
        retry: false,
    };

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Secrets(ask("Skylink"), first)),
        )
        .await;
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Secrets(ask("Skylink 2G"), second)),
        )
        .await;

    let refused = tokio::time::timeout(std::time::Duration::from_secs(2), second_reply)
        .await
        .expect("the second request must be answered, not held open");
    assert!(
        matches!(refused, Ok(Answer::Refused)),
        "two password dialogs at once cannot be told apart by whoever answers them"
    );
    assert_eq!(
        harness
            .service
            .secret
            .as_ref()
            .map(|(request, _)| request.name.clone()),
        Some("Skylink".to_owned()),
        "the first prompt is the one that stays open"
    );
}

#[tokio::test]
async fn answering_a_prompt_resolves_the_agent_and_clears_the_state() {
    let mut harness = harness().await;
    let (sender, reply) = tokio::sync::oneshot::channel();

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Secrets(
                Request {
                    name: "Skylink".to_owned(),
                    path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                    setting: "802-11-wireless-security".to_owned(),
                    retry: true,
                },
                sender,
            )),
        )
        .await;
    assert!(harness.service.secret.is_some());

    let (ack, _acked) = tokio::sync::oneshot::channel();
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Command(Command::AnswerSecret {
                answer: Answer::Secret(Zeroizing::new("hunter2hunter2".to_owned())),
                reply: ack,
            }),
        )
        .await;

    let answered = tokio::time::timeout(std::time::Duration::from_secs(2), reply)
        .await
        .expect("the agent must be resolved, not left waiting");
    assert!(matches!(answered, Ok(Answer::Secret(_))));
    assert!(harness.service.secret.is_none());
}

#[tokio::test]
async fn a_cancellation_naming_another_request_leaves_the_open_prompt_alone() {
    let mut harness = harness().await;
    let (sender, reply) = tokio::sync::oneshot::channel();

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Secrets(
                Request {
                    name: "Skylink".to_owned(),
                    path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                    setting: "802-11-wireless-security".to_owned(),
                    retry: false,
                },
                sender,
            )),
        )
        .await;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::SecretsCancelled {
                path: "/org/freedesktop/NetworkManager/Settings/9".to_owned(),
                setting: "802-11-wireless-security".to_owned(),
            }),
        )
        .await;
    assert!(
        harness.service.secret.is_some(),
        "a late cancellation for a request already answered would otherwise refuse the one \
         the user is looking at"
    );

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::SecretsCancelled {
                path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                setting: "802-11-wireless-security".to_owned(),
            }),
        )
        .await;
    assert!(harness.service.secret.is_none());
    assert!(matches!(reply.await, Ok(Answer::Refused)));
}

#[tokio::test]
async fn networkmanager_leaving_the_bus_refuses_an_open_prompt() {
    let mut harness = harness().await;
    let (sender, reply) = tokio::sync::oneshot::channel();

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Secrets(
                Request {
                    name: "Skylink".to_owned(),
                    path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                    setting: "802-11-wireless-security".to_owned(),
                    retry: false,
                },
                sender,
            )),
        )
        .await;
    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::NameOwner(None)))
        .await;

    let refused = tokio::time::timeout(std::time::Duration::from_secs(2), reply)
        .await
        .expect("a prompt left open against a dead NetworkManager would hang forever");
    assert!(matches!(refused, Ok(Answer::Refused)));
}
