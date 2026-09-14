use std::{any::Any, hash::Hash, panic::AssertUnwindSafe};

use futures_util::FutureExt;
use glimpse_dbus::Buses;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::Ctx;
use crate::subscription::{Live, Sub};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ServiceState {
    Starting,
    Running,
    Degraded { reason: String },
    Stopped { reason: Option<String> },
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("service failed to start")]
    StartError,
    #[error("did not send message: {0}")]
    SendError(String),
    #[error("dbus: {0}")]
    Bus(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub enum Input<S: Service> {
    Event(S::Event),
    Command(S::Command),
    Config(S::Config),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoConfig;

impl From<&glimpse_config::Config> for NoConfig {
    fn from(_document: &glimpse_config::Config) -> Self {
        Self
    }
}

pub trait Service: Sized + Send + 'static {
    const NAME: &'static str;

    type Config: Clone + PartialEq + Send + 'static + for<'a> From<&'a glimpse_config::Config>;
    type State: Clone + PartialEq + Send + Sync + 'static;
    type Handle: Clone + Send + 'static;
    type Command: Send + 'static;
    type Event: Send + 'static;
    type Dependencies: Send + 'static;
    type SubKey: Eq + Hash + Send + 'static;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle;

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        Vec::new()
    }

    fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> impl Future<Output = Result<Self, ServiceError>> + Send;
    fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) -> impl Future<Output = ()> + Send;
    fn stop(self, ctx: &Ctx<Self>) -> impl Future<Output = ()> + Send {
        let _ = ctx;
        async {}
    }
}

const INBOX_SIZE: usize = 128;

pub struct ServiceEndpoint<S: Service> {
    state: watch::Receiver<S::State>,
    health: watch::Receiver<ServiceState>,
    input: mpsc::Sender<Input<S>>,
}

impl<S: Service> Clone for ServiceEndpoint<S> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            health: self.health.clone(),
            input: self.input.clone(),
        }
    }
}

impl<S: Service> ServiceEndpoint<S> {
    pub(crate) fn snapshot(&self) -> S::State {
        self.state.borrow().clone()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<S::State> {
        self.state.clone()
    }

    pub(crate) fn health(&self) -> watch::Receiver<ServiceState> {
        self.health.clone()
    }

    pub(crate) fn command(&self, command: S::Command) -> Result<(), CommandError> {
        self.input.try_send(Input::Command(command)).map_err(|_| {
            CommandError::Unavailable(format!("`{}` is not accepting commands", S::NAME))
        })
    }
}

pub struct ServiceSender<S: Service> {
    inbox_tx: mpsc::Sender<Input<S>>,
}

impl<S: Service> Clone for ServiceSender<S> {
    fn clone(&self) -> Self {
        Self {
            inbox_tx: self.inbox_tx.clone(),
        }
    }
}

impl<S: Service> ServiceSender<S> {
    pub async fn send(&self, input: Input<S>) -> Result<(), ServiceError> {
        self.inbox_tx
            .send(input)
            .await
            .map_err(|error| ServiceError::SendError(error.to_string()))
    }

    pub fn reconfigure(&self, config: S::Config) {
        match self.inbox_tx.try_send(Input::Config(config)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    service = S::NAME,
                    "inbox full, dropped a configuration update"
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::debug!(service = S::NAME, "stopped, dropped a configuration update");
            }
        }
    }
}

pub struct ServiceRuntime<S: Service> {
    inbox_sender: mpsc::Sender<Input<S>>,
    inbox: mpsc::Receiver<Input<S>>,
    state: watch::Sender<S::State>,
    health: watch::Sender<ServiceState>,
    buses: Buses,
    cancel: CancellationToken,
}

impl<S: Service> ServiceRuntime<S> {
    pub fn new(initial: S::State, buses: Buses, cancel: CancellationToken) -> (Self, S::Handle) {
        let (inbox_sender, inbox) = mpsc::channel(INBOX_SIZE);
        let (state, state_rx) = watch::channel(initial);
        let (health, health_rx) = watch::channel(ServiceState::Starting);
        let handle = S::from_endpoint(ServiceEndpoint {
            state: state_rx,
            health: health_rx,
            input: inbox_sender.clone(),
        });
        (
            Self {
                inbox_sender,
                inbox,
                state,
                health,
                buses,
                cancel,
            },
            handle,
        )
    }

    pub fn sender(&self) -> ServiceSender<S> {
        ServiceSender {
            inbox_tx: self.inbox_sender.clone(),
        }
    }

