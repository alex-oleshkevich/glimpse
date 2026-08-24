use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

pub trait BrokerHandle: Send + Sync + 'static {
    fn publish(&self, topic: &str, data: Value);
    fn unsubscribe(&self, id: SubscriptionId);
}

#[cfg(test)]
pub(crate) mod mock {
    use std::sync::Mutex;

    use super::{BrokerHandle, SubscriptionId, Value};

    #[derive(Default)]
    pub(crate) struct MockBroker {
        published: Mutex<Vec<(String, Value)>>,
    }

    impl MockBroker {
        pub(crate) fn published(&self) -> Vec<(String, Value)> {
            self.published.lock().expect("mock broker poisoned").clone()
        }
    }

    impl BrokerHandle for MockBroker {
        fn publish(&self, topic: &str, data: Value) {
            self.published
                .lock()
                .expect("mock broker poisoned")
                .push((topic.to_owned(), data));
        }

        fn unsubscribe(&self, _id: SubscriptionId) {}
    }
}
