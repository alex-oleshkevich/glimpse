mod call;
mod failure;
mod source;

use std::time::Duration;

use tokio::sync::oneshot;

use ipp::client::non_blocking::AsyncIppClient;
use ipp::error::IppError;
use ipp::prelude::Uri;

pub use failure::Failure;
use failure::{Action, classify};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceError},
    subscription::Sub,
};

const DEFAULT_SERVER_URL: &str = "http://localhost:631/";

#[derive(Debug, Clone, PartialEq)]
pub struct Printer {
    pub name: String,
    pub make_model: String,
    pub state: PrinterState,
    pub state_reasons: Vec<String>,
    pub state_message: String,
    pub location: String,
    pub accepting_jobs: bool,
    pub color: bool,
    pub duplex: bool,
    pub media_ready: Vec<String>,
    pub resolution: String,
    pub job_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Idle,
    Processing,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintJob {
    pub id: u32,
    pub name: String,
    pub printer_name: String,
    pub state: JobState,
    pub pages_completed: Option<u32>,
    pub pages_total: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Held,
    Processing,
    Stopped,
    Completed,
    Cancelled,
    Aborted,
}

impl JobState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Processing | Self::Held)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrintingState {
    pub printers: Vec<Printer>,
    pub jobs: Vec<PrintJob>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    poll_active: Duration,
    poll_idle: Duration,
    server_url: Option<String>,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            poll_active: Duration::from_secs(document.printing.poll_active),
            poll_idle: Duration::from_secs(document.printing.poll_idle),
            server_url: document.printing.server_url.clone(),
        }
    }
}

impl Config {
    fn uri(&self) -> Result<Uri, String> {
        self.server_url
            .as_deref()
            .unwrap_or(DEFAULT_SERVER_URL)
            .parse::<Uri>()
            .map_err(|error| format!("invalid printing server url: {error}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cadence {
    Active,
    Idle,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    Poll(Cadence, Duration, u64),
    Notifier(u64),
}

type Reply = oneshot::Sender<Result<(), PrintingError>>;

#[derive(Debug, thiserror::Error)]
pub enum PrintingError {
    #[error("printing refused the command: {0:?}")]
    Failed(Failure),
    #[error(transparent)]
    Service(#[from] CommandError),
}

pub enum Command {
    CancelJob { id: u32, reply: Reply },
    PauseJob { id: u32, reply: Reply },
    ResumeJob { id: u32, reply: Reply },
    Refresh { reply: Reply },
}

pub enum Event {
    Polled(PrintingState),
    Unavailable(String),
}

pub struct Printing {
    state: Publisher<PrintingState>,
    config: Config,
    active: bool,
    generation: u64,
}

#[derive(Clone)]
pub struct PrintingHandle(crate::ServiceEndpoint<Printing>);

impl PrintingHandle {
    pub fn snapshot(&self) -> PrintingState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<PrintingState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn cancel_job(&self, id: u32) -> Result<(), PrintingError> {
        self.call(|reply| Command::CancelJob { id, reply }).await
    }

    pub async fn pause_job(&self, id: u32) -> Result<(), PrintingError> {
        self.call(|reply| Command::PauseJob { id, reply }).await
    }

    pub async fn resume_job(&self, id: u32) -> Result<(), PrintingError> {
        self.call(|reply| Command::ResumeJob { id, reply }).await
    }

    pub async fn refresh(&self) -> Result<(), PrintingError> {
        self.call(|reply| Command::Refresh { reply }).await
    }

    async fn call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), PrintingError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable("printing stopped before completing the command".to_owned())
        })?
    }
}

impl Service for Printing {
    const NAME: &'static str = "printing";
    type Config = Config;
    type State = PrintingState;
    type Handle = PrintingHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        PrintingHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let cadence = if self.active {
            Cadence::Active
        } else {
            Cadence::Idle
        };
        let period = if self.active {
            self.config.poll_active
        } else {
            self.config.poll_idle
        };
        let poll_config = self.config.clone();
        let notifier_config = self.config.clone();
        let floor = self.config.poll_active;
        let generation = self.generation;

