use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use relm4::Receiver;
use tokio::sync::oneshot;
use zbus::zvariant::{ObjectPath, OwnedValue, Str};
use zeroize::Zeroizing;

use super::*;
use crate::ask::{Ask, Input, Prompt, Reply, Source};
use crate::queue::Queue;

const USER_CANCELED: &str = "org.freedesktop.NetworkManager.SecretAgent.Error.UserCanceled";
const INTERNAL_ERROR: &str = "org.freedesktop.NetworkManager.SecretAgent.Error.InternalError";
const PROFILE_PATH: &str = "/org/freedesktop/NetworkManager/Settings/3";

#[derive(Default)]
struct Manager {
    registrations: Arc<Mutex<Vec<(String, u32)>>>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.AgentManager")]
impl Manager {
    async fn register_with_capabilities(&self, identifier: &str, capabilities: u32) {
        self.registrations
            .lock()
            .expect("registrations")
            .push((identifier.to_owned(), capabilities));
    }

    async fn unregister(&self) {}
}

#[derive(Clone, Default)]
struct Profile {
    settings: Arc<Mutex<Secrets>>,
    updates: Arc<Mutex<Vec<(Secrets, u32)>>>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    async fn get_settings(&self) -> Secrets {
        self.settings.lock().expect("settings").clone()
    }

    async fn update2(
        &self,
        settings: Secrets,
        flags: u32,
        _args: HashMap<String, OwnedValue>,
    ) -> HashMap<String, OwnedValue> {
        self.updates
            .lock()
            .expect("updates")
            .push((settings, flags));
        HashMap::new()
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.SecretAgent",
    default_service = "org.freedesktop.NetworkManager"
)]
trait SecretAgent {
    fn get_secrets(
        &self,
        connection: HashMap<String, HashMap<String, OwnedValue>>,
        path: &ObjectPath<'_>,
        setting: String,
        hints: Vec<String>,
        flags: u32,
    ) -> zbus::Result<HashMap<String, HashMap<String, OwnedValue>>>;

    fn cancel_get_secrets(&self, path: &ObjectPath<'_>, setting: String) -> zbus::Result<()>;

    fn save_secrets(
        &self,
        connection: HashMap<String, HashMap<String, OwnedValue>>,
        path: &ObjectPath<'_>,
    ) -> zbus::Result<()>;

    fn delete_secrets(
        &self,
        connection: HashMap<String, HashMap<String, OwnedValue>>,
        path: &ObjectPath<'_>,
    ) -> zbus::Result<()>;
}

struct Harness {
    _bus: glimpse_dbus::testing::PrivateBus,
    connection: zbus::Connection,
    manager: Manager,
    agent: SecretAgentProxy<'static>,
    receiver: Receiver<Input>,
    profile: Profile,
}

async fn harness() -> Harness {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let connection = bus.connection().await;
    let dbus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .expect("a bus proxy");
    dbus.request_name(
        nm::SERVICE.try_into().expect("a well-known name"),
        Default::default(),
    )
    .await
    .expect("the NetworkManager name");

    let manager = Manager::default();
    connection
        .object_server()
        .at(
            "/org/freedesktop/NetworkManager/AgentManager",
            Manager {
                registrations: manager.registrations.clone(),
            },
        )
        .await
        .expect("the manager mock");

    let profile = Profile::default();
    connection
        .object_server()
        .at(PROFILE_PATH, profile.clone())
        .await
        .expect("the profile mock");

    let (input, receiver) = relm4::channel::<Input>();
    tokio::spawn(start(connection.clone(), input));

    let unique = connection.unique_name().expect("a unique name").clone();
    let agent = SecretAgentProxy::builder(&connection)
        .destination(unique)
        .expect("a destination")
        .path(PATH)
        .expect("an agent path")
        .build()
        .await
        .expect("an agent proxy");

    Harness {
        _bus: bus,
        connection,
        manager,
        agent,
        receiver,
        profile,
    }
}

async fn wait_for(mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the condition within five seconds");
}

fn connection_named(id: &str) -> Secrets {
    HashMap::from([(
        "connection".to_owned(),
        HashMap::from([("id".to_owned(), OwnedValue::from(Str::from(id)))]),
    )])
}

fn autoconnected(id: &str, autoconnect: bool) -> Secrets {
    let mut connection = connection_named(id);
    connection
        .get_mut("connection")
        .expect("the connection group")
        .insert("autoconnect".to_owned(), OwnedValue::from(autoconnect));
    connection
}

fn path() -> ObjectPath<'static> {
    ObjectPath::try_from("/x").expect("a path")
}

fn profile_path() -> ObjectPath<'static> {
    ObjectPath::try_from(PROFILE_PATH).expect("a path")
}

