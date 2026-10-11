use std::{any::Any, hash::Hash, panic::AssertUnwindSafe};

use futures_util::FutureExt;
use glimpse_dbus::Buses;
use tokio::sync::{mpsc, oneshot, watch};
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

impl ServiceState {
    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Running => None,
            Self::Starting => Some("starting"),
            Self::Degraded { reason } => Some(reason),
            Self::Stopped { reason } => reason.as_deref().or(Some("stopped")),
        }
    }
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
    type Command: Send + 'static;
    type Event: Send + 'static;
    type Dependencies: Send + 'static;
    type SubKey: Eq + Hash + Send + 'static;

    fn command_stopped_reason(_command: &Self::Command) -> &'static str {
        "service stopped before answering"
    }

    fn initial_state(config: &Self::Config) -> Self::State;

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        Vec::new()
    }

    fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> impl Future<Output = Self> + Send;
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
    pub fn snapshot(&self) -> S::State {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<S::State> {
        self.state.clone()
    }

    pub fn health(&self) -> watch::Receiver<ServiceState> {
        self.health.clone()
    }

    pub(crate) async fn call<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<T>) -> S::Command,
    ) -> Result<T, CommandError> {
        let (reply, answer) = oneshot::channel();
        let command = make(reply);
        let reason = S::command_stopped_reason(&command);
        self.command(command)?;
        answer
            .await
            .map_err(|_| CommandError::Unavailable(reason.to_owned()))
    }

    pub(crate) fn command(&self, command: S::Command) -> Result<(), CommandError> {
        self.input.try_send(Input::Command(command)).map_err(|_| {
            CommandError::Unavailable(format!("`{}` is not accepting commands", S::NAME))
        })
    }
}

pub struct Running<S: Service> {
    sender: mpsc::Sender<Input<S>>,
    cancel: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}

/// A service whose channels exist but whose task has not been spawned. `glimpse-sunset` builds
/// every service, takes its D-Bus name and gamma control, and only then starts them, so a second
/// copy fails on the name before it touches the outputs the running one holds.
pub struct Pending<S: Service> {
    runtime: ServiceRuntime<S>,
    sender: mpsc::Sender<Input<S>>,
    cancel: CancellationToken,
}

impl<S: Service> Pending<S> {
    pub fn start(self, dependencies: S::Dependencies) -> Running<S> {
        let Self {
            mut runtime,
            sender,
            cancel,
        } = self;
        let task = tokio::spawn(async move {
            tracing::debug!(service = S::NAME, "service task starting");
            runtime.run(dependencies).await;
            tracing::debug!(service = S::NAME, "service task stopped");
        });
        Running {
            sender,
            cancel,
            task: Some(task),
        }
    }
}

impl<S: Service> Running<S> {
    pub fn build(
        document: &glimpse_config::Config,
        buses: Buses,
    ) -> (Pending<S>, ServiceEndpoint<S>) {
        let cancel = CancellationToken::new();
        let (runtime, handle) =
            ServiceRuntime::<S>::new(S::Config::from(document), buses, cancel.clone());
        let sender = runtime.sender();
        (
            Pending {
                runtime,
                sender,
                cancel,
            },
            handle,
        )
    }

    pub fn spawn(
        document: &glimpse_config::Config,
        buses: Buses,
        dependencies: S::Dependencies,
    ) -> (Self, ServiceEndpoint<S>) {
        let (pending, handle) = Self::build(document, buses);
        (pending.start(dependencies), handle)
    }

    pub fn reconfigure(&self, document: &glimpse_config::Config) {
        match self
            .sender
            .try_send(Input::Config(S::Config::from(document)))
        {
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

    /// Tell the service to stop without waiting for it, so a graph can cancel every service
    /// before it joins any of them.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub async fn stop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take()
            && let Err(error) = task.await
        {
            tracing::error!(service = S::NAME, %error, "service task failed");
        }
    }
}

impl<S: Service> Drop for Running<S> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub struct ServiceRuntime<S: Service> {
    inbox_sender: mpsc::Sender<Input<S>>,
    inbox: mpsc::Receiver<Input<S>>,
    state: watch::Sender<S::State>,
    health: watch::Sender<ServiceState>,
    buses: Buses,
    cancel: CancellationToken,
    /// What the service starts on. It is never updated — a reload reaches the handler as
    /// `Input::Config`, and `run` tracks the config in force in its own local.
    initial_config: S::Config,
}

impl<S: Service> ServiceRuntime<S> {
    pub fn new(
        config: S::Config,
        buses: Buses,
        cancel: CancellationToken,
    ) -> (Self, ServiceEndpoint<S>) {
        let (inbox_sender, inbox) = mpsc::channel(INBOX_SIZE);
        let (state, state_rx) = watch::channel(S::initial_state(&config));
        let (health, health_rx) = watch::channel(ServiceState::Starting);
        let handle = ServiceEndpoint {
            state: state_rx,
            health: health_rx,
            input: inbox_sender.clone(),
        };
        (
            Self {
                inbox_sender,
                inbox,
                state,
                health,
                buses,
                cancel,
                initial_config: config,
            },
            handle,
        )
    }

    pub fn sender(&self) -> mpsc::Sender<Input<S>> {
        self.inbox_sender.clone()
    }

    pub async fn run(&mut self, dependencies: S::Dependencies) {
        let config = self.initial_config.clone();
        let ctx = Ctx::<S>::new(
            self.inbox_sender.clone(),
            &self.cancel,
            self.state.clone(),
            self.health.clone(),
            self.buses.clone(),
        );

        set_health(&self.health, ServiceState::Starting);
        let mut applied = self.initial_config.clone();
        let mut service = S::start(&ctx, config, dependencies).await;
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
                    return;
                }
            }
        }

        self.close_inbox();
        ctx.shutdown().await;
        service.stop(&ctx).await;
        self.report_stopped(None);
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
mod tests;