        vec![
            Sub::interval(
                Watch::Poll(cadence, period, generation),
                period,
                move |_ctx| {
                    let config = poll_config.clone();
                    async move { source::fetch(&config).await }
                },
            ),
            Sub::stream(Watch::Notifier(generation), move |ctx| {
                source::notifier(ctx, notifier_config, floor)
            }),
        ]
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        _: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            config,
            active: false,
            generation: 0,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.dispatch(ctx, command).await,
            Input::Config(config) => {
                self.config = config;
                self.generation = self.generation.wrapping_add(1);
            }
            Input::Event(Event::Polled(state)) => {
                ctx.running();
                self.active = state.jobs.iter().any(|job| job.state.is_active());
                self.state.set(state);
            }
            Input::Event(Event::Unavailable(reason)) => self.note_unavailable(ctx, reason),
        }
    }
}

impl Printing {
    fn note_unavailable(&mut self, ctx: &Ctx<Self>, reason: String) {
        if !ctx.is_degraded() {
            tracing::debug!(reason = %reason, "printing: CUPS unreachable");
        }
        ctx.degraded(reason);
        self.active = false;
        self.state.set(PrintingState::default());
    }

    async fn dispatch(&mut self, ctx: &Ctx<Self>, command: Command) {
        match command {
            Command::CancelJob { id, reply } => {
                self.job_action(ctx, Action::Cancel, id, reply, call::cancel_job)
            }
            Command::PauseJob { id, reply } => {
                self.job_action(ctx, Action::Pause, id, reply, call::hold_job)
            }
            Command::ResumeJob { id, reply } => {
                self.job_action(ctx, Action::Resume, id, reply, call::release_job)
            }
            Command::Refresh { reply } => self.refresh(ctx, reply),
        }
    }

    fn refresh(&self, ctx: &Ctx<Self>, reply: Reply) {
        let config = self.config.clone();
        ctx.spawn_detached(move |ctx| async move {
            let event = source::fetch(&config).await;
            let sent = ctx.events().send(Input::Event(event)).await.is_ok();
            let _ = reply.send(if sent {
                Ok(())
            } else {
                Err(CommandError::Unavailable(
                    "printing stopped before the refresh completed".to_owned(),
                )
                .into())
            });
        });
    }

