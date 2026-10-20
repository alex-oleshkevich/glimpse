use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};
use zbus::zvariant::{ObjectPath, OwnedValue, Str};
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

/// The answer NetworkManager's schema expects, which is not one shape. `vpn.secrets` is `a{ss}`
/// keyed by the hint the plugin asked under, and a wireless key is named by its own key management —
/// a WEP network answered under `psk` is refused as an invalid property, not as a wrong password.
fn answer_for(setting: &str, hints: &[String], secret: &str) -> Secrets {
    let mut inner: HashMap<String, OwnedValue> = HashMap::new();
    match setting {
        VPN => {
            let key = hints.first().map(String::as_str).unwrap_or("password");
            let secrets = HashMap::from([(key.to_owned(), secret.to_owned())]);
            inner.insert("secrets".to_owned(), OwnedValue::from(secrets));
        }
        _ => {
            let key = hints
                .iter()
                .map(String::as_str)
                .find(|hint| WIRELESS_KEYS.contains(hint))
                .unwrap_or("psk");
            inner.insert(key.to_owned(), OwnedValue::from(Str::from(secret)));
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
mod tests;
