use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zeroize::Zeroizing;

use glimpse_dbus::network_manager as nm;

use crate::service::Input;

use super::{Event, Network};

pub const PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";

const WIRELESS_SECURITY: &str = "802-11-wireless-security";
const NO_STORE: &str =
    "glimpse holds no secret store; a secret it collects is written into the profile";
const VPN: &str = "vpn";

type Secrets = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub name: String,
    pub path: String,
    pub setting: String,
    pub retry: bool,
}

#[derive(Clone)]
pub enum Answer {
    Secret(Zeroizing<String>),
    Refused,
}

impl std::fmt::Debug for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Secret(_) => f.write_str("Secret(redacted)"),
            Self::Refused => f.write_str("Refused"),
        }
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.NetworkManager.SecretAgent.Error")]
pub enum AgentError {
    UserCanceled(String),
    InternalError(String),
}

pub struct Agent {
    events: mpsc::Sender<Input<Network>>,
}

impl Agent {
    pub fn new(events: mpsc::Sender<Input<Network>>) -> Self {
        Self { events }
    }

    async fn ask(&self, request: Request) -> Result<Answer, AgentError> {
        let (answer, reply) = oneshot::channel();
        self.events
            .send(Input::Event(Event::Secrets(request, answer)))
            .await
            .map_err(|_| {
                AgentError::InternalError("the panel is not accepting prompts".to_owned())
            })?;

        match reply.await {
            Ok(answer) => Ok(answer),
            Err(_) => Err(AgentError::UserCanceled("the prompt went away".to_owned())),
        }
    }
}

pub fn may_prompt(flags: u32) -> bool {
    flags & (nm::SECRET_FLAG_ALLOW_INTERACTION | nm::SECRET_FLAG_REQUEST_NEW) != 0
}

pub fn is_retry(flags: u32) -> bool {
    flags & nm::SECRET_FLAG_REQUEST_NEW != 0
}

pub fn serviceable(setting: &str) -> bool {
    setting == WIRELESS_SECURITY || setting == VPN
}

fn named(settings: &Secrets) -> Option<String> {
    let connection = settings.get("connection")?;
    <&str>::try_from(connection.get("id")?)
        .ok()
        .map(str::to_owned)
}

const WIRELESS_KEYS: &[&str] = &["psk", "wep-key0", "leap-password"];

fn owned(value: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(value).unwrap_or_else(|_| {
        OwnedValue::try_from(Value::from("")).expect("an empty string is representable")
    })
}

/// The answer NetworkManager's schema expects, which is not one shape. `vpn.secrets` is `a{ss}`
/// keyed by the hint the plugin asked under, and a wireless key is named by its own key management —
/// a WEP network answered under `psk` is refused as an invalid property, not as a wrong password.
fn answer_for(setting: &str, hints: &[String], secret: &str) -> Secrets {
    let mut inner: HashMap<String, OwnedValue> = HashMap::new();
    match setting {
        VPN => {
            let key = hints.first().map(String::as_str).unwrap_or("password");
            let secrets = HashMap::from([(key.to_owned(), secret.to_owned())]);
            inner.insert("secrets".to_owned(), owned(Value::from(secrets)));
        }
        _ => {
            let key = hints
                .iter()
                .map(String::as_str)
                .find(|hint| WIRELESS_KEYS.contains(hint))
                .unwrap_or("psk");
            inner.insert(key.to_owned(), owned(Value::from(secret)));
        }
    }
    HashMap::from([(setting.to_owned(), inner)])
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl Agent {
    async fn get_secrets(
        &self,
        connection: Secrets,
        path: ObjectPath<'_>,
        setting_name: String,
        hints: Vec<String>,
        flags: u32,
    ) -> Result<Secrets, AgentError> {
        if !serviceable(&setting_name) {
            tracing::debug!(setting = %setting_name, "not a setting glimpse can service");
            return Ok(Secrets::new());
        }
        if !may_prompt(flags) {
            return Ok(Secrets::new());
        }

        let request = Request {
            name: named(&connection).unwrap_or_default(),
            path: path.to_string(),
            setting: setting_name.clone(),
            retry: is_retry(flags),
        };
        match self.ask(request).await? {
            Answer::Secret(secret) => Ok(answer_for(&setting_name, &hints, &secret)),
            Answer::Refused => Err(AgentError::UserCanceled("refused".to_owned())),
        }
    }

    async fn cancel_get_secrets(
        &self,
        path: ObjectPath<'_>,
        setting_name: String,
    ) -> Result<(), AgentError> {
        let _ = self
            .events
            .send(Input::Event(Event::SecretsCancelled {
                path: path.to_string(),
                setting: setting_name,
            }))
            .await;
        Ok(())
    }

    async fn save_secrets(
        &self,
        _connection: Secrets,
        _path: ObjectPath<'_>,
    ) -> Result<(), AgentError> {
        Err(AgentError::InternalError(NO_STORE.to_owned()))
    }

    async fn delete_secrets(
        &self,
        _connection: Secrets,
        _path: ObjectPath<'_>,
    ) -> Result<(), AgentError> {
        Err(AgentError::InternalError(NO_STORE.to_owned()))
    }
}

fn identifier() -> String {
    format!("me.aresa.glimpse.{}", std::process::id())
}

pub async fn register(
    connection: &zbus::Connection,
    events: mpsc::Sender<Input<Network>>,
) -> zbus::Result<()> {
    let exported = connection
        .object_server()
        .at(PATH, Agent::new(events))
        .await?;

    let manager = nm::AgentManagerProxy::new(connection).await?;
    let _ = manager.unregister().await;

    match manager
        .register_with_capabilities(&identifier(), nm::AGENT_CAPABILITY_VPN_HINTS)
        .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            if exported {
                let _ = connection.object_server().remove::<Agent, _>(PATH).await;
            }
            Err(error)
        }
    }
}

pub async fn unregister(connection: &zbus::Connection) {
    if let Ok(manager) = nm::AgentManagerProxy::new(connection).await {
        let _ = manager.unregister().await;
    }
    let _ = connection.object_server().remove::<Agent, _>(PATH).await;
}

#[cfg(test)]
mod tests {
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
        let wifi = answer_for(WIRELESS_SECURITY, &[], "x");
        assert!(
            wifi[WIRELESS_SECURITY].contains_key("psk"),
            "a wireless request with no hint is the ordinary pre-shared key"
        );

        let wep = answer_for(WIRELESS_SECURITY, &["wep-key0".to_owned()], "x");
        assert!(
            wep[WIRELESS_SECURITY].contains_key("wep-key0")
                && !wep[WIRELESS_SECURITY].contains_key("psk"),
            "a WEP key answered under psk is refused as an invalid property, which reads as a \
             wrong password and is not one"
        );

        let ignored = answer_for(WIRELESS_SECURITY, &["something-else".to_owned()], "x");
        assert!(ignored[WIRELESS_SECURITY].contains_key("psk"));

        let vpn = answer_for(VPN, &["password".to_owned()], "x");
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
}
