use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

pub trait BrokerHandle: Send + Sync + 'static {
    fn publish(&self, topic: &'static str, data: Value);
    fn unsubscribe(&self, id: SubscriptionId);
}

#[cfg(test)]
pub(crate) mod mock {
    use std::sync::Mutex;

    use super::{BrokerHandle, SubscriptionId, Value};

    #[derive(Default)]
    pub(crate) struct MockBroker {
        published: Mutex<Vec<(&'static str, Value)>>,
    }

    impl MockBroker {
        pub(crate) fn published(&self) -> Vec<(&'static str, Value)> {
            self.lock().clone()
        }

        fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(&'static str, Value)>> {
            self.published
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }
    }

    impl BrokerHandle for MockBroker {
        fn publish(&self, topic: &'static str, data: Value) {
            self.lock().push((topic, data));
        }

        fn unsubscribe(&self, _id: SubscriptionId) {}
    }
}
