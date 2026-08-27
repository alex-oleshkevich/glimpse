use std::collections::{HashMap, HashSet};

use futures_util::Stream;
use glimpse_contracts::Message;
use tokio::time;

use crate::context::{Ctx, SourceGuard};
use crate::service::Service;

/// A source declared rather than started. The runtime diffs the declared set against the running
/// one after every input, so a source lives exactly as long as the service keeps naming it.
///
/// `key` is the identity a boxed closure cannot supply: same key means same source and it is left
/// alone, a key that stops appearing is torn down, a key that appears is built. Whatever must force
/// a restart therefore belongs in the key, and whatever must not must stay out of it.
pub struct Sub<S: Service> {
    key: S::SubKey,
    start: Start<S>,
}

type Start<S> = Box<dyn FnOnce(&Ctx<S>) -> SourceGuard + Send>;

impl<S: Service> Sub<S> {
    pub fn stream<F, Fut, St>(key: S::SubKey, source: F) -> Self
    where
        F: FnOnce(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = St> + Send + 'static,
        St: Stream<Item = S::Event> + Send + 'static,
    {
        Self {
            key,
            start: Box::new(move |ctx| ctx.stream(source)),
        }
    }

    pub fn interval<F, Fut>(key: S::SubKey, period: time::Duration, on_tick: F) -> Self
    where
        F: Fn(Ctx<S>) -> Fut + Send + 'static,
        Fut: Future<Output = S::Event> + Send + 'static,
    {
        Self {
            key,
            start: Box::new(move |ctx| ctx.interval(period, on_tick)),
        }
    }

    pub fn topic<T: Message>(
        key: S::SubKey,
        map: impl Fn(T::Payload) -> S::Event + Send + 'static,
    ) -> Self {
        Self {
            key,
            start: Box::new(move |ctx| ctx.subscribe::<T>(map)),
        }
    }
}

/// What the service declared, against what is running. Dropping a guard is the whole teardown: it
/// aborts the task and releases the broker subscription behind it.
pub(crate) fn reconcile<S: Service>(
    ctx: &Ctx<S>,
    live: &mut HashMap<S::SubKey, SourceGuard>,
    declared: Vec<Sub<S>>,
) {
    let declared_keys: HashSet<&S::SubKey> = declared.iter().map(|sub| &sub.key).collect();
    // One of the two sources the service asked for is about to be dropped on the floor.
    if declared_keys.len() != declared.len() {
        tracing::warn!(
            service = S::NAME,
            "two subscriptions share a key; only the first is running"
        );
    }

    live.retain(|key, _| declared_keys.contains(&key));

    for sub in declared {
        live.entry(sub.key).or_insert_with(|| (sub.start)(ctx));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::{StreamExt, stream};
    use glimpse_dbus::Buses;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::service::{Input, ServiceError};
    use crate::{BrokerHandle, MockBroker};

    #[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
    enum Watch {
        First,
        Second,
    }

    struct Probe;

    impl Service for Probe {
        const NAME: &'static str = "probe";
        const TOPICS: &'static [&'static str] = &[];

        type Config = ();
        type Command = ();
        type Event = u8;
        type SubKey = Watch;

        async fn start(_ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
            Ok(Self)
        }

        async fn handle(&mut self, _ctx: &Ctx<Self>, _input: Input<Self>) {}

        fn peek_config(_config: &glimpse_config::Config) -> Self::Config {}
    }

    fn probe() -> (Ctx<Probe>, mpsc::Receiver<Input<Probe>>) {
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

    async fn event(received: &mut mpsc::Receiver<Input<Probe>>) -> Option<u8> {
        match received.recv().await {
            Some(Input::Event(event)) => Some(event),
            _ => None,
        }
    }

    fn forever(key: Watch, value: u8) -> Sub<Probe> {
        Sub::stream(key, move |_ctx| async move {
            stream::once(async move { value }).chain(stream::pending())
        })
    }

    #[tokio::test]
    async fn a_declared_key_is_built() {
        let (ctx, mut received) = probe();
        let mut live = HashMap::new();

        reconcile(&ctx, &mut live, vec![forever(Watch::First, 1)]);

        assert_eq!(event(&mut received).await, Some(1));
        assert_eq!(live.len(), 1);
    }

    /// The property the whole diff exists for: `subscriptions` runs after every input, and an
    /// unchanged key must not cost a teardown and a rebuild each time.
    #[tokio::test]
    async fn a_key_that_stays_is_not_rebuilt() {
        let (ctx, mut received) = probe();
        let mut live = HashMap::new();

        reconcile(&ctx, &mut live, vec![forever(Watch::First, 1)]);
        assert_eq!(event(&mut received).await, Some(1));

        for _ in 0..3 {
            reconcile(&ctx, &mut live, vec![forever(Watch::First, 1)]);
        }
        // A rebuilt source would deliver its first item on the next poll, so give it the chance.
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }

        assert!(received.try_recv().is_err(), "the source was left running");
    }

    #[tokio::test]
    async fn a_key_that_stops_being_declared_is_torn_down() {
        let (ctx, _received) = probe();
        let mut live = HashMap::new();

        reconcile(&ctx, &mut live, vec![forever(Watch::First, 1)]);
        reconcile(&ctx, &mut live, Vec::new());

        assert!(live.is_empty());
    }

    #[tokio::test]
    async fn a_changed_key_replaces_the_source_behind_it() {
        let (ctx, mut received) = probe();
        let mut live = HashMap::new();

        reconcile(&ctx, &mut live, vec![forever(Watch::First, 1)]);
        assert_eq!(event(&mut received).await, Some(1));

        reconcile(&ctx, &mut live, vec![forever(Watch::Second, 2)]);

        assert_eq!(event(&mut received).await, Some(2));
        assert_eq!(
            live.len(),
            1,
            "the old key is gone, not kept beside the new"
        );
    }
}
