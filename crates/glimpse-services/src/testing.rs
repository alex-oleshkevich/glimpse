use std::sync::Arc;

use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::context::Ctx;
use crate::service::{Input, Service, ServiceError};
use crate::{BrokerHandle, MockBroker};

/// A service that does nothing, for exercising the framework rather than any behaviour of its own.
/// Its `Event` is a `u8` so a test can assert on which source delivered what.
pub(crate) struct Probe;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub(crate) enum Watch {
    First,
    Second,
}

impl Service for Probe {
    const NAME: &'static str = "probe";
    const TOPICS: &'static [&'static str] = &[];

    type Config = ();
    type Command = ();
    type Event = u8;
    type SubKey = Watch;

    async fn start(_ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        Ok(Self)
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}

    fn peek_config(_config: &glimpse_config::Config) -> Self::Config {}
}

/// A topic to publish into a subscriber under test, independent of any real contract.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Ping {
    pub(crate) value: u8,
}
glimpse_contracts::topic!(Ping, "test.ping");

pub(crate) type Inbox = mpsc::Receiver<Input<Probe>>;

pub(crate) fn probe() -> (Ctx<Probe>, Inbox) {
    let (ctx, received, _broker) = wired_probe();
    (ctx, received)
}

/// The same, with the broker kept so a test can drive a sink or read what was published.
pub(crate) fn wired_probe() -> (Ctx<Probe>, Inbox, Arc<MockBroker>) {
    let (events, received) = mpsc::channel(8);
    let mock = Arc::new(MockBroker::default());
    let broker: Arc<dyn BrokerHandle> = mock.clone();
    let ctx = Ctx::new(
        events,
        &CancellationToken::new(),
        broker,
        Buses::unavailable("no bus in tests"),
    );
    (ctx, received, mock)
}

pub(crate) async fn event(received: &mut Inbox) -> Option<u8> {
    match received.recv().await {
        Some(Input::Event(event)) => Some(event),
        _ => None,
    }
}
