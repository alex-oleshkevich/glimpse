use std::panic::AssertUnwindSafe;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::{FutureExt, Stream, StreamExt, stream};
use glimpse_contracts::Message;
use glimpse_dbus::Buses;
use serde::Deserialize;
use serde_json::Value;
use tokio::{
    sync::{mpsc, watch},
    task::AbortHandle,
    time,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::publisher::Publisher;
use crate::service::{Input, Service, panic_reason};
use crate::{BrokerHandle, ServiceState, SubscriptionId};

pub struct Ctx<S: Service> {
    events: mpsc::Sender<Input<S>>,
    tasks: TaskTracker,
    cancel: CancellationToken,
    broker: Arc<dyn BrokerHandle>,
    buses: Buses,
    degraded: Arc<AtomicBool>,
}

/// Every field is owned and cheap to clone, which is what lets a spawned task be handed a `Ctx` of
/// its own instead of the sender and token it would otherwise have to be passed piecemeal.
impl<S: Service> Clone for Ctx<S> {
    fn clone(&self) -> Self {
        Self {
            events: self.events.clone(),
            tasks: self.tasks.clone(),
            cancel: self.cancel.clone(),
            broker: self.broker.clone(),
            buses: self.buses.clone(),
            degraded: self.degraded.clone(),
        }
    }
}

impl<S: Service> Ctx<S> {
    pub fn new(
        events: mpsc::Sender<Input<S>>,
        cancel: &CancellationToken,
        broker: Arc<dyn BrokerHandle>,
        buses: Buses,
    ) -> Self {
        Self {
            events,
            broker,
            buses,
            degraded: Arc::new(AtomicBool::new(false)),
            tasks: TaskTracker::new(),
            cancel: cancel.clone(),
        }
    }

    /// The connection, or why there is none. A service that needs a bus and gets `Err` reports
    /// `degraded` with the reason and keeps running — a missing bus is never a reason to stop.
    pub fn session_bus(&self) -> Result<&zbus::Connection, &str> {
        self.buses.session_bus()
    }

    /// See [`Ctx::session_bus`].
    pub fn system_bus(&self) -> Result<&zbus::Connection, &str> {
        self.buses.system_bus()
    }

    pub fn publisher<T: Message>(&self) -> Publisher<T::Payload> {
        Publisher::new(T::NAME, self.broker.clone())
    }

    pub fn cancel(&self) -> CancellationToken {
        self.cancel.child_token()
    }

    /// The broker calls the sink from its own task and must never be made to wait, so the sink only
    /// parks the newest payload in a `watch` cell: the producer never blocks, and a payload that
    /// arrives before the previous one was read replaces it rather than queueing. A pump task then
    /// applies `map` and delivers it, which keeps two things off the broker: the wait for a full
    /// inbox, and the service's own closure — a panic in `map` degrades this service instead of
    /// taking the broker down with it.
    pub fn subscribe<T: Message>(
        &self,
        map: impl Fn(T::Payload) -> S::Event + Send + 'static,
    ) -> SourceGuard {
        let (latest, mut changed) = watch::channel(None::<T::Payload>);

        let id = self.broker.subscribe(
            T::NAME,
            Box::new(move |data: &Value| match T::Payload::deserialize(data) {
                Ok(value) => {
                    latest.send_replace(Some(value));
                }
                Err(err) => tracing::warn!(topic=T::NAME, %err, "undecodable payload"),
            }),
        );

        let events = self.events.clone();
        let mut guard = self.spawn_raw(async move {
            while changed.changed().await.is_ok() {
                // Cloned out in its own statement: the borrow guard must not be held across the
                // send below.
                let payload = changed.borrow_and_update().clone();
                let Some(payload) = payload else { continue };
                if events.send(Input::Event(map(payload))).await.is_err() {
                    break;
                }
            }
        });
        guard.subscription = Some((self.broker.clone(), id));
        guard
    }

    /// One unit of asynchronous work whose result is one event. The task is handed a `Ctx` of its
    /// own, so it reaches the buses, the publishers and `degraded` without any of them being
    /// threaded through its arguments.
    pub fn spawn<F, Fut>(&self, task: F) -> SourceGuard
    where
        F: FnOnce(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = S::Event> + Send + 'static,
    {
        self.stream(|ctx| async move { stream::once(task(ctx)) })
    }

    /// Work with nothing to report back: a handler that moved its `Responder` into a task so a slow
    /// backend cannot freeze the service, and has nothing to tell itself once it finishes. Work
    /// that does produce a value belongs in [`Ctx::spawn`], which delivers it.
    pub fn spawn_detached<F, Fut>(&self, task: F) -> SourceGuard
    where
        F: FnOnce(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let ctx = self.clone();
        self.spawn_raw(async move { task(ctx).await })
    }

    /// An event per tick, starting now. A tick that is still running when the next is due does not
    /// stack them up: the missed one is skipped, so a slow handler falls behind rather than
    /// building a backlog it can never clear.
    pub fn interval<F, Fut>(&self, period: time::Duration, on_tick: F) -> SourceGuard
    where
        F: Fn(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = S::Event> + Send + 'static,
    {
        self.at_interval(time::Instant::now(), period, on_tick)
    }

    /// See [`Ctx::interval`]; this one starts at a chosen instant instead of immediately.
    pub fn at_interval<F, Fut>(
        &self,
        start: time::Instant,
        period: time::Duration,
        on_tick: F,
    ) -> SourceGuard
    where
        F: Fn(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = S::Event> + Send + 'static,
    {
        self.stream(move |ctx| async move {
            let mut timer = time::interval_at(start, period);
            timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            // `on_tick` rides in the unfold state rather than an `Arc`, which would need a `Sync`
            // bound on every caller's closure to buy nothing but a shorter line here.
            stream::unfold(
                (timer, on_tick, ctx),
                |(mut timer, on_tick, ctx)| async move {
                    timer.tick().await;
                    let event = on_tick(ctx.clone()).await;
                    Some((event, (timer, on_tick, ctx)))
                },
            )
        })
    }

    /// A backend that produces events for as long as the service lives. The closure is async
    /// because building such a source usually is — a D-Bus signal stream has to be requested
    /// before it can be read — and everything it yields reaches the handler as an event.
    ///
    /// Every event-producing source ends up here, which is what keeps one answer to a closed inbox
    /// rather than one per constructor.
    pub fn stream<F, Fut, St>(&self, source: F) -> SourceGuard
    where
        F: FnOnce(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = St> + Send + 'static,
        St: Stream<Item = S::Event> + Send + 'static,
    {
        let ctx = self.clone();
        let events = self.events.clone();

        self.spawn_raw(async move {
            let stream = source(ctx).await;
            tokio::pin!(stream);

            while let Some(event) = stream.next().await {
                if events.send(Input::Event(event)).await.is_err() {
                    break;
                }
            }
        })
    }

    /// The one place a task is registered and made cancellable. Private because a service task that
    /// produces no event has no way to reach its handler, and is therefore not a source.
    fn spawn_raw(&self, task: impl Future<Output = ()> + Send + 'static) -> SourceGuard {
        let cancel = self.cancel.clone();
        let ctx = self.clone();

        let handle = self
            .tasks
            .spawn(async move {
                // A source is where the backend's own data gets parsed, which makes it both the
                // likeliest place to panic and the least visible: the task would simply stop, and
                // the service would go on believing it still has a source. Catching it is what
                // turns silence into a state somebody can read.
                let outcome = AssertUnwindSafe(async {
                    tokio::select! {
                        () = cancel.cancelled() => {},
                        () = task => {}
                    }
                })
                .catch_unwind()
                .await;

                if let Err(panic) = outcome {
                    let reason = panic_reason(panic.as_ref());
                    tracing::error!(service = S::NAME, reason, "a source task panicked");
                    ctx.degraded(format!("a source task panicked: {reason}"));
                }
            })
            .abort_handle();

        SourceGuard {
            abort: Some(handle),
            subscription: None,
        }
    }

    /// The service's inbox sender, for a producer the sources cannot wrap: a synchronous callback
    /// from a foreign thread that has to hand an event over without a task of its own. Nothing
    /// needs it today — the one caller it was added for, the config watcher, has been removed.
    pub fn events(&self) -> mpsc::Sender<Input<S>> {
        self.events.clone()
    }

    /// Stops every source and waits for it, so nothing is still publishing on the service's behalf
    /// once it has reported itself stopped.
    pub(crate) async fn shutdown(&self) {
        self.cancel.cancel();
        self.tasks.close();
        self.tasks.wait().await;
    }

    /// A service's own judgement that it is running but cannot fully do its job — a missing Wayland
    /// protocol, a backend that will not answer. Its topics stay current and are never `stale`.
    pub fn degraded(&self, reason: impl Into<String>) {
        self.degraded.store(true, Ordering::Relaxed);
        self.broker.report_health(
            S::NAME,
            ServiceState::Degraded {
                reason: reason.into(),
            },
        );
    }

    /// Withdraw a previous `degraded`, once whatever was missing turns up.
    pub fn running(&self) {
        self.degraded.store(false, Ordering::Relaxed);
        self.broker.report_health(S::NAME, ServiceState::Running);
    }

    pub(crate) fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }
}

#[must_use = "a source stops the moment its guard is dropped; bind it for as long as the source \
              should live, and `let _ = ...` starts a source that is aborted before it runs"]
pub struct SourceGuard {
    abort: Option<AbortHandle>,
    subscription: Option<(Arc<dyn BrokerHandle>, SubscriptionId)>,
}

impl Drop for SourceGuard {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
        if let Some((broker, id)) = self.subscription.take() {
            broker.unsubscribe(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::stream;

    use super::*;
    use crate::MockBroker;
    use crate::service::{Input, ServiceError};

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Ping {
        value: u8,
    }
    glimpse_contracts::topic!(Ping, "test.ping");

    struct Probe;

    impl Service for Probe {
        const NAME: &'static str = "probe";
        const TOPICS: &'static [&'static str] = &[];

        type Config = ();
        type Command = ();
        type Event = u8;
        type SubKey = ();

        async fn start(_ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
            Ok(Self)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}

        fn peek_config(_config: &glimpse_config::Config) -> Self::Config {}
    }

    type Inbox = mpsc::Receiver<Input<Probe>>;

    fn probe() -> (Ctx<Probe>, Inbox) {
        let (ctx, received, _broker) = wired_probe();
        (ctx, received)
    }

    fn wired_probe() -> (Ctx<Probe>, Inbox, Arc<MockBroker>) {
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

    async fn event(received: &mut Inbox) -> Option<u8> {
        match received.recv().await {
            Some(Input::Event(event)) => Some(event),
            _ => None,
        }
    }

    #[tokio::test]
    async fn a_spawned_task_delivers_the_event_it_returns() {
        let (ctx, mut received) = probe();
        let _source = ctx.spawn(|_ctx| async { 7 });

        assert_eq!(event(&mut received).await, Some(7));
    }

    #[tokio::test]
    async fn a_detached_task_runs_and_delivers_nothing() {
        let (ctx, mut received) = probe();
        let (ran, finished) = tokio::sync::oneshot::channel();
        let _source = ctx.spawn_detached(|_ctx| async move {
            let _ = ran.send(());
        });

        finished.await.expect("the task ran");
        assert!(received.try_recv().is_err(), "nothing reached the inbox");
    }

    #[tokio::test]
    async fn a_stream_delivers_every_item_it_yields() {
        let (ctx, mut received) = probe();
        let _source = ctx.stream(|_ctx| async { stream::iter([1, 2, 3]) });

        assert_eq!(event(&mut received).await, Some(1));
        assert_eq!(event(&mut received).await, Some(2));
        assert_eq!(event(&mut received).await, Some(3));
    }

    #[tokio::test]
    async fn a_tick_reaches_the_handler_as_an_event() {
        let (ctx, mut received) = probe();
        let _source = ctx.interval(time::Duration::from_millis(1), |_ctx| async { 9 });

        assert_eq!(event(&mut received).await, Some(9));
    }

    #[tokio::test]
    async fn a_subscription_delivers_the_mapped_payload() {
        let (ctx, mut received, broker) = wired_probe();
        let _source = ctx.subscribe::<Ping>(|ping| ping.value);

        broker.deliver(Ping::NAME, &serde_json::json!({ "value": 3 }));

        assert_eq!(event(&mut received).await, Some(3));
    }

    /// The property the shared cell exists for: the broker never waits, so a burst it delivers
    /// before the pump wakes must collapse to the newest rather than queue or drop the latest.
    #[tokio::test]
    async fn a_burst_collapses_to_its_newest_payload() {
        let (ctx, mut received, broker) = wired_probe();
        let _source = ctx.subscribe::<Ping>(|ping| ping.value);

        // Nothing is awaited between these, so the pump has not run and all three land in the cell.
        for value in [1, 2, 3] {
            broker.deliver(Ping::NAME, &serde_json::json!({ "value": value }));
        }

        assert_eq!(event(&mut received).await, Some(3));
        assert!(received.try_recv().is_err(), "1 and 2 were superseded");
    }

    /// A payload that does not decode is one bad publisher, not a reason to stop following the
    /// topic — without this the subscription would go silent and report nothing.
    #[tokio::test]
    async fn an_undecodable_payload_leaves_the_subscription_running() {
        let (ctx, mut received, broker) = wired_probe();
        let _source = ctx.subscribe::<Ping>(|ping| ping.value);

        broker.deliver(Ping::NAME, &serde_json::json!({ "value": "not a number" }));
        broker.deliver(Ping::NAME, &serde_json::json!({ "value": 5 }));

        assert_eq!(event(&mut received).await, Some(5));
    }

    /// Without this the task simply vanishes and the service keeps reporting itself healthy while
    /// one of its sources is gone.
    #[tokio::test]
    async fn a_panicking_source_degrades_its_service() {
        let (ctx, _received, mock) = wired_probe();

        let _source = ctx.spawn_detached(|_ctx| async { panic!("the backend sent nonsense") });
        // Let the task reach its panic before cancelling: `shutdown` races the cancel branch of the
        // source's own select, and a cancelled task never panics at all.
        tokio::task::yield_now().await;
        ctx.shutdown().await;

        assert!(
            mock.health().iter().any(|(_, state)| matches!(
                state,
                ServiceState::Degraded { reason } if reason.contains("nonsense")
            )),
            "expected a Degraded naming the panic, got {:?}",
            mock.health()
        );
    }

    /// The reason `degraded` is shared rather than owned: a task that degrades the service through
    /// its own `Ctx` has to be visible to the runtime, which holds the original.
    #[tokio::test]
    async fn a_clone_shares_the_degraded_flag_with_the_original() {
        let (ctx, _received) = probe();
        assert!(!ctx.is_degraded());

        ctx.clone().degraded("no bus");
        assert!(ctx.is_degraded());
    }
}
