mod handle;
mod store;

use std::collections::HashMap;

use glimpse_contracts::{Message as _, ServiceState, SystemServices, SystemTopics};
use glimpse_ipc::{CallError, Event, Publisher, Subscribed};
use glimpse_services::{Sink, SubscriptionId};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub use handle::Handle;
use store::Store;

const MAILBOX: usize = 256;

pub enum Message {
    Declare {
        service: &'static str,
        topics: &'static [&'static str],
    },
    SetPublisher(Publisher),
    Publish {
        topic: String,
        data: Value,
    },
    Health {
        service: &'static str,
        state: ServiceState,
    },
    Subscribe {
        id: SubscriptionId,
        topic: String,
        sink: Sink,
    },
    Unsubscribe {
        id: SubscriptionId,
    },
    Get {
        topic: String,
        reply: oneshot::Sender<Result<Option<Event>, CallError>>,
    },
    Matching {
        pattern: String,
        reply: oneshot::Sender<Subscribed>,
    },
}

/// One task owns every topic value and every subscription. It routes and nothing else: no
/// filesystem access, no decoding, and no synchronous write to a client — writes go to the
/// per-client outboxes inside `glimpse-ipc`, which the publisher only ever appends to.
pub struct Broker {
    store: Store,
    sinks: HashMap<SubscriptionId, (String, Sink)>,
    publisher: Option<Publisher>,
}

pub fn spawn(cancel: CancellationToken) -> Handle {
    let (tx, rx) = mpsc::channel(MAILBOX);
    let handle = Handle::new(tx);
    tokio::spawn(
        Broker {
            store: Store::new(),
            sinks: HashMap::new(),
            publisher: None,
        }
        .run(rx, cancel),
    );
    handle
}

impl Broker {
    async fn run(mut self, mut inbox: mpsc::Receiver<Message>, cancel: CancellationToken) {
        tracing::debug!("broker started");
        loop {
            let message = tokio::select! {
                () = cancel.cancelled() => break,
                message = inbox.recv() => match message {
                    Some(message) => message,
                    None => break,
                },
            };
            self.handle(message);
        }
        tracing::debug!("broker stopped");
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Declare { service, topics } => {
                self.store.declare(service, topics);
                self.announce();
            }
            Message::SetPublisher(publisher) => self.publisher = Some(publisher),
            Message::Publish { topic, data } => {
                if let Some((event, first)) = self.store.publish(&topic, data) {
                    self.deliver(event);
                    if first {
                        self.announce_topics();
                    }
                }
            }
            Message::Health { service, state } => self.health(service, state),
            Message::Subscribe { id, topic, sink } => {
                self.sinks.insert(id, (topic, sink));
            }
            Message::Unsubscribe { id } => {
                self.sinks.remove(&id);
            }
            Message::Get { topic, reply } => {
                let _ = reply.send(self.store.get(&topic));
            }
            Message::Matching { pattern, reply } => {
                let _ = reply.send(self.store.matching(&pattern));
            }
        }
    }

    fn health(&mut self, service: &'static str, state: ServiceState) {
        let restamp = self.store.set_state(service, state);
        self.announce();

        // Only a transition that crossed the stale boundary reaches here, so this is not churn: an
        // already-connected subscriber has no other way to learn that its data froze.
        for topic in restamp {
            if let Some(event) = self.store.restamp(&topic) {
                self.deliver(event);
            }
        }
    }

    fn announce(&mut self) {
        let services = self.store.services();
        self.publish_own(SystemServices::NAME, services);
        self.announce_topics();
    }

    fn announce_topics(&mut self) {
        let topics = self.store.topics();
        self.publish_own(SystemTopics::NAME, topics);
    }

    fn publish_own(&mut self, topic: &'static str, payload: impl serde::Serialize) {
        let data = match serde_json::to_value(payload) {
            Ok(data) => data,
            Err(error) => {
                tracing::error!(topic, %error, "broker payload failed to serialize");
                return;
            }
        };
        if let Some((event, _)) = self.store.publish(topic, data) {
            self.deliver(event);
        }
    }

    fn deliver(&self, event: Event) {
        for (topic, sink) in self.sinks.values() {
            if *topic == event.topic {
                sink(&event.data);
            }
        }

        if let Some(publisher) = &self.publisher {
            publisher.publish(event);
        }
    }
}