fn secret(secret: &str, autoconnect: bool) -> Answer {
    Answer::Secret {
        secret: Zeroizing::new(secret.to_owned()),
        username: None,
        autoconnect,
    }
}

async fn recv_ask(receiver: &Receiver<Input>) -> Ask {
    match receiver.recv().await.expect("an ask") {
        Input::Ask(ask) => ask,
        other => panic!("expected a network ask, got {other:?}"),
    }
}

async fn next_ask(
    receiver: &Receiver<Input>,
) -> (crate::network::Request, oneshot::Sender<Answer>) {
    match receiver.recv().await.expect("an ask") {
        Input::Ask(Ask {
            source: Source::Network,
            key,
            prompt: Prompt::Secret(request),
            reply: Reply::Secret(reply),
        }) => {
            assert_eq!(key, format!("{}:{}", request.path, request.setting));
            (request, reply)
        }
        other => panic!("expected a network ask, got {other:?}"),
    }
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

#[test]
fn an_answer_is_shaped_by_the_setting_and_named_by_the_hint() {
    let wifi = answer_for(WIRELESS_SECURITY, &[], "x", None);
    assert!(
        wifi[WIRELESS_SECURITY].contains_key("psk"),
        "a wireless request with no hint is the ordinary pre-shared key"
    );

    let wep = answer_for(WIRELESS_SECURITY, &["wep-key0".to_owned()], "x", None);
    assert!(
        wep[WIRELESS_SECURITY].contains_key("wep-key0")
            && !wep[WIRELESS_SECURITY].contains_key("psk"),
        "a WEP key answered under psk is refused as an invalid property, which reads as a \
             wrong password and is not one"
    );

    let leap = answer_for(WIRELESS_SECURITY, &["leap-password".to_owned()], "x", None);
    assert!(
        leap[WIRELESS_SECURITY].contains_key("leap-password")
            && !leap[WIRELESS_SECURITY].contains_key("psk"),
        "a LEAP password is answered under its own key"
    );

    let ignored = answer_for(WIRELESS_SECURITY, &["something-else".to_owned()], "x", None);
    assert!(ignored[WIRELESS_SECURITY].contains_key("psk"));

    let vpn = answer_for(VPN, &["password".to_owned()], "x", None);
    let secrets = vpn[VPN].get("secrets").expect("the secrets member");
    let secrets = HashMap::<String, String>::try_from(secrets.try_clone().expect("a clone"))
        .expect("vpn.secrets is a{ss}, never a bare string");
    assert_eq!(secrets.get("password").map(String::as_str), Some("x"));

    let both = answer_for(
        VPN,
        &["username".to_owned(), "password".to_owned()],
        "x",
        Some("alice"),
    );
    let secrets = both[VPN].get("secrets").expect("the secrets member");
    let secrets = HashMap::<String, String>::try_from(secrets.try_clone().expect("a clone"))
        .expect("vpn.secrets is a{ss}");
    assert_eq!(secrets.get("username").map(String::as_str), Some("alice"));
    assert_eq!(secrets.get("password").map(String::as_str), Some("x"));

    let unnamed = answer_for(VPN, &["username".to_owned()], "x", None);
    let secrets = unnamed[VPN].get("secrets").expect("the secrets member");
    let secrets = HashMap::<String, String>::try_from(secrets.try_clone().expect("a clone"))
        .expect("vpn.secrets is a{ss}");
    assert!(
        !secrets.contains_key("username"),
        "no username was offered, so none is answered"
    );
    assert_eq!(secrets.get("password").map(String::as_str), Some("x"));
}

#[test]
fn an_answer_never_prints_the_secret() {
    let answer = Answer::Secret {
        secret: Zeroizing::new("hunter2hunter2".to_owned()),
        username: None,
        autoconnect: true,
    };
    let printed = format!("{answer:?}");

    assert!(!printed.contains("hunter2hunter2"), "printed: {printed}");
    assert!(printed.contains("<redacted>"));
}

#[tokio::test]
async fn a_wireless_secret_is_answered_under_its_key_management() {
    let harness = harness().await;
    wait_for(|| {
        !harness
            .manager
            .registrations
            .lock()
            .expect("registrations")
            .is_empty()
    })
    .await;
    assert_eq!(
        harness.manager.registrations.lock().expect("registrations")[0],
        (IDENTIFIER.to_owned(), nm::AGENT_CAPABILITY_VPN_HINTS),
        "the agent registers with the VPN-hints capability so a plugin's hints arrive"
    );

    let agent = harness.agent.clone();
    let path = profile_path();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                autoconnected("Skylink", true),
                &path,
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION | nm::SECRET_FLAG_USER_REQUESTED,
            )
            .await
    });

    let (request, reply) = next_ask(&harness.receiver).await;
    assert_eq!(request.name, "Skylink");
    assert_eq!(request.path, PROFILE_PATH);
    assert_eq!(request.setting, WIRELESS_SECURITY);
    assert!(!request.retry);
    assert!(request.autoconnect, "absent means true in a fresh profile");
    assert!(request.hints.is_empty());

    let _ = reply.send(secret("hunter2hunter2", true));
    let secrets = asked.await.expect("the task").expect("secrets");
    let psk = secrets
        .get(WIRELESS_SECURITY)
        .and_then(|setting| setting.get("psk"))
        .and_then(|value| <&str>::try_from(value).ok());
    assert_eq!(psk, Some("hunter2hunter2"));
}

