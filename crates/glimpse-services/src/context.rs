use std::sync::Arc;

use glimpse_ipc::topics::Topic;
use serde::Serialize;
use tokio::{sync::mpsc, task::AbortHandle, time};
use tokio_util::task::TaskTracker;

use crate::service::Service;

pub struct Ctx<S: Service> {
    events: mpsc::Sender<S::Event>,
    tasks: TaskTracker,
}

impl<S: Service> Ctx<S> {
    pub fn publisher<T: Topic>(&self) -> Publisher<T::Payload> {
        Publisher {}
    }

    pub fn subscribe<T: Topic>(
        &self,
        map: impl Fn(T::Payload) -> S::Event + Send + 'static,
    ) -> SourceGuard {
        SourceGuard(())
    }

    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        let cancel = self.cancel.clone();
        self.tasks.spawn(async move {
            tokio::select! {
                () = cancel.cancelled() => {},
                () = task => {}
            }
        });
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
        // let pump = self.task
        SourceGuard(())
    }

    pub fn events(&self) -> mpsc::Sender<S::Event> {
        self.events.clone()
    }
}

pub struct Publisher<P> {}

impl<P: Serialize + PartialEq> Publisher<P> {
    pub fn set(&self, _value: P) {}
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
