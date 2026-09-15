use glimpse_dbus::Buses;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::context::Ctx;
use crate::service::{Input, NoConfig, Service, ServiceEndpoint, ServiceError, ServiceState};

pub(crate) struct Probe;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub(crate) enum Watch {
    First,
    Second,
}

impl Service for Probe {
    const NAME: &'static str = "probe";

    type Config = NoConfig;
    type State = ();
    type Handle = ServiceEndpoint<Self>;
    type Command = ();
    type Event = u8;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        endpoint
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    async fn start(
        _ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self)
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}
}

pub(crate) type Inbox = mpsc::Receiver<Input<Probe>>;

pub(crate) fn probe() -> (Ctx<Probe>, Inbox) {
    let (ctx, received, _health) = wired_probe();
    (ctx, received)
}

pub(crate) fn wired_probe() -> (Ctx<Probe>, Inbox, watch::Receiver<ServiceState>) {
    let (events, received) = mpsc::channel(8);
    let (state, _state_rx) = watch::channel(());
    let (health, health_rx) = watch::channel(ServiceState::Starting);
    let ctx = Ctx::new(
        events,
        &CancellationToken::new(),
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    (ctx, received, health_rx)
}

pub(crate) async fn event(received: &mut Inbox) -> Option<u8> {
    match received.recv().await {
        Some(Input::Event(event)) => Some(event),
        _ => None,
    }
}