#[tokio::test]
async fn the_switch_becomes_the_profile_autoconnect() {
    let harness = harness().await;
    harness
        .profile
        .settings
        .lock()
        .expect("settings")
        .clone_from(&autoconnected("Skylink", true));

    let agent = harness.agent.clone();
    let path = profile_path();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                autoconnected("Skylink", true),
                &path,
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });
    let (_, reply) = next_ask(&harness.receiver).await;
    let _ = reply.send(secret("hunter2hunter2", false));

    asked.await.expect("the task").expect("secrets");
    assert!(
        !harness.profile.updates.lock().expect("updates").is_empty(),
        "autoconnect is written before GetSecrets returns"
    );
    let (settings, flags) = harness.profile.updates.lock().expect("updates")[0].clone();
    assert_eq!(
        flags, UPDATE2_TO_DISK_NO_REAPPLY,
        "the profile is written to disk without a reapply"
    );
    assert_eq!(
        settings
            .get("connection")
            .and_then(|group| group.get("autoconnect"))
            .and_then(|value| bool::try_from(value).ok()),
        Some(false)
    );
    assert_eq!(
        settings
            .get("connection")
            .and_then(|group| group.get("id"))
            .and_then(|value| <&str>::try_from(value).ok()),
        Some("Skylink"),
        "the untouched members of the profile survive the write"
    );

    let agent = harness.agent.clone();
    let path = profile_path();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                autoconnected("Skylink", true),
                &path,
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });
    let (_, reply) = next_ask(&harness.receiver).await;
    let _ = reply.send(secret("hunter2hunter2", true));
    asked.await.expect("the task").expect("secrets");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        harness.profile.updates.lock().expect("updates").len(),
        1,
        "an unchanged switch writes nothing"
    );
}

#[tokio::test]
async fn a_request_new_round_keeps_the_dialog_and_hints_at_the_error() {
    let harness = harness().await;

    let agent = harness.agent.clone();
    let path = profile_path();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                &path,
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_REQUEST_NEW,
            )
            .await
    });

    let (request, reply) = next_ask(&harness.receiver).await;
    assert!(
        request.retry,
        "REQUEST_NEW means the stored secret was wrong; the prompt has to say so"
    );
    let _ = reply.send(secret("hunter2hunter2", true));
    asked.await.expect("the task").expect("secrets");

    let agent = harness.agent.clone();
    let path = profile_path();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                &path,
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION | nm::SECRET_FLAG_REQUEST_NEW,
            )
            .await
    });
    let (request, reply) = next_ask(&harness.receiver).await;
    assert!(request.retry);
    let _ = reply.send(secret("hunter2hunter2", true));
    asked.await.expect("the task").expect("secrets");
}

#[tokio::test]
async fn a_cancelled_request_answers_user_canceled() {
    let harness = harness().await;

    let agent = harness.agent.clone();
    let path = profile_path();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                &path,
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });
    let (_, reply) = next_ask(&harness.receiver).await;
    let _ = reply.send(Answer::Refused);

    assert!(
        matches!(
            asked.await.expect("the task"),
            Err(zbus::Error::MethodError(name, ..)) if name.as_str() == USER_CANCELED
        ),
        "NetworkManager must be told the user said no, not handed an empty map"
    );
}

#[tokio::test]
async fn a_withdrawn_request_closes_its_dialog() {
    let harness = harness().await;

    let agent = harness.agent.clone();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                &profile_path(),
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });
    let mut queue = Queue::default();
    queue.push(recv_ask(&harness.receiver).await);
    queue.withdraw(
        Source::Network,
        Some(&format!("{PROFILE_PATH}:{WIRELESS_SECURITY}")),
    );

    assert!(
        matches!(
            asked.await.expect("the task"),
            Err(zbus::Error::MethodError(name, ..)) if name.as_str() == USER_CANCELED
        ),
        "a dropped ask is NetworkManager's own cancel, as its 120 s timeout produces"
    );

    let agent = harness.agent.clone();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                &profile_path(),
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });
    let mut queue = Queue::default();
    queue.push(recv_ask(&harness.receiver).await);
    harness
        .agent
        .cancel_get_secrets(&profile_path(), WIRELESS_SECURITY.to_owned())
        .await
        .expect("cancel acknowledged");
    match harness.receiver.recv().await.expect("a withdraw") {
        Input::Withdraw {
            source: Source::Network,
            key: Some(key),
        } => {
            assert_eq!(key, format!("{PROFILE_PATH}:{WIRELESS_SECURITY}"));
            queue.withdraw(Source::Network, Some(&key));
        }
        other => panic!("expected a network withdraw, got {other:?}"),
    }

    assert!(
        matches!(
            asked.await.expect("the task"),
            Err(zbus::Error::MethodError(name, ..)) if name.as_str() == USER_CANCELED
        ),
        "a withdraw drops the ask, which is NetworkManager's own cancel answer"
    );
}

