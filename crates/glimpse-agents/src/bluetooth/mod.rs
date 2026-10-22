use std::future::Future;
use std::time::Duration;

use futures_util::StreamExt;
use relm4::Sender;
use tokio::sync::oneshot;
use zbus::zvariant::ObjectPath;

use glimpse_dbus::bluez::{AgentManager1Proxy, Device1Proxy, SERVICE};

use crate::ask::{Ask, Input, Prompt, Reply, Source};

pub const PATH: &str = "/me/aresa/glimpse/bluetooth/agent";
const CAPABILITY: &str = "KeyboardDisplay";
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const NAME_CAP: usize = 64;
const PIN_CAP: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub device: String,
    pub name: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Confirm { passkey: u32 },
    Authorize,
    Pin,
    Passkey,
    ShowPin { pin: String },
    ShowPasskey { passkey: u32, entered: u16 },
}

#[derive(Clone, PartialEq, Eq)]
pub enum Answer {
    Confirm,
    Deny,
    Pin(String),
    Passkey(u32),
}

impl std::fmt::Debug for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Confirm => f.write_str("Confirm"),
            Self::Deny => f.write_str("Deny"),
            Self::Pin(_) => f.write_str("Pin(<redacted>)"),
            Self::Passkey(_) => f.write_str("Passkey(<redacted>)"),
        }
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum AgentError {
    Rejected(String),
    Canceled(String),
}

pub struct Agent {
    input: Sender<Input>,
    system: zbus::Connection,
}

impl Agent {
    fn new(system: zbus::Connection, input: Sender<Input>) -> Self {
        Self { input, system }
    }

    async fn ask(&self, device: &str, kind: Kind) -> Result<Answer, AgentError> {
        let (reply, answer) = oneshot::channel();
        let ask = Ask {
            source: Source::Bluetooth,
            key: device.to_owned(),
            prompt: Prompt::Pairing(Request {
                device: device.to_owned(),
                name: alias(&self.system, device).await,
                kind,
            }),
            reply: Reply::Pairing(reply),
        };
        if self.input.send(Input::Ask(ask)).is_err() {
            return Err(canceled("the panel is not accepting prompts"));
        }
        match answer.await {
            Ok(answer) => Ok(answer),
            Err(_) => Err(canceled("the prompt went away")),
        }
    }

    async fn show(&self, device: &str, kind: Kind) {
        let key = device.to_owned();
        let _ = self.input.send(Input::Ask(Ask {
            source: Source::Bluetooth,
            key: key.clone(),
            prompt: Prompt::Pairing(Request {
                device: key,
                name: alias(&self.system, device).await,
                kind,
            }),
            reply: Reply::Shown,
        }));
    }

    fn withdraw_all(&self) {
        let _ = self.input.send(Input::Withdraw {
            source: Source::Bluetooth,
            key: None,
        });
    }
}

fn canceled(reason: &str) -> AgentError {
    AgentError::Canceled(reason.to_owned())
}

fn refused() -> AgentError {
    AgentError::Rejected("the request was refused".to_owned())
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self) {
        self.withdraw_all();
    }

    async fn request_pin_code(&self, device: ObjectPath<'_>) -> Result<String, AgentError> {
        match self.ask(device.as_str(), Kind::Pin).await? {
            Answer::Pin(pin) => Ok(pin),
            _ => Err(refused()),
        }
    }

    async fn request_passkey(&self, device: ObjectPath<'_>) -> Result<u32, AgentError> {
        match self.ask(device.as_str(), Kind::Passkey).await? {
            Answer::Passkey(passkey) => Ok(passkey),
            _ => Err(refused()),
        }
    }

    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
    ) -> Result<(), AgentError> {
        match self.ask(device.as_str(), Kind::Confirm { passkey }).await? {
            Answer::Confirm => Ok(()),
            _ => Err(refused()),
        }
    }

    async fn request_authorization(&self, device: ObjectPath<'_>) -> Result<(), AgentError> {
        match self.ask(device.as_str(), Kind::Authorize).await? {
            Answer::Confirm => Ok(()),
            _ => Err(refused()),
        }
    }

    async fn display_pin_code(
        &self,
        device: ObjectPath<'_>,
        pincode: String,
    ) -> Result<(), AgentError> {
        self.show(
            device.as_str(),
            Kind::ShowPin {
                pin: glimpse_utils::clean(&pincode, PIN_CAP),
            },
        )
        .await;
        Ok(())
    }

    async fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, entered: u16) {
        self.show(device.as_str(), Kind::ShowPasskey { passkey, entered })
            .await;
    }

    async fn authorize_service(
        &self,
        device: ObjectPath<'_>,
        _uuid: String,
    ) -> Result<(), AgentError> {
        match bonded(&self.system, device.as_str()).await {
            true => Ok(()),
            false => Err(AgentError::Rejected(
                "glimpse authorizes services for bonded devices only".to_owned(),
            )),
        }
    }

    async fn cancel(&self) {
        self.withdraw_all();
    }
}

