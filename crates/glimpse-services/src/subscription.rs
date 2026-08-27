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

/// The sources a service currently has running, against the ones it declares. Dropping a guard is
/// the whole teardown: it aborts the task and releases the broker subscription behind it.
pub(crate) struct Live<S: Service> {
    running: HashMap<S::SubKey, SourceGuard>,
    warned_duplicate: bool,
}

impl<S: Service> Live<S> {
    pub(crate) fn new() -> Self {
        Self {
            running: HashMap::new(),
            warned_duplicate: false,
        }
    }

    pub(crate) fn reconcile(&mut self, ctx: &Ctx<S>, declared: Vec<Sub<S>>) {
        let keys: HashSet<&S::SubKey> = declared.iter().map(|sub| &sub.key).collect();
        // Said once: this runs after every input, and one of the two sources the service asked for
        // is being dropped on every one of them.
        if keys.len() != declared.len() && !std::mem::replace(&mut self.warned_duplicate, true) {
            tracing::warn!(
                service = S::NAME,
                "two subscriptions share a key; only the first is running"
            );
        }

        self.running.retain(|key, _| keys.contains(&key));

        for sub in declared {
            self.running
                .entry(sub.key)
                .or_insert_with(|| (sub.start)(ctx));
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.running.len()
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{StreamExt, stream};

    use super::*;
    use crate::testing::{Probe, Watch, event, probe};

    fn forever(key: Watch, value: u8) -> Sub<Probe> {
        Sub::stream(key, move |_ctx| async move {
            stream::once(async move { value }).chain(stream::pending())
        })
    }

    #[tokio::test]
    async fn a_declared_key_is_built() {
        let (ctx, mut received) = probe();
        let mut live = Live::new();

        live.reconcile(&ctx, vec![forever(Watch::First, 1)]);

        assert_eq!(event(&mut received).await, Some(1));
        assert_eq!(live.len(), 1);
    }

    /// The property the whole diff exists for: `subscriptions` runs after every input, and an
    /// unchanged key must not cost a teardown and a rebuild each time.
    #[tokio::test]
    async fn a_key_that_stays_is_not_rebuilt() {
        let (ctx, mut received) = probe();
        let mut live = Live::new();

        live.reconcile(&ctx, vec![forever(Watch::First, 1)]);
        assert_eq!(event(&mut received).await, Some(1));

        for _ in 0..3 {
            live.reconcile(&ctx, vec![forever(Watch::First, 1)]);
        }
        // A rebuilt source would deliver its first item on the next poll, so give it the chance.
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }

        assert!(received.try_recv().is_err(), "the source was left running");
    }

    /// A key names one source. Declaring it twice loses the second, which is a service bug worth
    /// pinning down rather than discovering as a source that never runs.
    #[tokio::test]
    async fn two_declarations_of_one_key_build_a_single_source() {
        let (ctx, mut received) = probe();
        let mut live = Live::new();

        live.reconcile(
            &ctx,
            vec![forever(Watch::First, 1), forever(Watch::First, 2)],
        );

        assert_eq!(live.len(), 1);
        assert_eq!(event(&mut received).await, Some(1), "the first one wins");
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        assert!(received.try_recv().is_err(), "the second never started");
    }

    #[tokio::test]
    async fn a_key_that_stops_being_declared_is_torn_down() {
        let (ctx, _received) = probe();
        let mut live = Live::new();

        live.reconcile(&ctx, vec![forever(Watch::First, 1)]);
        live.reconcile(&ctx, Vec::new());

        assert_eq!(live.len(), 0);
    }

    #[tokio::test]
    async fn a_changed_key_replaces_the_source_behind_it() {
        let (ctx, mut received) = probe();
        let mut live = Live::new();

        live.reconcile(&ctx, vec![forever(Watch::First, 1)]);
        assert_eq!(event(&mut received).await, Some(1));

        live.reconcile(&ctx, vec![forever(Watch::Second, 2)]);

        assert_eq!(event(&mut received).await, Some(2));
        assert_eq!(
            live.len(),
            1,
            "the old key is gone, not kept beside the new"
        );
    }
}
