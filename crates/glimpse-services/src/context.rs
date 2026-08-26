use std::sync::Arc;

use glimpse_contracts::Message;
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
}

impl<S: Service> Ctx<S> {
    pub fn new(
        events: mpsc::Sender<S::Event>,
        cancel: &CancellationToken,
        broker: Arc<dyn BrokerHandle>,
    ) -> Self {
        Self {
            events,
            broker,
            tasks: TaskTracker::new(),
            cancel: cancel.clone(),
        }
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
        self.broker.report_health(
            S::NAME,
            ServiceState::Degraded {
                reason: reason.into(),
            },
        );
    }

    /// Withdraw a previous `degraded`, once whatever was missing turns up.
    pub fn running(&self) {
        self.broker.report_health(S::NAME, ServiceState::Running);
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
