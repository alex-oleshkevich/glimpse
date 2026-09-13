use glimpse_contracts::ServiceState;
use serde_json::Value;
use tokio::sync::mpsc;

use super::Message;

/// The services' side of the broker. Every method hands work to the task and returns immediately —
/// a service handler must never wait on the broker, or one service's latency becomes everyone's.
#[derive(Clone)]
pub struct Handle {
    tx: mpsc::Sender<Message>,
}

impl Handle {
    pub(super) fn new(tx: mpsc::Sender<Message>) -> Self {
        Self { tx }
    }

    pub fn send(&self, message: Message) {
        match self.tx.try_send(message) {
            Ok(()) => {}
            // A wedged broker is a real fault and dropping is the only option that does not block a
            // service handler, so it is loud.
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::error!("the broker mailbox is full, dropped a message");
            }
            // A closed one is just shutdown: the broker stops before the last service does, and
            // reporting that as a fault every time would train everyone to ignore the loud case.
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::debug!("the broker has stopped, dropped a message");
            }
        }
    }
}

impl Handle {
    pub fn publish(&self, topic: &str, data: Value) {
        self.send(Message::Publish {
            topic: topic.to_owned(),
            data,
        });
    }

    pub fn report_health(&self, service: &'static str, state: ServiceState) {
        self.send(Message::Health { service, state });
    }
}
