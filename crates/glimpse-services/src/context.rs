use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::{Stream, StreamExt};
use glimpse_contracts::Message;
use glimpse_dbus::Buses;
use serde::Deserialize;
use serde_json::Value;
use tokio::{sync::mpsc, task::AbortHandle, time};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::publisher::Publisher;
use crate::service::Service;
use crate::{BrokerHandle, ServiceState, SubscriptionId};

pub struct Ctx<S: Service> {
    events: mpsc::Sender<S::Event>,
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
        events: mpsc::Sender<S::Event>,
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
                    if events.try_send(map(value)).is_err() {
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
            let _ = events.send(event).await;
        })
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
                if events.send(on_tick(ctx.clone()).await).await.is_err() {
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
                if events.send(event).await.is_err() {
                    break;
                }
            }
        })
    }

    /// The one place a task is registered and made cancellable. Private because a service task that
    /// produces no event has no way to reach its handler, and is therefore not a source.
    fn spawn_raw(&self, task: impl Future<Output = ()> + Send + 'static) -> SourceGuard {
        let cancel = self.cancel.clone();
        let handle = self
            .tasks
            .spawn(async move {
                tokio::select! {
                    () = cancel.cancelled() => {},
                    () = task => {}
                }
            })
            .abort_handle();
        SourceGuard {
            abort: Some(handle),
            subscription: None,
        }
    }

    pub fn events(&self) -> mpsc::Sender<S::Event> {
        self.events.clone()
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

    fn probe() -> (Ctx<Probe>, mpsc::Receiver<u8>) {
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

    #[tokio::test]
    async fn a_spawned_task_delivers_the_event_it_returns() {
        let (ctx, mut received) = probe();
        let _source = ctx.spawn(|_ctx| async { 7 });

        assert_eq!(received.recv().await, Some(7));
    }

    #[tokio::test]
    async fn a_stream_delivers_every_item_it_yields() {
        let (ctx, mut received) = probe();
        let _source = ctx.stream(|_ctx| async { stream::iter([1, 2, 3]) });

        assert_eq!(received.recv().await, Some(1));
        assert_eq!(received.recv().await, Some(2));
        assert_eq!(received.recv().await, Some(3));
    }

    #[tokio::test]
    async fn a_tick_reaches_the_handler_as_an_event() {
        let (ctx, mut received) = probe();
        let _source = ctx.interval(time::Duration::from_millis(1), |_ctx| async { 9 });

        assert_eq!(received.recv().await, Some(9));
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