#[tokio::test]
async fn a_vpn_request_offers_a_user_name_and_answers_under_the_hint() {
    let harness = harness().await;

    let agent = harness.agent.clone();
    let path = profile_path();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Work VPN"),
                &path,
                VPN.to_owned(),
                vec!["username".to_owned(), "password".to_owned()],
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });

    let (request, reply) = next_ask(&harness.receiver).await;
    assert_eq!(request.name, "Work VPN");
    assert_eq!(request.setting, VPN);
    assert_eq!(
        request.hints,
        vec!["username".to_owned(), "password".to_owned()]
    );

    let _ = reply.send(Answer::Secret {
        secret: Zeroizing::new("hunter2hunter2".to_owned()),
        username: Some("alice".to_owned()),
        autoconnect: true,
    });
    let secrets = asked.await.expect("the task").expect("secrets");
    let vpn = secrets.get(VPN).expect("the vpn setting");
    let secrets = vpn.get("secrets").expect("the secrets member");
    let secrets = HashMap::<String, String>::try_from(secrets.try_clone().expect("a clone"))
        .expect("vpn.secrets is a{ss}");
    assert_eq!(secrets.get("username").map(String::as_str), Some("alice"));
    assert_eq!(
        secrets.get("password").map(String::as_str),
        Some("hunter2hunter2")
    );
}

#[tokio::test]
async fn a_request_nobody_can_answer_is_refused_without_a_dialog() {
    let harness = harness().await;

    let agent = harness.agent.clone();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("Skylink"),
                &path(),
                WIRELESS_SECURITY.to_owned(),
                Vec::new(),
                0,
            )
            .await
    });
    let secrets = tokio::time::timeout(Duration::from_secs(2), asked)
        .await
        .expect("it must answer rather than wait for a prompt")
        .expect("the task")
        .expect("an empty map rather than an error");
    assert!(secrets.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(200), harness.receiver.recv())
            .await
            .is_err(),
        "nothing may reach the app when interaction is not allowed"
    );

    let agent = harness.agent.clone();
    let asked = tokio::spawn(async move {
        agent
            .get_secrets(
                connection_named("eduroam"),
                &path(),
                "802-1x".to_owned(),
                Vec::new(),
                nm::SECRET_FLAG_ALLOW_INTERACTION,
            )
            .await
    });
    let secrets = tokio::time::timeout(Duration::from_secs(2), asked)
        .await
        .expect("it must answer rather than open a prompt it cannot fulfil")
        .expect("the task")
        .expect("an empty map");
    assert!(secrets.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(200), harness.receiver.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn save_and_delete_secrets_store_nothing() {
    let harness = harness().await;

    for call in [
        harness
            .agent
            .save_secrets(connection_named("Skylink"), &path())
            .await,
        harness
            .agent
            .delete_secrets(connection_named("Skylink"), &path())
            .await,
    ] {
        assert!(
            matches!(
                call,
                Err(zbus::Error::MethodError(name, ..)) if name.as_str() == INTERNAL_ERROR
            ),
            "the agent holds no store; NetworkManager keeps the profile's secrets itself"
        );
    }
}

#[tokio::test]
async fn the_agent_registers_again_when_networkmanager_returns() {
    let harness = harness().await;
    wait_for(|| {
        !harness
            .manager
            .registrations
            .lock()
            .expect("registrations")
            .is_empty()
    })
    .await;
    let dbus = zbus::fdo::DBusProxy::new(&harness.connection)
        .await
        .expect("a bus proxy");

    dbus.release_name(nm::SERVICE.try_into().expect("a well-known name"))
        .await
        .expect("the name released");
    dbus.request_name(
        nm::SERVICE.try_into().expect("a well-known name"),
        Default::default(),
    )
    .await
    .expect("the name back");

    wait_for(|| {
        harness
            .manager
            .registrations
            .lock()
            .expect("registrations")
            .len()
            >= 2
    })
    .await;
    assert_eq!(
        harness.manager.registrations.lock().expect("registrations")[1],
        (IDENTIFIER.to_owned(), nm::AGENT_CAPABILITY_VPN_HINTS)
    );
}
