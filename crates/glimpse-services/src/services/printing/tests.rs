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
    let service = Printing::start(&ctx, cfg, ()).await;
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
        reason: String::new(),
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
        reason: String::new(),
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
    let service = Printing::start(&ctx, config(15, 5_000, Some("http://127.0.0.1:1/")), ()).await;

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
async fn cups_unreachable_marks_health_degraded_and_logs_debug_once_per_transition_never_warn() {
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
