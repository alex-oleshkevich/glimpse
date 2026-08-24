use std::sync::Arc;

use serde::Serialize;

use crate::broker_handle::BrokerHandle;

pub struct Publisher<P> {
    topic: &'static str,
    broker: Arc<dyn BrokerHandle>,
    last: Option<P>,
}

impl<P: Serialize + PartialEq> Publisher<P> {
    pub(crate) fn new(topic: &'static str, broker: Arc<dyn BrokerHandle>) -> Self {
        Self {
            topic,
            broker,
            last: None,
        }
    }

    pub fn set(&mut self, value: P) {
        if self.last.as_ref() == Some(&value) {
            return;
        }

        match serde_json::to_value(&value) {
            Ok(data) => {
                self.broker.publish(self.topic, data);
                self.last = Some(value);
            }
            Err(error) => {
                tracing::error!(topic = self.topic, %error, "payload failed to serialize");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Serializer;
    use serde_json::{Value, json};

    use super::*;
    use crate::broker_handle::mock::MockBroker;

    const TOPIC: &str = "test.topic";

    fn publisher() -> (Arc<MockBroker>, Publisher<u32>) {
        let broker = Arc::new(MockBroker::default());
        let publisher = Publisher::new(TOPIC, broker.clone());
        (broker, publisher)
    }

    fn values(broker: &MockBroker) -> Vec<Value> {
        broker
            .published()
            .into_iter()
            .map(|(_, data)| data)
            .collect()
    }

    #[test]
    fn publishes_the_first_value() {
        let (broker, mut publisher) = publisher();

        publisher.set(1);

        assert_eq!(broker.published(), vec![(TOPIC.to_owned(), json!(1))]);
    }

    #[test]
    fn gates_an_unchanged_value() {
        let (broker, mut publisher) = publisher();

        publisher.set(1);
        publisher.set(1);

        assert_eq!(values(&broker), vec![json!(1)]);
    }

    #[test]
    fn publishes_again_once_the_value_moves() {
        let (broker, mut publisher) = publisher();

        publisher.set(1);
        publisher.set(2);
        publisher.set(1);

        assert_eq!(values(&broker), vec![json!(1), json!(2), json!(1)]);
    }

    #[derive(PartialEq)]
    struct Unserializable;

    impl Serialize for Unserializable {
        fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("unserializable"))
        }
    }

    #[test]
    fn publishes_nothing_when_a_payload_fails_to_serialize() {
        let broker = Arc::new(MockBroker::default());
        let mut publisher = Publisher::new(TOPIC, broker.clone());

        publisher.set(Unserializable);

        assert!(broker.published().is_empty());
    }
}
