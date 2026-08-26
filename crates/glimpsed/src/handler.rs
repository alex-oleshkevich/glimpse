use std::time::Duration;

use glimpse_ipc::{CallError, ClientId, ErrorCode, Event, Handler, Subscribed};
use serde_json::Value;
use tokio::{sync::oneshot, time::timeout};

use crate::broker::{Handle, Message};

/// A service that never answers must not hold a request open forever. Generous, because the
/// backends behind a command carry their own timeouts inside this one — what it catches is a
/// handler that moved its `Responder` into a task and then wedged, which nothing else notices.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

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
        settle(answer).await
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
        settle(answer).await?
    }

    async fn call(&self, command: &str, args: Value) -> Result<Value, CallError> {
        tracing::debug!(command, "call");
        let (reply, answer) = oneshot::channel();
        self.broker.send(Message::Call {
            method: command.to_owned(),
            args,
            reply,
        });
        settle(answer).await?
    }

    async fn disconnected(&self, client: ClientId) {
        tracing::debug!(?client, "client gone");
    }
}

/// A dropped sender means the broker stopped; the elapsed deadline means a service took the message
/// and never answered. Both are ours to report, and neither is the value the broker sent back.
async fn settle<T>(answer: oneshot::Receiver<T>) -> Result<T, CallError> {
    match timeout(ANSWER_TIMEOUT, answer).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(gone()),
        Err(_) => Err(CallError::new(
            ErrorCode::Timeout,
            format!("no answer within {ANSWER_TIMEOUT:?}"),
        )),
    }
}

fn gone() -> CallError {
    CallError::new(ErrorCode::Unavailable, "the broker is not answering")
}