    fn job_action<F, Fut>(&self, ctx: &Ctx<Self>, action: Action, id: u32, reply: Reply, call: F)
    where
        F: FnOnce(AsyncIppClient, Uri, u32) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), IppError>> + Send + 'static,
    {
        let uri = match self.config.uri() {
            Ok(uri) => uri,
            Err(reason) => {
                let _ = reply.send(Err(CommandError::Unavailable(reason).into()));
                return;
            }
        };
        let client = AsyncIppClient::new(uri.clone());
        ctx.spawn_detached(move |_ctx| async move {
            let outcome = match call(client, uri, id).await {
                Ok(()) => Ok(()),
                Err(error) => classify(action, &error).map_err(PrintingError::Failed),
            };
            let _ = reply.send(outcome);
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::service::ServiceRuntime;
    use glimpse_dbus::Buses;

    fn config(poll_active_ms: u64, poll_idle_ms: u64, server_url: Option<&str>) -> Config {
        Config {
            poll_active: Duration::from_millis(poll_active_ms),
            poll_idle: Duration::from_millis(poll_idle_ms),
            server_url: server_url.map(str::to_owned),
        }
    }

    async fn printing(
        cfg: Config,
    ) -> (
        Printing,
        Ctx<Printing>,
        tokio::sync::watch::Receiver<PrintingState>,
        tokio::sync::watch::Receiver<crate::ServiceState>,
    ) {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel(8);
        let (state, state_rx) = tokio::sync::watch::channel(PrintingState::default());
        let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Printing>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Printing::start(&ctx, cfg, ()).await.expect("starts");
        (service, ctx, state_rx, health_rx)
    }

    fn active_job() -> PrintJob {
        PrintJob {
            id: 1,
            name: "doc".to_owned(),
            printer_name: "office".to_owned(),
            state: JobState::Processing,
            pages_completed: None,
            pages_total: Some(3),
        }
    }

    fn poll_key(subs: &[Sub<Printing>]) -> (Cadence, Duration) {
        subs.iter()
            .find_map(|sub| match sub.key() {
                Watch::Poll(cadence, period, _) => Some((*cadence, *period)),
                Watch::Notifier(_) => None,
            })
            .expect("a poll subscription is always declared")
    }

    fn generations(subs: &[Sub<Printing>]) -> (u64, u64) {
        let poll = subs
            .iter()
            .find_map(|sub| match sub.key() {
                Watch::Poll(_, _, generation) => Some(*generation),
                Watch::Notifier(_) => None,
            })
            .expect("a poll subscription is always declared");
        let notifier = subs
            .iter()
            .find_map(|sub| match sub.key() {
                Watch::Notifier(generation) => Some(*generation),
                Watch::Poll(..) => None,
            })
            .expect("a notifier subscription is always declared");
        (poll, notifier)
    }

    #[tokio::test]
    async fn the_poll_cadence_differs_between_idle_and_active_and_carries_the_right_period() {
        let (mut service, ctx, _state, _health) = printing(config(20, 300, None)).await;

        let idle = poll_key(&service.subscriptions());
        assert_eq!(idle, (Cadence::Idle, Duration::from_millis(300)));

        service
            .handle(
                &ctx,
                Input::Event(Event::Polled(PrintingState {
                    printers: Vec::new(),
                    jobs: vec![active_job()],
                })),
            )
            .await;

        let active = poll_key(&service.subscriptions());
        assert_eq!(active, (Cadence::Active, Duration::from_millis(20)));
        assert_ne!(
            idle, active,
            "the runtime only rebuilds a source when its key changes"
        );
    }

    #[tokio::test]
    async fn a_completed_only_job_list_keeps_the_idle_cadence() {
        let (mut service, ctx, _state, _health) = printing(config(20, 300, None)).await;
        let completed = PrintJob {
            id: 2,
            name: "doc".to_owned(),
            printer_name: "office".to_owned(),
            state: JobState::Completed,
            pages_completed: Some(3),
            pages_total: Some(3),
        };

        service
            .handle(
                &ctx,
                Input::Event(Event::Polled(PrintingState {
                    printers: Vec::new(),
                    jobs: vec![completed],
                })),
            )
            .await;

        assert_eq!(
            poll_key(&service.subscriptions()),
            (Cadence::Idle, Duration::from_millis(300))
        );
    }

    #[tokio::test]
    async fn a_config_reload_rebuilds_both_sources_even_when_the_cadence_does_not_change() {
        let (mut service, ctx, _state, _health) = printing(config(20, 300, None)).await;
        let before = generations(&service.subscriptions());

        service
            .handle(
                &ctx,
                Input::Config(config(20, 300, Some("http://printserver:631/"))),
            )
            .await;

        let after = generations(&service.subscriptions());
        assert_ne!(
            before, after,
            "changing server_url must rebuild the poll and the notifier, or both keep talking \
             to the old server forever"
        );
    }

    #[tokio::test]
    async fn declared_subscriptions_really_start_a_running_source() {
        let cancel = CancellationToken::new();
        let (events, mut inbox) = tokio::sync::mpsc::channel(8);
        let (state, _state_rx) = tokio::sync::watch::channel(PrintingState::default());
        let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Printing>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Printing::start(&ctx, config(15, 5_000, Some("http://127.0.0.1:1/")), ())
            .await
            .expect("starts");

        let mut live = crate::subscription::Live::new();
        live.reconcile(&ctx, service.subscriptions());

        let received = tokio::time::timeout(Duration::from_secs(5), inbox.recv())
            .await
            .expect("the interval's own first tick must run without waiting a whole period");
        assert!(
            matches!(received, Some(Input::Event(Event::Unavailable(_)))),
            "a real timer must have ticked and reached the inbox, not merely a correct key in isolation"
        );
    }

    #[derive(Default)]
    struct Counts {
        debug: AtomicUsize,
        warn: AtomicUsize,
    }

    struct Counter(Arc<Counts>);

    impl tracing::Subscriber for Counter {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            match *event.metadata().level() {
                tracing::Level::DEBUG => {
                    self.0.debug.fetch_add(1, Ordering::SeqCst);
                }
                tracing::Level::WARN => {
                    self.0.warn.fetch_add(1, Ordering::SeqCst);
                }
                _ => {}
            }
        }

        fn enter(&self, _span: &tracing::span::Id) {}

        fn exit(&self, _span: &tracing::span::Id) {}
    }

    #[tokio::test]
    async fn cups_unreachable_marks_health_degraded_and_logs_debug_once_per_transition_never_warn()
    {
        let (mut service, ctx, _state, health) = printing(config(20, 300, None)).await;
        let counts = Arc::new(Counts::default());
        let subscriber = Counter(counts.clone());

        tracing::subscriber::with_default(subscriber, || {
            futures_util::FutureExt::now_or_never(service.handle(
                &ctx,
                Input::Event(Event::Unavailable("connection refused".to_owned())),
            ))
            .expect("Event::Unavailable has no real await point");

            assert!(
                matches!(
                    &*health.borrow(),
                    crate::ServiceState::Degraded { reason } if reason == "connection refused"
                ),
                "the first failure must reach the health watch, not just the log"
            );

            ctx.running();

            futures_util::FutureExt::now_or_never(service.handle(
                &ctx,
                Input::Event(Event::Unavailable("still down".to_owned())),
            ))
            .expect("Event::Unavailable has no real await point");
        });

        assert!(
            matches!(&*health.borrow(), crate::ServiceState::Degraded { reason } if reason == "still down"),
            "the second, later failure must be the reason the health watch now shows"
        );
        assert_eq!(
            counts.debug.load(Ordering::SeqCst),
            2,
            "a recovered service degrading again is a second transition and must log again"
        );
        assert_eq!(
            counts.warn.load(Ordering::SeqCst),
            0,
            "a degraded backend is reported with debug!, never warn!"
        );
    }

    #[test]
    fn a_missing_server_url_falls_back_to_the_cups_default() {
        let config = config(20, 300, None);
        assert_eq!(config.uri().expect("a uri").to_string(), DEFAULT_SERVER_URL);
    }

    #[test]
    fn an_explicit_server_url_is_used_verbatim() {
        let config = config(20, 300, Some("http://printserver:631/"));
        assert_eq!(
            config.uri().expect("a uri").to_string(),
            "http://printserver:631/"
        );
    }

    #[test]
    fn a_malformed_server_url_is_refused_rather_than_guessed() {
        let config = config(20, 300, Some("not a uri"));
        assert!(config.uri().is_err());
    }

    #[tokio::test]
    async fn snapshot_before_the_first_poll_is_the_default_empty_state_and_does_not_block() {
        let (_runtime, handle) = ServiceRuntime::<Printing>::new(
            config(2_000, 30_000, None),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );

        assert_eq!(handle.snapshot(), PrintingState::default());
    }

    #[tokio::test]
    async fn a_job_action_against_an_unreachable_cups_resolves_a_typed_failure() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
            config(20, 300, Some("http://127.0.0.1:1/")),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let running = tokio::spawn(async move { runtime.run(()).await });

        let outcome = handle.cancel_job(1).await;

        cancel.cancel();
        let _ = running.await;

        assert!(
            matches!(outcome, Err(PrintingError::Failed(Failure::Unreachable))),
            "a connection refused must classify as a typed Failure, never leak the raw ipp error"
        );
    }

    #[tokio::test]
    async fn refresh_resolves_ok_even_when_the_underlying_poll_fails() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
            config(20, 300, Some("http://127.0.0.1:1/")),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let running = tokio::spawn(async move { runtime.run(()).await });

        let outcome = handle.refresh().await;

        cancel.cancel();
        let _ = running.await;

        assert!(
            outcome.is_ok(),
            "refresh only promises the poll was queued, not that CUPS answered it well"
        );
    }

    #[tokio::test]
    async fn a_command_against_a_stopped_service_is_refused_rather_than_left_waiting() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Printing>::new(
            config(20, 300, None),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let running = tokio::spawn(async move { runtime.run(()).await });
        cancel.cancel();
        let _ = running.await;

        let outcome = handle.cancel_job(1).await;

        assert!(matches!(
            outcome,
            Err(PrintingError::Service(CommandError::Unavailable(_)))
        ));
    }
}