    pub async fn run(
        &mut self,
        config: S::Config,
        dependencies: S::Dependencies,
    ) -> Result<(), ServiceError> {
        let ctx = Ctx::<S>::new(
            self.inbox_sender.clone(),
            &self.cancel,
            self.state.clone(),
            self.health.clone(),
            self.buses.clone(),
        );

        set_health(&self.health, ServiceState::Starting);
        let mut applied = config.clone();
        let mut service = match S::start(&ctx, config, dependencies).await {
            Ok(service) => service,
            Err(error) => {
                self.close_inbox();
                ctx.shutdown().await;
                self.report_stopped(Some(error.to_string()));
                return Err(error);
            }
        };
        if !ctx.is_degraded() {
            set_health(&self.health, ServiceState::Running);
        }

        let mut live = Live::<S>::new();
        live.reconcile(&ctx, service.subscriptions());

        loop {
            let input = tokio::select! {
                () = self.cancel.cancelled() => break,
                input = self.inbox.recv() => match input {
                    Some(input) => input,
                    None => break,
                },
            };

            if let Input::Config(next) = &input {
                if *next == applied {
                    continue;
                }
                applied = next.clone();
            }

            let handled = AssertUnwindSafe(async {
                service.handle(&ctx, input).await;
                service.subscriptions()
            })
            .catch_unwind()
            .await;

            match handled {
                Ok(declared) => live.reconcile(&ctx, declared),
                Err(panic) => {
                    let reason = panic_reason(panic.as_ref());
                    tracing::error!(
                        service = S::NAME,
                        reason,
                        "handler panicked, stopping the service"
                    );
                    self.close_inbox();
                    ctx.shutdown().await;
                    self.report_stopped(Some(reason));
                    return Ok(());
                }
            }
        }

        self.close_inbox();
        ctx.shutdown().await;
        service.stop(&ctx).await;
        self.report_stopped(None);
        Ok(())
    }

    fn report_stopped(&self, reason: Option<String>) {
        set_health(&self.health, ServiceState::Stopped { reason });
    }

    fn close_inbox(&mut self) {
        self.inbox.close();
        while self.inbox.try_recv().is_ok() {}
    }
}

pub(crate) fn set_health(health: &watch::Sender<ServiceState>, next: ServiceState) {
    health.send_if_modified(|current| {
        if *current == next {
            false
        } else {
            *current = next;
            true
        }
    });
}

