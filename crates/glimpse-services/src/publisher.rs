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
        if self.last.as_ref().is_some_and(|last| *last == value) {
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
    use super::*;
    use crate::broker_handle::mock::MockBroker;

    fn publisher() -> (Arc<MockBroker>, Publisher<u32>) {
        let broker = Arc::new(MockBroker::default());
        let publisher = Publisher::new("test.topic", broker.clone());
        (broker, publisher)
    }

    #[test]
    fn publishes_the_first_value() {
        let (broker, mut publisher) = publisher();

        publisher.set(1);

        assert_eq!(broker.published(), vec![("test.topic", 1.into())]);
    }

    #[test]
    fn gates_an_unchanged_value() {
        let (broker, mut publisher) = publisher();

        publisher.set(1);
        publisher.set(1);

        assert_eq!(broker.published().len(), 1);
    }

    #[test]
    fn publishes_again_once_the_value_moves() {
        let (broker, mut publisher) = publisher();

        publisher.set(1);
        publisher.set(2);
        publisher.set(1);

        assert_eq!(
            broker.published(),
            vec![
                ("test.topic", 1.into()),
                ("test.topic", 2.into()),
                ("test.topic", 1.into()),
            ]
        );
    }
}
