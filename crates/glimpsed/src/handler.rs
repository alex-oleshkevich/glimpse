use glimpse_ipc::{CallError, ClientId, ErrorCode, Event, Handler, Subscribed};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::broker::{Handle, Message};

/// Turns socket requests into broker messages. It holds no state of its own: which client is
/// subscribed to what is the server's bookkeeping, and what exists is the broker's.
pub struct BrokerHandler {
    broker: Handle,
}

impl BrokerHandler {
    pub fn new(broker: Handle) -> Self {
        Self { broker }
    }
}

impl Handler for BrokerHandler {
    async fn subscribe(&self, client: ClientId, pattern: &str) -> Result<Subscribed, CallError> {
        tracing::debug!(?client, pattern, "subscribe");
        let (reply, answer) = oneshot::channel();
        self.broker.send(Message::Matching {
            pattern: pattern.to_owned(),
            reply,
        });
        answer.await.map_err(|_| gone())
    }

    async fn unsubscribe(&self, client: ClientId, pattern: &str) {
        tracing::debug!(?client, pattern, "unsubscribe");
    }

    async fn get(&self, topic: &str) -> Result<Option<Event>, CallError> {
        let (reply, answer) = oneshot::channel();
        self.broker.send(Message::Get {
            topic: topic.to_owned(),
            reply,
        });
        answer.await.map_err(|_| gone())?
    }

    async fn call(&self, command: &str, args: Value) -> Result<Value, CallError> {
        tracing::debug!(command, "call");
        let (reply, answer) = oneshot::channel();
        self.broker.send(Message::Call {
            method: command.to_owned(),
            args,
            reply,
        });
        answer.await.map_err(|_| gone())?
    }

    async fn disconnected(&self, client: ClientId) {
        tracing::debug!(?client, "client gone");
    }
}

fn gone() -> CallError {
    CallError::new(ErrorCode::Unavailable, "the broker is not answering")
}
