use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use glimpse_services::{BrokerHandle, ServiceState, Sink, SubscriptionId};
use serde_json::Value;
use tokio::sync::mpsc;

use super::Message;

/// The services' side of the broker. Every method hands work to the task and returns immediately —
/// a service handler must never wait on the broker, or one service's latency becomes everyone's.
#[derive(Clone)]
pub struct Handle {
    tx: mpsc::Sender<Message>,
    next_id: Arc<AtomicU64>,
}

impl Handle {
    pub(super) fn new(tx: mpsc::Sender<Message>) -> Self {
        Self {
            tx,
            next_id: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn send(&self, message: Message) {
        // A full mailbox means the broker is wedged; dropping is the only option that does not
        // block a service handler, and it is loud.
        if let Err(error) = self.tx.try_send(message) {
            tracing::error!(%error, "the broker mailbox is full, dropped a message");
        }
    }
}

impl BrokerHandle for Handle {
    fn publish(&self, topic: &str, data: Value) {
        self.send(Message::Publish {
            topic: topic.to_owned(),
            data,
        });
    }

    fn subscribe(&self, topic: &str, sink: Sink) -> SubscriptionId {
        // The id is minted here rather than by the task, so the caller gets one without waiting.
        let id = SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.send(Message::Subscribe {
            id,
            topic: topic.to_owned(),
            sink,
        });
        id
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        self.send(Message::Unsubscribe { id });
    }

    fn report_health(&self, service: &'static str, state: ServiceState) {
        self.send(Message::Health { service, state });
    }
}
