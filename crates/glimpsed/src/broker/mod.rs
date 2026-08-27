mod handle;
mod store;

use std::collections::HashMap;

use glimpse_contracts::{Message as _, ServiceState, SystemMethods, SystemServices, SystemTopics};
use glimpse_ipc::{CallError, ErrorCode, Event, Publisher, Subscribed};
use glimpse_services::{Dispatch, Responder, Sink, SubscriptionId};
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
        methods: &'static [&'static str],
        /// Carried with the declaration rather than sent separately, so declaring a method without
        /// a way to route it cannot be expressed.
        dispatch: Dispatch,
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
    Call {
        method: String,
        args: Value,
        reply: oneshot::Sender<Result<Value, CallError>>,
    },
}

/// One task owns every topic value and every subscription. It routes and nothing else: no
/// filesystem access, no decoding, and no synchronous write to a client — writes go to the
/// per-client outboxes inside `glimpse-ipc`, which the publisher only ever appends to.
pub struct Broker {
    store: Store,
    sinks: HashMap<SubscriptionId, (String, Sink)>,
    dispatchers: HashMap<&'static str, Dispatch>,
    publisher: Option<Publisher>,
}

pub fn spawn(cancel: CancellationToken) -> Handle {
    let (tx, rx) = mpsc::channel(MAILBOX);
    let handle = Handle::new(tx);
    tokio::spawn(
        Broker {
            store: Store::new(),
            sinks: HashMap::new(),
            dispatchers: HashMap::new(),
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
            Message::Declare {
                service,
                topics,
                methods,
                dispatch,
            } => {
                self.store.declare(service, topics, methods);
                self.dispatchers.insert(service, dispatch);
                self.announce_services();
                self.announce_topics();
                self.announce_methods();
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
            // The stored value first: a service that subscribes after the producer already
            // published would otherwise wait for a change that may never come, since the
            // publisher's equality gate never republishes an unchanged value. A `manual` location
            // publishes exactly once, and solar subscribes after it.
            Message::Subscribe { id, topic, sink } => {
                if let Ok(Some(event)) = self.store.get(&topic) {
                    sink(&event.data);
                }
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
            Message::Call {
                method,
                args,
                reply,
            } => self.call(&method, args, Responder::new(reply)),
        }
    }

    /// The dispatcher hands the command to its service's inbox and returns; the service answers the
    /// responder, possibly from a spawned task. Nothing here awaits the result.
    fn call(&self, method: &str, args: Value, responder: Responder) {
        let dispatch = self
            .store
            .method_owner(method)
            .and_then(|service| self.dispatchers.get(service));

        match dispatch {
            Some(dispatch) => dispatch(method, args, responder),
            None => responder.fail(CallError::new(
                ErrorCode::UnknownCommand,
                format!("no service declares `{method}`"),
            )),
        }
    }

    fn health(&mut self, service: &'static str, state: ServiceState) {
        let restamp = self.store.set_state(service, state);
        self.announce_services();

        // Only a transition that crossed the stale boundary reaches here, so this is not churn: an
        // already-connected subscriber has no other way to learn that its data froze.
        for topic in restamp {
            if let Some(event) = self.store.restamp(&topic) {
                self.deliver(event);
            }
        }
    }

    fn announce_services(&mut self) {
        let services = self.store.services();
        self.publish_own(SystemServices::NAME, services);
    }

    fn announce_topics(&mut self) {
        let topics = self.store.topics();
        self.publish_own(SystemTopics::NAME, topics);
    }

    fn announce_methods(&mut self) {
        let methods = self.store.methods();
        self.publish_own(SystemMethods::NAME, methods);
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    type Seen = Arc<Mutex<Vec<Value>>>;

    fn declared() -> Broker {
        let mut broker = Broker {
            store: Store::new(),
            sinks: HashMap::new(),
            dispatchers: HashMap::new(),
            publisher: None,
        };
        broker.handle(Message::Declare {
            service: "audio",
            topics: &["audio.volume"],
            methods: &[],
            dispatch: Box::new(|_method, _args, _responder| {}),
        });
        broker
    }

    fn watch(broker: &mut Broker, topic: &str) -> Seen {
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();
        broker.handle(Message::Subscribe {
            id: SubscriptionId(1),
            topic: topic.to_owned(),
            sink: Box::new(move |data| recorder.lock().expect("not poisoned").push(data.clone())),
        });
        seen
    }

    /// Without the replay, a service subscribing after the producer published sits blank until the
    /// value happens to change — and the publisher's equality gate means a one-shot producer never
    /// changes it. That is a `manual` location against solar.
    #[test]
    fn a_new_subscriber_is_handed_the_topics_current_value() {
        let mut broker = declared();
        broker.handle(Message::Publish {
            topic: "audio.volume".to_owned(),
            data: Value::from(0.4),
        });

        let seen = watch(&mut broker, "audio.volume");

        assert_eq!(*seen.lock().expect("not poisoned"), [Value::from(0.4)]);
    }

    #[test]
    fn a_declared_topic_with_no_value_replays_nothing() {
        let mut broker = declared();
        let seen = watch(&mut broker, "audio.volume");

        assert!(seen.lock().expect("not poisoned").is_empty());
    }

    #[test]
    fn an_undeclared_topic_replays_nothing_rather_than_failing() {
        let mut broker = declared();
        let seen = watch(&mut broker, "audio.balance");

        assert!(seen.lock().expect("not poisoned").is_empty());
    }
}
