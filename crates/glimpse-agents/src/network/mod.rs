use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use relm4::Sender;
use tokio::sync::oneshot;
use zbus::zvariant::{ObjectPath, OwnedValue, Str, Value};

use glimpse_dbus::network_manager as nm;

use crate::ask::{Ask, Input, Prompt, Reply, Source};

pub const IDENTIFIER: &str = "me.aresa.glimpse.agents";
pub const PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";

const WIRELESS_SECURITY: &str = "802-11-wireless-security";
const VPN: &str = "vpn";
const USERNAME: &str = "username";
const NO_STORE: &str =
    "glimpse holds no secret store; a secret it collects is written into the profile";
const UPDATE2_TO_DISK_NO_REAPPLY: u32 = 0x41;

const WIRELESS_KEYS: &[&str] = &["psk", "wep-key0", "leap-password"];

pub type Secrets = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub name: String,
    pub path: String,
    pub setting: String,
    pub retry: bool,
    pub hints: Vec<String>,
    pub autoconnect: bool,
}

#[derive(Clone)]
pub enum Answer {
    Secret {
        secret: crate::ask::Secret,
        username: Option<String>,
        autoconnect: bool,
    },
    Refused,
}

impl fmt::Debug for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secret {
                username,
                autoconnect,
                ..
            } => f
                .debug_struct("Secret")
                .field("secret", &"<redacted>")
                .field("username", username)
                .field("autoconnect", autoconnect)
                .finish(),
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
    input: Sender<Input>,
    system: zbus::Connection,
}

impl Agent {
    fn new(input: Sender<Input>, system: zbus::Connection) -> Self {
        Self { input, system }
    }

    async fn ask(&self, request: &Request) -> Result<Answer, AgentError> {
        let (reply, receiver) = oneshot::channel();
        let ask = Ask {
            source: Source::Network,
            key: format!("{}:{}", request.path, request.setting),
            prompt: Prompt::Secret(request.clone()),
            reply: Reply::Secret(reply),
        };
        self.input.send(Input::Ask(ask)).map_err(|_| {
            AgentError::InternalError("the app is not accepting prompts".to_owned())
        })?;
        receiver
            .await
            .map_err(|_| AgentError::UserCanceled("the prompt went away".to_owned()))
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

fn autoconnect(settings: &Secrets) -> bool {
    settings
        .get("connection")
        .and_then(|connection| connection.get("autoconnect"))
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(true)
}

pub fn answer_for(
    setting: &str,
    hints: &[String],
    secret: &str,
    username: Option<&str>,
) -> Secrets {
    let mut inner: HashMap<String, OwnedValue> = HashMap::new();
    match setting {
        VPN => {
            let mut secrets: HashMap<String, String> = HashMap::new();
            if let Some(username) = username {
                secrets.insert(USERNAME.to_owned(), username.to_owned());
            }
            let key = hints
                .iter()
                .map(String::as_str)
                .find(|hint| *hint != USERNAME)
                .unwrap_or("password");
            secrets.insert(key.to_owned(), secret.to_owned());
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

pub async fn write_autoconnect(
    system: &zbus::Connection,
    path: &str,
    autoconnect: bool,
) -> zbus::Result<()> {
    let connection = nm::SettingsConnectionProxy::builder(system)
        .path(path)?
        .build()
        .await?;
    let mut settings = connection.get_settings().await?;
    settings
        .entry("connection".to_owned())
        .or_default()
        .insert("autoconnect".to_owned(), OwnedValue::from(autoconnect));
    let updated: HashMap<&str, HashMap<&str, Value<'_>>> = settings
        .iter()
        .map(|(group, values)| {
            (
                group.as_str(),
                values
                    .iter()
                    .map(|(key, value)| (key.as_str(), Value::from(value.clone())))
                    .collect(),
            )
        })
        .collect();
    connection
        .update2(updated, UPDATE2_TO_DISK_NO_REAPPLY, HashMap::new())
        .await?;
    Ok(())
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
            hints: hints.clone(),
            autoconnect: autoconnect(&connection),
        };
        match self.ask(&request).await? {
            Answer::Secret {
                secret,
                username,
                autoconnect,
            } => {
                if autoconnect != request.autoconnect {
                    let system = self.system.clone();
                    let path = request.path.clone();
                    match tokio::time::timeout(
                        Duration::from_secs(5),
                        write_autoconnect(&system, &path, autoconnect),
                    )
                    .await
                    {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            tracing::warn!(%error, "could not write connection.autoconnect");
                        }
                        Err(_) => {
                            tracing::warn!("connection.autoconnect write timed out");
                        }
                    }
                }
                Ok(answer_for(
                    &setting_name,
                    &hints,
                    &secret,
                    username.as_deref(),
                ))
            }
            Answer::Refused => Err(AgentError::UserCanceled("refused".to_owned())),
        }
    }

    async fn cancel_get_secrets(
        &self,
        path: ObjectPath<'_>,
        setting_name: String,
    ) -> Result<(), AgentError> {
        let _ = self.input.send(Input::Withdraw {
            source: Source::Network,
            key: Some(format!("{}:{setting_name}", path.as_str())),
        });
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

pub async fn register(connection: &zbus::Connection) -> zbus::Result<()> {
    let manager = nm::AgentManagerProxy::new(connection).await?;
    let _ = manager.unregister().await;
    manager
        .register_with_capabilities(IDENTIFIER, nm::AGENT_CAPABILITY_VPN_HINTS)
        .await
}

pub async fn start(connection: zbus::Connection, input: Sender<Input>) {
    let agent = Agent::new(input, connection.clone());
    if let Err(error) = connection.object_server().at(PATH, agent).await {
        tracing::warn!(%error, "could not export the network secret agent");
        return;
    }
    crate::agents::on_owner(&connection, nm::SERVICE, || async {
        if let Err(error) = register(&connection).await {
            tracing::warn!(%error, "could not register the network secret agent");
        }
    })
    .await;
}

#[cfg(test)]
mod tests;
