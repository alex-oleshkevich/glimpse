use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use glimpse_ipc::{CallError, ErrorCode};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::oneshot;

pub use glimpse_contracts::ServiceState;

pub type Sink = Box<dyn Fn(&Value) + Send>;

/// Routes one `call` to the service that declared it. Erased where the concrete service is still
/// known — at registration — because the broker is a single task with no type parameter to spare.
pub type Dispatch = Box<dyn Fn(&str, Value, Responder) + Send>;

/// The reply channel for one `call`, held until the service answers. A handler that has to await a
/// backend moves this into `ctx.spawn` instead of answering inline, so one slow backend does not
/// stall every other command the service owns.
pub struct Responder {
    reply: Option<oneshot::Sender<Result<Value, CallError>>>,
}

impl Responder {
    pub fn new(reply: oneshot::Sender<Result<Value, CallError>>) -> Self {
        Self { reply: Some(reply) }
    }

    pub fn ok<T: Serialize>(mut self, output: T) {
        let outcome = serde_json::to_value(output).map_err(|error| {
            CallError::new(
                ErrorCode::Internal,
                format!("the result did not serialize: {error}"),
            )
        });
        self.answer(outcome);
    }

    pub fn fail(mut self, error: CallError) {
        self.answer(Err(error));
    }

    fn answer(&mut self, outcome: Result<Value, CallError>) {
        // A caller that gave up is not an error: the result simply has nowhere to go.
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(outcome);
        }
    }
}

/// A command can reach here unanswered three ways: it was still queued when the service stopped,
/// the handler panicked mid-command, or the handler simply forgot. All three mean it did not take
/// effect and may well succeed later, so the answer is `Unavailable` — the caller would otherwise
/// wait out its whole timeout with nothing said anywhere.
impl Drop for Responder {
    fn drop(&mut self) {
        if self.reply.is_some() {
            tracing::warn!("a command was dropped without an answer");
            self.answer(Err(CallError::new(
                ErrorCode::Unavailable,
                "the service did not answer",
            )));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

/// Declared here and implemented in `glimpsed`, so a service reaches the broker without depending
/// on the daemon and every service test can run against `MockBroker` with no socket and no task.
///
/// Every method is synchronous and must not block: `publish` is called from a service handler, and
/// a broker that made its callers wait would pay one service's latency out of every other's.
pub trait BrokerHandle: Send + Sync + 'static {
    fn publish(&self, topic: &str, data: Value);
    fn subscribe(&self, topic: &str, sink: Sink) -> SubscriptionId;
    fn unsubscribe(&self, id: SubscriptionId);
    fn report_health(&self, service: &'static str, state: ServiceState);
}

/// Records what a service did instead of routing it.
#[derive(Default)]
pub struct MockBroker {
    published: Mutex<Vec<(String, Value)>>,
    health: Mutex<Vec<(&'static str, ServiceState)>>,
    sinks: Mutex<Vec<(String, Sink)>>,
    next_id: AtomicU64,
}

impl MockBroker {
    pub fn published(&self) -> Vec<(String, Value)> {
        self.published
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn health(&self) -> Vec<(&'static str, ServiceState)> {
        self.health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn deliver(&self, topic: &str, data: &Value) {
        for (subscribed, sink) in self
            .sinks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
        {
            if subscribed == topic {
                sink(data);
            }
        }
    }
}

impl BrokerHandle for MockBroker {
    fn publish(&self, topic: &str, data: Value) {
        self.published
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((topic.to_owned(), data));
    }

    /// Replays the topic's latest value the way the broker does, so a test agrees with the daemon
    /// about when a new subscriber first hears anything.
    fn subscribe(&self, topic: &str, sink: Sink) -> SubscriptionId {
        let latest = self
            .published
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .rev()
            .find(|(published, _)| published == topic)
            .map(|(_, data)| data.clone());

        if let Some(data) = latest {
            sink(&data);
        }
        self.sinks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((topic.to_owned(), sink));
        SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn unsubscribe(&self, _id: SubscriptionId) {}

    fn report_health(&self, service: &'static str, state: ServiceState) {
        self.health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((service, state));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// The failure this guards against is silence: without the `Drop` impl the caller waits out its
    /// whole timeout and no log line explains why.
    #[tokio::test]
    async fn a_dropped_responder_answers_instead_of_leaving_the_caller_waiting() {
        let (reply, answer) = oneshot::channel();
        drop(Responder::new(reply));

        let error = answer.await.expect("answered").expect_err("an error");
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(error.retryable, "a dropped command is worth retrying");
    }

    #[tokio::test]
    async fn a_responder_that_answered_does_not_answer_again_on_drop() {
        let (reply, answer) = oneshot::channel();
        Responder::new(reply).ok(7);

        assert_eq!(answer.await.expect("answered"), Ok(Value::from(7)));
    }

    /// Mirrors the daemon. Without it a service test disagrees with the broker about when a new
    /// subscriber first hears anything, which is the difference between a topic that arrives and
    /// one that sits blank forever.
    #[test]
    fn subscribing_after_a_publish_replays_the_latest_value() {
        let mock = MockBroker::default();
        mock.publish("audio.volume", Value::from(0.2));
        mock.publish("audio.volume", Value::from(0.4));

        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();
        mock.subscribe(
            "audio.volume",
            Box::new(move |data| recorder.lock().expect("not poisoned").push(data.clone())),
        );

        assert_eq!(*seen.lock().expect("not poisoned"), [Value::from(0.4)]);
    }

    #[test]
    fn subscribing_to_a_topic_with_no_value_replays_nothing() {
        let mock = MockBroker::default();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();

        mock.subscribe(
            "audio.volume",
            Box::new(move |data| recorder.lock().expect("not poisoned").push(data.clone())),
        );

        assert!(seen.lock().expect("not poisoned").is_empty());
    }
}
