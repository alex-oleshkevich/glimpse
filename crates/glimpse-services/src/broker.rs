use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

pub type Sink = Box<dyn Fn(&Value) + Send>;

#[derive(Debug, thiserror::Error)]
pub enum BrokerError {}

pub struct Broker {}

impl Broker {
    pub async fn spawn(cancel: CancellationToken) -> Result<Self, BrokerError> {
        tracing::debug!("broker started");
        let _ = cancel;
        Ok(Self {})
    }

    pub fn handle(&self) -> BrokerHandle {
        BrokerHandle {}
    }
}

pub struct BrokerHandle {}

impl BrokerHandle {
    pub fn publish(&self, topic: &str, data: Value) {
        tracing::trace!(topic, ?data, "publish");
    }

    pub fn subscribe(&self, topic: &str, sink: Sink) -> SubscriptionId {
        tracing::trace!(topic, "subscribe");
        let _ = sink;
        SubscriptionId(0)
    }

    pub fn unsubscribe(&self, id: SubscriptionId) {
        tracing::trace!(?id, "unsubscribe");
    }
}
