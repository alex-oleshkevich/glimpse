use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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
    degraded: AtomicBool,
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
            degraded: AtomicBool::new(false),
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

    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) -> SourceGuard {
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

    pub fn interval(
        &self,
        period: time::Duration,
        on_tick: impl Fn() -> S::Event + Send + 'static,
    ) -> SourceGuard {
        self.at_interval(time::Instant::now(), period, on_tick)
    }

    pub fn at_interval(
        &self,
        start: time::Instant,
        period: time::Duration,
        on_tick: impl Fn() -> S::Event + Send + 'static,
    ) -> SourceGuard {
        let events = self.events.clone();

        self.spawn(async move {
            let mut timer = time::interval_at(start, period);
            timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                timer.tick().await;
                if events.send(on_tick()).await.is_err() {
                    break;
                }
            }
        })
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
