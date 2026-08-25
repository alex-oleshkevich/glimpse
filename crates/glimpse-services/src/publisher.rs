use std::sync::Arc;

use serde::Serialize;

use crate::BrokerHandle;

pub struct Publisher<P> {
    topic: &'static str,
    broker: Arc<BrokerHandle>,
    last: Option<P>,
}

impl<P: Serialize + PartialEq> Publisher<P> {
    pub(crate) fn new(topic: &'static str, broker: Arc<BrokerHandle>) -> Self {
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