pub async fn spawn(system: zbus::Connection, input: Sender<Input>) {
    let agent = Agent::new(system.clone(), input);
    if let Err(error) = system.object_server().at(PATH, agent).await {
        tracing::warn!(%error, "could not export the bluetooth pairing agent");
        return;
    }
    crate::agents::on_owner(&system, SERVICE, || register(&system)).await;
}

async fn register(system: &zbus::Connection) {
    let Ok(path) = ObjectPath::try_from(PATH) else {
        return;
    };
    let manager = match AgentManager1Proxy::new(system).await {
        Ok(manager) => manager,
        Err(error) => {
            tracing::warn!(%error, "could not reach the bluetooth agent manager");
            return;
        }
    };
    match manager.register_agent(&path, CAPABILITY).await {
        Ok(()) => {}
        Err(error) if already_registered(&error) => {}
        Err(error) => {
            tracing::warn!(%error, "could not register the bluetooth pairing agent");
            return;
        }
    }
    if let Err(error) = manager.request_default_agent(&path).await {
        tracing::warn!(%error, "bluez did not make glimpse the default agent");
    }
}

fn already_registered(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => name.as_str() == "org.bluez.Error.AlreadyExists",
        _ => false,
    }
}

async fn read<T>(path: &str, read: impl Future<Output = zbus::Result<T>>) -> Option<T> {
    match tokio::time::timeout(READ_TIMEOUT, read).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            tracing::debug!(%error, path, "bluetooth device read failed");
            None
        }
        Err(_) => {
            tracing::debug!(path, "bluetooth device read timed out");
            None
        }
    }
}

async fn device_proxy<'a>(system: &'a zbus::Connection, path: &'a str) -> Option<Device1Proxy<'a>> {
    read(path, async {
        Device1Proxy::builder(system).path(path)?.build().await
    })
    .await
}

async fn alias(system: &zbus::Connection, path: &str) -> String {
    let name = read(path, async {
        Device1Proxy::builder(system)
            .path(path)?
            .build()
            .await?
            .alias()
            .await
    });
    name.await
        .map(|name| glimpse_utils::clean(&name, NAME_CAP))
        .unwrap_or_default()
}

async fn bonded(system: &zbus::Connection, path: &str) -> bool {
    let bonded = read(path, async {
        Device1Proxy::builder(system)
            .path(path)?
            .build()
            .await?
            .bonded()
            .await
    });
    bonded.await.unwrap_or(false)
}

pub(crate) async fn follow_paired(system: &zbus::Connection, device: &str, input: &Sender<Input>) {
    let watch = async {
        let Some(proxy) = device_proxy(system, device).await else {
            return;
        };
        let mut changes = proxy.receive_paired_changed().await;
        if proxy.paired().await.unwrap_or(false) {
            withdraw_device(input, device);
            return;
        }
        while let Some(change) = changes.next().await {
            if change.get().await.unwrap_or(false) {
                withdraw_device(input, device);
                return;
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(60), watch).await;
}

fn withdraw_device(input: &Sender<Input>, device: &str) {
    let _ = input.send(Input::Withdraw {
        source: Source::Bluetooth,
        key: Some(device.to_owned()),
    });
}

#[cfg(test)]
mod tests;
