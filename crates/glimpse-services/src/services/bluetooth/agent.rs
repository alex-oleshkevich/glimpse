use tokio::sync::{mpsc, oneshot};
use zbus::zvariant::ObjectPath;

use glimpse_dbus::bluez::AgentManager1Proxy;

use crate::service::Input;

use super::{Bluetooth, DeviceId, Event, Prompt};

pub const PATH: &str = "/me/aresa/glimpse/bluetooth/agent";
const CAPABILITY: &str = "KeyboardDisplay";

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum AgentError {
    Rejected(String),
    Canceled(String),
}

pub struct Agent {
    events: mpsc::Sender<Input<Bluetooth>>,
}

impl Agent {
    pub fn new(events: mpsc::Sender<Input<Bluetooth>>) -> Self {
        Self { events }
    }

    async fn ask(&self, prompt: Prompt) -> Result<Answer, AgentError> {
        let (answer, reply) = oneshot::channel();
        self.events
            .send(Input::Event(Event::Prompt(prompt, Some(answer))))
            .await
            .map_err(|_| AgentError::Canceled("the panel is not accepting prompts".to_owned()))?;

        match reply.await {
            Ok(Answer::Confirm) => Ok(Answer::Confirm),
            Ok(answer) => Ok(answer),
            Err(_) => Err(AgentError::Canceled("the prompt went away".to_owned())),
        }
    }

    async fn authorized(&self, device: DeviceId) -> bool {
        let (answer, reply) = oneshot::channel();
        if self
            .events
            .send(Input::Event(Event::AuthorizeService {
                device,
                reply: answer,
            }))
            .await
            .is_err()
        {
            return false;
        }
        reply.await.unwrap_or(false)
    }

    async fn show(&self, prompt: Prompt) {
        let _ = self
            .events
            .send(Input::Event(Event::Prompt(prompt, None)))
            .await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Confirm,
    Deny,
    Pin(String),
    Passkey(u32),
}

fn denied() -> AgentError {
    AgentError::Rejected("the request was refused".to_owned())
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self) {
        let _ = self.events.send(Input::Event(Event::AgentReleased)).await;
    }

    async fn request_pin_code(&self, device: ObjectPath<'_>) -> Result<String, AgentError> {
        match self
            .ask(Prompt::RequestPin(DeviceId(device.to_string())))
            .await?
        {
            Answer::Pin(pin) => Ok(pin),
            _ => Err(denied()),
        }
    }

    async fn request_passkey(&self, device: ObjectPath<'_>) -> Result<u32, AgentError> {
        match self
            .ask(Prompt::RequestPasskey(DeviceId(device.to_string())))
            .await?
        {
            Answer::Passkey(passkey) => Ok(passkey),
            _ => Err(denied()),
        }
    }

    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
    ) -> Result<(), AgentError> {
        match self
            .ask(Prompt::Confirm {
                device: DeviceId(device.to_string()),
                passkey,
            })
            .await?
        {
            Answer::Confirm => Ok(()),
            _ => Err(denied()),
        }
    }

    async fn request_authorization(&self, device: ObjectPath<'_>) -> Result<(), AgentError> {
        match self
            .ask(Prompt::Authorize(DeviceId(device.to_string())))
            .await?
        {
            Answer::Confirm => Ok(()),
            _ => Err(denied()),
        }
    }

    async fn display_pin_code(
        &self,
        device: ObjectPath<'_>,
        pincode: String,
    ) -> Result<(), AgentError> {
        self.show(Prompt::DisplayPin {
            device: DeviceId(device.to_string()),
            pin: glimpse_utils::clean(&pincode, 32),
        })
        .await;
        Ok(())
    }

    async fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, entered: u16) {
        self.show(Prompt::DisplayPasskey {
            device: DeviceId(device.to_string()),
            passkey,
            entered,
        })
        .await;
    }

    async fn authorize_service(
        &self,
        device: ObjectPath<'_>,
        _uuid: String,
    ) -> Result<(), AgentError> {
        match self.authorized(DeviceId(device.to_string())).await {
            true => Ok(()),
            false => Err(AgentError::Rejected(
                "glimpse authorizes services for bonded devices only".to_owned(),
            )),
        }
    }

    async fn cancel(&self) {
        let _ = self.events.send(Input::Event(Event::PromptGone)).await;
    }
}

pub async fn register(
    connection: &zbus::Connection,
    events: mpsc::Sender<Input<Bluetooth>>,
) -> zbus::Result<()> {
    let exported = connection
        .object_server()
        .at(PATH, Agent::new(events))
        .await?;

    let path = ObjectPath::try_from(PATH)?;
    let manager = AgentManager1Proxy::new(connection).await?;
    match manager.register_agent(&path, CAPABILITY).await {
        Ok(()) => {
            if let Err(error) = manager.request_default_agent(&path).await {
                tracing::warn!(%error, "bluez did not make glimpse the default agent");
            }
            Ok(())
        }
        Err(error) => {
            if exported {
                let _ = connection.object_server().remove::<Agent, _>(PATH).await;
            }
            Err(error)
        }
    }
}

pub async fn unregister(connection: &zbus::Connection) {
    if let Ok(path) = ObjectPath::try_from(PATH)
        && let Ok(manager) = AgentManager1Proxy::new(connection).await
    {
        let _ = manager.unregister_agent(&path).await;
    }
    let _ = connection.object_server().remove::<Agent, _>(PATH).await;
}

pub async fn withdraw(connection: &zbus::Connection) {
    let _ = connection.object_server().remove::<Agent, _>(PATH).await;
}

#[cfg(test)]
mod tests;
