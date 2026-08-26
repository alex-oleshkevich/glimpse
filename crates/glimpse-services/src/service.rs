use std::{any::Any, panic::AssertUnwindSafe, sync::Arc};

use futures_util::FutureExt;
use glimpse_config::Config;
use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{BrokerHandle, Ctx, ServiceState};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("service failed to start")]
    StartError,
    #[error("did not send message: {0}")]
    SendError(String),
}

pub enum Input<S: Service> {
    Event(S::Event),
    Command(S::Command),
    Config(S::Config),
}

pub trait Service: Sized + Send + 'static {
    /// Identifies the service in `system.services` and owns its topics in the registry.
    const NAME: &'static str;

    /// Every topic this service may publish, declared before it starts. The broker needs the
    /// mapping while the service is still stopped: a `get` on one of these has to answer
    /// "declared, no value" rather than "unknown", and a pattern has to match it.
    const TOPICS: &'static [&'static str];

    type Config: PartialEq + Send + 'static;
    type Command: Send + 'static;
    type Event: Send + 'static;

    fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
    ) -> impl Future<Output = Result<Self, ServiceError>> + Send;
    fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) -> impl Future<Output = ()> + Send;
    fn stop(self, ctx: &Ctx<Self>) -> impl Future<Output = ()> + Send {
        let _ = ctx;
        async {}
    }
    fn peek_config(config: &Config) -> Self::Config;
}

const EVENT_BACKLOG_SIZE: usize = 128;

pub struct ServiceSender<S: Service> {
    inbox_tx: mpsc::Sender<Input<S>>,
}

impl<S: Service> ServiceSender<S> {
    pub async fn send(&self, input: Input<S>) -> Result<(), ServiceError> {
        self.inbox_tx
            .send(input)
            .await
            .map_err(|e| ServiceError::SendError(e.to_string()))
    }
}

pub struct ServiceRuntime<S: Service> {
    inbox_sender: mpsc::Sender<Input<S>>,
    inbox: mpsc::Receiver<Input<S>>,
    broker: Arc<dyn BrokerHandle>,
    buses: Buses,
    cancel: CancellationToken,
}

impl<S: Service> ServiceRuntime<S> {
    pub fn new(
        broker_handle: Arc<dyn BrokerHandle>,
        buses: Buses,
        cancel: CancellationToken,
    ) -> Self {
        let (inbox_tx, inbox_rx) = mpsc::channel::<Input<S>>(EVENT_BACKLOG_SIZE);
        Self {
            cancel,
            buses,
            inbox: inbox_rx,
            broker: broker_handle,
            inbox_sender: inbox_tx,
        }
    }

    pub fn sender(&self) -> ServiceSender<S> {
        ServiceSender {
            inbox_tx: self.inbox_sender.clone(),
        }
    }

    pub async fn run(&mut self, config: S::Config) -> Result<(), ServiceError> {
        let (events_tx, mut events_rx) = mpsc::channel::<S::Event>(EVENT_BACKLOG_SIZE);
        let ctx = Ctx::<S>::new(
            events_tx,
            &self.cancel,
            self.broker.clone(),
            self.buses.clone(),
        );

        self.broker.report_health(S::NAME, ServiceState::Starting);
        let mut service = match S::start(&ctx, config).await {
            Ok(service) => service,
            Err(error) => {
                self.report_stopped(Some(error.to_string()));
                return Err(error);
            }
        };
        // A service that judged itself degraded while starting keeps that state: reporting
        // `Running` over the top would erase the reason before anyone could read it.
        if !ctx.is_degraded() {
            self.broker.report_health(S::NAME, ServiceState::Running);
        }

        loop {
            let input = tokio::select! {
                () = self.cancel.cancelled() => break,
                Some(event) = events_rx.recv() => Input::Event(event),
                Some(input) = self.inbox.recv() => input,
                else => break,
            };

            // A panicking handler takes down its own service and nothing else. Unwinding past a
            // `&mut self` the handler was midway through mutating leaves it in a state nobody can
            // reason about, so the service stops rather than carrying on with it — and `stop` is
            // skipped for the same reason.
            if let Err(panic) = AssertUnwindSafe(service.handle(&ctx, input))
                .catch_unwind()
                .await
            {
                let reason = panic_reason(panic.as_ref());
                tracing::error!(
                    service = S::NAME,
                    reason,
                    "handler panicked, stopping the service"
                );
                self.report_stopped(Some(reason));
                return Ok(());
            }
        }

        service.stop(&ctx).await;
        self.report_stopped(None);
        Ok(())
    }

    fn report_stopped(&self, reason: Option<String>) {
        self.broker
            .report_health(S::NAME, ServiceState::Stopped { reason });
    }
}

fn panic_reason(panic: &(dyn Any + Send)) -> String {
    if let Some(text) = panic.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = panic.downcast_ref::<String>() {
        return text.clone();
    }
    "panicked".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockBroker;

    struct Panicky;

    impl Service for Panicky {
        const NAME: &'static str = "panicky";
        const TOPICS: &'static [&'static str] = &[];

        type Config = ();
        type Command = ();
        type Event = ();

        async fn start(_ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
            Ok(Self)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {
            panic!("the backend said something unrepeatable");
        }

        fn peek_config(_config: &Config) -> Self::Config {}
    }

    /// The panic is expected and its message reaches the log; what matters is that the service is
    /// reported `Stopped` rather than left `Running`, and that `run` returns instead of unwinding
    /// into the daemon.
    #[tokio::test]
    async fn a_panicking_handler_stops_its_own_service() {
        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let mut runtime = ServiceRuntime::<Panicky>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );

        runtime
            .sender()
            .send(Input::Event(()))
            .await
            .expect("queued");
        runtime
            .run(())
            .await
            .expect("run returns rather than unwinding");

        let states: Vec<ServiceState> = mock.health().into_iter().map(|(_, s)| s).collect();
        assert_eq!(states.first(), Some(&ServiceState::Starting));
        assert!(states.contains(&ServiceState::Running));
        assert!(
            matches!(states.last(), Some(ServiceState::Stopped { reason: Some(reason) })
                if reason.contains("unrepeatable")),
            "expected a Stopped carrying the panic message, got {states:?}"
        );
    }

    struct NeedsTheBus;

    impl Service for NeedsTheBus {
        const NAME: &'static str = "needs-the-bus";
        const TOPICS: &'static [&'static str] = &[];

        type Config = ();
        type Command = ();
        type Event = ();

        async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
            if let Err(reason) = ctx.system_bus() {
                ctx.degraded(format!("no system bus: {reason}"));
            }
            Ok(Self)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}

        fn peek_config(_config: &Config) -> Self::Config {}
    }

    /// A missing bus costs the service its backend, not its life: it still starts, still reaches
    /// `Running`, and puts the reason somewhere `glimpsectl services` can read it.
    #[tokio::test]
    async fn a_service_without_a_bus_degrades_and_keeps_running() {
        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<NeedsTheBus>::new(
            broker,
            Buses::unavailable("connect failed"),
            cancel.clone(),
        );

        cancel.cancel();
        runtime.run(()).await.expect("starts without a bus");

        let states: Vec<ServiceState> = mock.health().into_iter().map(|(_, s)| s).collect();
        assert!(
            states.iter().any(|state| matches!(
                state,
                ServiceState::Degraded { reason } if reason.contains("connect failed")
            )),
            "expected a Degraded naming the connect failure, got {states:?}"
        );
        assert!(
            !states.contains(&ServiceState::Running),
            "a service that degraded during start must not then be reported Running, got {states:?}"
        );
    }
}
