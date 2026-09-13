use std::panic::AssertUnwindSafe;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::{FutureExt, Stream, StreamExt, stream};
use glimpse_dbus::Buses;
use tokio::{
    sync::{mpsc, watch},
    task::AbortHandle,
    time,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::ServiceState;
use crate::publisher::Publisher;
use crate::service::{Input, Service, panic_reason, set_health};

pub struct Ctx<S: Service> {
    events: mpsc::Sender<Input<S>>,
    tasks: TaskTracker,
    cancel: CancellationToken,
    state: Publisher<S::State>,
    health: watch::Sender<ServiceState>,
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
            state: self.state.clone(),
            health: self.health.clone(),
            buses: self.buses.clone(),
            degraded: self.degraded.clone(),
        }
    }
}

impl<S: Service> Ctx<S> {
    pub fn new(
        events: mpsc::Sender<Input<S>>,
        cancel: &CancellationToken,
        state: watch::Sender<S::State>,
        health: watch::Sender<ServiceState>,
        buses: Buses,
    ) -> Self {
        Self {
            events,
            state: Publisher::new(state),
            health,
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

    pub fn publisher(&self) -> Publisher<S::State> {
        self.state.clone()
    }

    pub fn cancel(&self) -> CancellationToken {
        self.cancel.child_token()
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

    pub fn spawn_detached<F, Fut>(&self, task: F)
    where
        F: FnOnce(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let ctx = self.clone();
        self.spawn_raw(async move { task(ctx).await }).detach();
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
        }
    }

    /// The service's inbox sender, for a synchronous foreign callback that cannot be a source.
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

    pub fn degraded(&self, reason: impl Into<String>) {
        self.degraded.store(true, Ordering::Relaxed);
        set_health(
            &self.health,
            ServiceState::Degraded {
                reason: reason.into(),
            },
        );
    }

    /// Withdraw a previous `degraded`, once whatever was missing turns up.
    pub fn running(&self) {
        self.degraded.store(false, Ordering::Relaxed);
        set_health(&self.health, ServiceState::Running);
    }

    pub(crate) fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }
}

#[must_use = "a source stops the moment its guard is dropped; bind it for as long as the source \
              should live, and `let _ = ...` starts a source that is aborted before it runs"]
pub struct SourceGuard {
    abort: Option<AbortHandle>,
}

impl SourceGuard {
    fn detach(mut self) {
        self.abort = None;
    }
}

impl Drop for SourceGuard {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::stream;

    use super::*;
    use crate::testing::{event, probe, wired_probe};

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
        ctx.spawn_detached(|_ctx| async move {
            tokio::task::yield_now().await;
            let _ = ran.send(());
        });

        finished
            .await
            .expect("the task ran rather than being aborted by the statement ending");
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
        let (ctx, _received, health) = wired_probe();

        ctx.spawn_detached(|_ctx| async { panic!("the backend sent nonsense") });
        // Let the task reach its panic before cancelling: `shutdown` races the cancel branch of the
        // source's own select, and a cancelled task never panics at all.
        tokio::task::yield_now().await;
        ctx.shutdown().await;

        assert!(matches!(
            &*health.borrow(),
            ServiceState::Degraded { reason } if reason.contains("nonsense")
        ));
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