pub(crate) fn panic_reason(panic: &(dyn Any + Send)) -> String {
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
    use futures_util::{StreamExt, stream};

    use super::*;

    struct Panicky;

    impl Service for Panicky {
        const NAME: &'static str = "panicky";

        type Config = NoConfig;
        type State = ();
        type Handle = ServiceEndpoint<Self>;
        type Command = tokio::sync::oneshot::Sender<()>;
        type Event = ();
        type Dependencies = ();
        type SubKey = ();

        fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
            endpoint
        }

        async fn start(
            _ctx: &Ctx<Self>,
            _config: Self::Config,
            _dependencies: Self::Dependencies,
        ) -> Result<Self, ServiceError> {
            Ok(Self)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {
            panic!("the backend said something unrepeatable");
        }
    }

    #[tokio::test]
    async fn a_panicking_handler_stops_its_service() {
        let (mut runtime, handle) = ServiceRuntime::<Panicky>::new(
            (),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );
        runtime
            .sender()
            .send(Input::Event(()))
            .await
            .expect("queued");
        runtime
            .run(NoConfig, ())
            .await
            .expect("run returns rather than unwinding");

        assert!(matches!(
            &*handle.health().borrow(),
            ServiceState::Stopped { reason: Some(reason) } if reason.contains("unrepeatable")
        ));
    }

    #[tokio::test]
    async fn a_command_queued_behind_a_panicking_handler_is_settled_rather_than_left_waiting() {
        let (mut runtime, handle) = ServiceRuntime::<Panicky>::new(
            (),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );
        runtime
            .sender()
            .send(Input::Event(()))
            .await
            .expect("queued");
        let (reply, answer) = tokio::sync::oneshot::channel();
        handle.command(reply).expect("queued");

        runtime
            .run(NoConfig, ())
            .await
            .expect("run returns rather than unwinding");

        let settled = tokio::time::timeout(std::time::Duration::from_secs(1), answer)
            .await
            .expect("a caller is not left waiting on a service that already stopped");
        assert!(settled.is_err(), "the responder is dropped, not answered");
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Tuning(u8);

    impl From<&glimpse_config::Config> for Tuning {
        fn from(_document: &glimpse_config::Config) -> Self {
            Self(0)
        }
    }

    struct Tunable {
        seen: u32,
    }

    impl Service for Tunable {
        const NAME: &'static str = "tunable";

        type Config = Tuning;
        type State = (u8, u32);
        type Handle = ServiceEndpoint<Self>;
        type Command = ();
        type Event = ();
        type Dependencies = ();
        type SubKey = ();

        fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
            endpoint
        }

        async fn start(
            _ctx: &Ctx<Self>,
            _config: Self::Config,
            _dependencies: Self::Dependencies,
        ) -> Result<Self, ServiceError> {
            Ok(Self { seen: 0 })
        }

        async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
            if let Input::Config(Tuning(value)) = input {
                self.seen += 1;
                ctx.publisher().set((value, self.seen));
            }
        }
    }

    #[tokio::test]
    async fn an_unchanged_configuration_never_reaches_the_handler() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Tunable>::new(
            (0, 0),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let sender = runtime.sender();
        let mut state = handle.subscribe();
        let running = tokio::spawn(async move { runtime.run(Tuning(1), ()).await });

        sender.reconfigure(Tuning(1));
        sender.reconfigure(Tuning(2));
        state
            .wait_for(|state| *state == (2, 1))
            .await
            .expect("a reload matching the config it started on is not a change");

        sender.reconfigure(Tuning(2));
        sender.reconfigure(Tuning(1));
        state
            .wait_for(|state| *state == (1, 2))
            .await
            .expect("going back is a change, repeating is not");

        cancel.cancel();
        running.await.expect("joined").expect("stopped");
    }

    struct RefusesToStart;

    impl Service for RefusesToStart {
        const NAME: &'static str = "refuses-to-start";

        type Config = NoConfig;
        type State = ();
        type Handle = ServiceEndpoint<Self>;
        type Command = tokio::sync::oneshot::Sender<()>;
        type Event = ();
        type Dependencies = ();
        type SubKey = ();

        fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
            endpoint
        }

        async fn start(
            _ctx: &Ctx<Self>,
            _config: Self::Config,
            _dependencies: Self::Dependencies,
        ) -> Result<Self, ServiceError> {
            Err(ServiceError::StartError)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
            if let Input::Command(reply) = input {
                let _ = reply.send(());
            }
        }
    }

    #[tokio::test]
    async fn a_failed_start_settles_queued_command_replies() {
        let (mut runtime, handle) = ServiceRuntime::<RefusesToStart>::new(
            (),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );
        let (reply, answer) = tokio::sync::oneshot::channel();
        handle.command(reply).expect("queued");

        assert!(runtime.run(NoConfig, ()).await.is_err());
        assert!(answer.await.is_err());
    }

    struct Armable {
        armed: bool,
    }

    impl Service for Armable {
        const NAME: &'static str = "armable";

        type Config = NoConfig;
        type State = u8;
        type Handle = ServiceEndpoint<Self>;
        type Command = ();
        type Event = u8;
        type Dependencies = ();
        type SubKey = ();

        fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
            endpoint
        }

        fn subscriptions(&self) -> Vec<Sub<Self>> {
            self.armed
                .then(|| {
                    Sub::stream((), |_ctx| async {
                        stream::once(async { 7 }).chain(stream::pending())
                    })
                })
                .into_iter()
                .collect()
        }

        async fn start(
            _ctx: &Ctx<Self>,
            _config: Self::Config,
            _dependencies: Self::Dependencies,
        ) -> Result<Self, ServiceError> {
            Ok(Self { armed: false })
        }

        async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
            match input {
                Input::Command(()) => self.armed = true,
                Input::Event(value) => {
                    ctx.publisher().set(value);
                }
                Input::Config(_) => {}
            }
        }
    }

    #[tokio::test]
    async fn a_source_declared_by_a_handler_is_started_by_the_runtime() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Armable>::new(
            0,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        runtime
            .sender()
            .send(Input::Command(()))
            .await
            .expect("queued");

        let running = tokio::spawn(async move { runtime.run(NoConfig, ()).await });
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert_eq!(handle.snapshot(), 7);
        cancel.cancel();
        let _ = running.await;
        assert!(
            handle.command(()).is_err(),
            "a stopped service rejects commands"
        );
    }

    struct NeedsTheBus;

    impl Service for NeedsTheBus {
        const NAME: &'static str = "needs-the-bus";

        type Config = NoConfig;
        type State = ();
        type Handle = ServiceEndpoint<Self>;
        type Command = ();
        type Event = ();
        type Dependencies = ();
        type SubKey = ();

        fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
            endpoint
        }

        async fn start(
            ctx: &Ctx<Self>,
            _config: Self::Config,
            _dependencies: Self::Dependencies,
        ) -> Result<Self, ServiceError> {
            if let Err(reason) = ctx.system_bus() {
                ctx.degraded(format!("no system bus: {reason}"));
            }
            Ok(Self)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}
    }

    #[tokio::test]
    async fn a_service_without_a_bus_degrades_and_keeps_running() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<NeedsTheBus>::new(
            (),
            Buses::unavailable("connect failed"),
            cancel.clone(),
        );
        let mut health = handle.health();
        let running = tokio::spawn(async move { runtime.run(NoConfig, ()).await });

        health.changed().await.expect("health changes");
        assert!(matches!(
            &*health.borrow(),
            ServiceState::Degraded { reason } if reason.contains("connect failed")
        ));

        cancel.cancel();
        running
            .await
            .expect("runtime joins")
            .expect("runtime stops");
    }
}
