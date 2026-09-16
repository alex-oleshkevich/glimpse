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
    match AgentManager1Proxy::new(connection)
        .await?
        .register_agent(&path, CAPABILITY)
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
mod tests {
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
}
