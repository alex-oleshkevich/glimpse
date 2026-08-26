use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use serde_json::Value;

pub use glimpse_contracts::ServiceState;

pub type Sink = Box<dyn Fn(&Value) + Send>;

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
    subscribed: Mutex<Vec<String>>,
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

    pub fn subscribed(&self) -> Vec<String> {
        self.subscribed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl BrokerHandle for MockBroker {
    fn publish(&self, topic: &str, data: Value) {
        self.published
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((topic.to_owned(), data));
    }

    fn subscribe(&self, topic: &str, _sink: Sink) -> SubscriptionId {
        self.subscribed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(topic.to_owned());
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
