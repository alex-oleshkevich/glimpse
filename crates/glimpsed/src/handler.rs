use glimpse_ipc::{CallError, ClientId, ErrorCode, Event, Handler};
use serde_json::Value;

/// Every answer is a refusal until the broker owns a topic registry: no service reaches a client
/// through it yet, and claiming a topic exists would be worse than saying it does not.
pub struct BrokerHandler;

impl Handler for BrokerHandler {
    async fn subscribe(&self, client: ClientId, pattern: &str) -> Result<usize, CallError> {
        tracing::debug!(?client, pattern, "subscribe");
        Ok(0)
    }

    async fn unsubscribe(&self, client: ClientId, pattern: &str) {
        tracing::debug!(?client, pattern, "unsubscribe");
    }

    async fn get(&self, topic: &str) -> Result<Option<Event>, CallError> {
        Err(CallError::new(
            ErrorCode::UnknownTopic,
            format!("no service declares `{topic}`"),
        ))
    }

    async fn call(&self, command: &str, _args: Value) -> Result<Value, CallError> {
        Err(CallError::new(
            ErrorCode::UnknownCommand,
            format!("no service declares `{command}`"),
        ))
    }

    async fn disconnected(&self, client: ClientId) {
        tracing::debug!(?client, "forgetting subscriptions");
    }
}
