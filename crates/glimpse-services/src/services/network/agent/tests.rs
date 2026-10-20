use super::*;

fn agent() -> (Agent, mpsc::Receiver<Input<Network>>) {
    let (events, inbox) = mpsc::channel(8);
    (Agent::new(events), inbox)
}

fn connection_named(id: &str) -> Secrets {
    HashMap::from([(
        "connection".to_owned(),
        HashMap::from([(
            "id".to_owned(),
            OwnedValue::try_from(Value::from(id)).expect("a string"),
        )]),
    )])
}

#[test]
fn request_new_implies_interaction_even_without_allow_interaction() {
    assert!(
        may_prompt(nm::SECRET_FLAG_REQUEST_NEW),
        "libnm: this flag implies that interaction is allowed"
    );
    assert!(may_prompt(nm::SECRET_FLAG_ALLOW_INTERACTION));
    assert!(may_prompt(
        nm::SECRET_FLAG_ALLOW_INTERACTION | nm::SECRET_FLAG_REQUEST_NEW
    ));
    assert!(!may_prompt(0));
    assert!(
        !may_prompt(nm::SECRET_FLAG_USER_REQUESTED),
        "USER_REQUESTED alone permits nothing"
    );
}

#[test]
fn only_a_request_new_is_a_retry() {
    assert!(is_retry(nm::SECRET_FLAG_REQUEST_NEW));
    assert!(is_retry(
        nm::SECRET_FLAG_ALLOW_INTERACTION | nm::SECRET_FLAG_REQUEST_NEW
    ));
    assert!(!is_retry(nm::SECRET_FLAG_ALLOW_INTERACTION));
}

#[test]
fn glimpse_services_a_psk_and_a_vpn_and_refuses_enterprise() {
    assert!(serviceable(WIRELESS_SECURITY));
    assert!(serviceable(VPN));
    assert!(
        !serviceable("802-1x"),
        "an enterprise profile cannot be created here, so a prompt would be a lie"
    );
    assert!(!serviceable("ipv4"));
}

#[tokio::test]
async fn a_request_with_no_interaction_bit_answers_empty_without_asking_anyone() {
    let (agent, mut inbox) = agent();

    let secrets = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        agent.get_secrets(
            connection_named("Skylink"),
            ObjectPath::try_from("/x").expect("a path"),
            WIRELESS_SECURITY.to_owned(),
            Vec::new(),
            0,
        ),
    )
    .await
    .expect("it must answer rather than wait for a prompt")
    .expect("an empty map rather than an error");

    assert!(secrets.is_empty());
    assert!(
        inbox.try_recv().is_err(),
        "nothing may reach the panel when interaction is not allowed"
    );
}

#[tokio::test]
async fn an_enterprise_request_is_answered_empty_and_never_prompts() {
    let (agent, mut inbox) = agent();

    let secrets = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        agent.get_secrets(
            connection_named("eduroam"),
            ObjectPath::try_from("/x").expect("a path"),
            "802-1x".to_owned(),
            Vec::new(),
            nm::SECRET_FLAG_ALLOW_INTERACTION,
        ),
    )
    .await
    .expect("it must answer rather than open a prompt it cannot fulfil")
    .expect("an empty map");

    assert!(secrets.is_empty());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn a_retry_reaches_the_panel_marked_as_one() {
    let (agent, mut inbox) = agent();

    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                ObjectPath::try_from("/x").expect("a path"),
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_REQUEST_NEW,
            )
            .await
    });

    let Some(Input::Event(Event::Secrets(request, reply))) = inbox.recv().await else {
        panic!("the agent must ask");
    };
    assert_eq!(request.name, "Skylink");
    assert!(
        request.retry,
        "REQUEST_NEW means the stored secret was wrong; the prompt has to say so"
    );

    let _ = reply.send(Answer::Secret(Zeroizing::new("hunter2hunter2".to_owned())));
    let secrets = asked.await.expect("the task").expect("secrets");
    assert!(secrets.contains_key(WIRELESS_SECURITY));
}

#[tokio::test]
async fn a_refused_prompt_is_a_user_cancellation_and_not_an_empty_answer() {
    let (agent, mut inbox) = agent();

    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                ObjectPath::try_from("/x").expect("a path"),
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });

    let Some(Input::Event(Event::Secrets(_, reply))) = inbox.recv().await else {
        panic!("the agent must ask");
    };
    let _ = reply.send(Answer::Refused);

    assert!(
        matches!(
            asked.await.expect("the task"),
            Err(AgentError::UserCanceled(_))
        ),
        "NetworkManager must be told the user said no, not handed an empty map"
    );
}

#[tokio::test]
async fn a_prompt_that_is_dropped_cancels_rather_than_hanging() {
    let (agent, mut inbox) = agent();

    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                ObjectPath::try_from("/x").expect("a path"),
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });

    let Some(Input::Event(Event::Secrets(_, reply))) = inbox.recv().await else {
        panic!("the agent must ask");
    };
    drop(reply);

    assert!(matches!(
        asked.await.expect("the task"),
        Err(AgentError::UserCanceled(_))
    ));
}

#[test]
fn an_answer_is_shaped_by_the_setting_and_named_by_the_hint() {
    let wifi = answer_for(WIRELESS_SECURITY, &[], "x").expect("an answer");
    assert!(
        wifi[WIRELESS_SECURITY].contains_key("psk"),
        "a wireless request with no hint is the ordinary pre-shared key"
    );

    let wep = answer_for(WIRELESS_SECURITY, &["wep-key0".to_owned()], "x").expect("an answer");
    assert!(
        wep[WIRELESS_SECURITY].contains_key("wep-key0")
            && !wep[WIRELESS_SECURITY].contains_key("psk"),
        "a WEP key answered under psk is refused as an invalid property, which reads as a \
             wrong password and is not one"
    );

    let ignored =
        answer_for(WIRELESS_SECURITY, &["something-else".to_owned()], "x").expect("an answer");
    assert!(ignored[WIRELESS_SECURITY].contains_key("psk"));

    let vpn = answer_for(VPN, &["password".to_owned()], "x").expect("an answer");
    let secrets = vpn[VPN].get("secrets").expect("the secrets member");
    let secrets = HashMap::<String, String>::try_from(secrets.try_clone().expect("a clone"))
        .expect("vpn.secrets is a{ss}, never a bare string");
    assert_eq!(secrets.get("password").map(String::as_str), Some("x"));
}

#[test]
fn an_answer_never_prints_the_secret() {
    let answer = Answer::Secret(Zeroizing::new("hunter2hunter2".to_owned()));
    let printed = format!("{answer:?}");

    assert!(!printed.contains("hunter2hunter2"), "printed: {printed}");
    assert_eq!(printed, "Secret(redacted)");
}

#[test]
fn the_agent_identifier_is_per_process() {
    assert!(identifier().ends_with(&std::process::id().to_string()));
}
