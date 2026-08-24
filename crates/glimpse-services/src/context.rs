use std::sync::Arc;

use glimpse_ipc::topics::Topic;
use tokio::{sync::mpsc, task::AbortHandle, time};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::broker_handle::{BrokerHandle, SubscriptionId};
use crate::publisher::Publisher;
use crate::service::Service;

pub struct Ctx<S: Service> {
    events: mpsc::Sender<S::Event>,
    tasks: TaskTracker,
    cancel: CancellationToken,
    broker: Arc<dyn BrokerHandle>,
}

impl<S: Service> Ctx<S> {
    pub fn publisher<T: Topic>(&self) -> Publisher<T::Payload> {
        Publisher::new(T::NAME, self.broker.clone())
    }

    pub fn subscribe<T: Topic>(
        &self,
        _map: impl Fn(T::Payload) -> S::Event + Send + 'static,
    ) -> SourceGuard {
        todo!("needs the broker")
    }

    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) -> AbortHandle {
        let cancel = self.cancel.clone();
        self.tasks
            .spawn(async move {
                tokio::select! {
                    () = cancel.cancelled() => {},
                    () = task => {}
                }
            })
            .abort_handle()
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

        SourceGuard {
            abort: self.spawn(async move {
                let mut timer = time::interval_at(start, period);
                timer.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

                loop {
                    timer.tick().await;
                    if events.send(on_tick()).await.is_err() {
                        break;
                    }
                }
            }),
            subscription: None,
        }
    }

    pub fn events(&self) -> mpsc::Sender<S::Event> {
        self.events.clone()
    }
}

pub struct SourceGuard {
    abort: AbortHandle,
    subscription: Option<(Arc<dyn BrokerHandle>, SubscriptionId)>,
}

impl Drop for SourceGuard {
    fn drop(&mut self) {
        self.abort.abort();
        if let Some((broker, id)) = self.subscription.take() {
            broker.unsubscribe(id);
        }
    }
}
