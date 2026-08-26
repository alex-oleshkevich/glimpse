use std::panic::AssertUnwindSafe;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::{FutureExt, Stream, StreamExt};
use glimpse_contracts::Message;
use glimpse_dbus::Buses;
use serde::Deserialize;
use serde_json::Value;
use tokio::{sync::mpsc, task::AbortHandle, time};
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

    pub fn subscribe<T: Message>(
        &self,
        map: impl Fn(T::Payload) -> S::Event + Send + 'static,
    ) -> SourceGuard {
        let events = self.events();
        let id = self.broker.subscribe(
            T::NAME,
            Box::new(move |data: &Value| match T::Payload::deserialize(data) {
                Ok(value) => {
                    if events.try_send(Input::Event(map(value))).is_err() {
                        tracing::warn!(topic = T::NAME, "dropped a topic update");
                    }
                }
                Err(err) => tracing::warn!(topic=T::NAME, %err, "undecodable payload"),
            }),
        );
        SourceGuard {
            abort: None,
            subscription: Some((self.broker.clone(), id)),
        }
    }

    /// One unit of asynchronous work whose result is one event. The task is handed a `Ctx` of its
    /// own, so it reaches the buses, the publishers and `degraded` without any of them being
    /// threaded through its arguments.
    pub fn spawn<F, Fut>(&self, task: F) -> SourceGuard
    where
        F: FnOnce(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = S::Event> + Send + 'static,
    {
        let ctx = self.clone();
        let events = self.events.clone();

        self.spawn_raw(async move {
            let event = task(ctx).await;
            let _ = events.send(Input::Event(event)).await;
        })
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
        let ctx = self.clone();
        let events = self.events.clone();

        self.spawn_raw(async move {
            let mut timer = time::interval_at(start, period);
            timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                timer.tick().await;
                let event = Input::Event(on_tick(ctx.clone()).await);
                if events.send(event).await.is_err() {
                    break;
                }
            }
        })
    }

    /// A backend that produces events for as long as the service lives. The closure is async
    /// because building such a source usually is — a D-Bus signal stream has to be requested
    /// before it can be read — and everything it yields reaches the handler as an event.
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

    /// The service's inbox sender, for the one case the sources do not cover: a synchronous
    /// callback from a foreign thread, as `notify`'s debouncer delivers.
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

    struct Probe;

    impl Service for Probe {
        const NAME: &'static str = "probe";
        const TOPICS: &'static [&'static str] = &[];

        type Config = ();
        type Command = ();
        type Event = u8;

        async fn start(_ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
            Ok(Self)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}

        fn peek_config(_config: &glimpse_config::Config) -> Self::Config {}
    }

    type Inbox = mpsc::Receiver<Input<Probe>>;

    fn probe() -> (Ctx<Probe>, Inbox) {
        let (events, received) = mpsc::channel(8);
        let broker: Arc<dyn BrokerHandle> = Arc::new(MockBroker::default());
        let ctx = Ctx::new(
            events,
            &CancellationToken::new(),
            broker,
            Buses::unavailable("no bus in tests"),
        );
        (ctx, received)
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

    /// Without this the task simply vanishes and the service keeps reporting itself healthy while
    /// one of its sources is gone.
    #[tokio::test]
    async fn a_panicking_source_degrades_its_service() {
        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let (events, _received) = mpsc::channel(8);
        let ctx = Ctx::<Probe>::new(
            events,
            &CancellationToken::new(),
            broker,
            Buses::unavailable("no bus in tests"),
        );

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
