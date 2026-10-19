use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use futures_util::{Stream, StreamExt, stream};
use tokio::{sync::watch, time};

const RECHECK: time::Duration = time::Duration::from_secs(60);

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
    pub fn key(&self) -> &S::SubKey {
        &self.key
    }

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

    pub fn deadline(key: S::SubKey, at: DateTime<Utc>, event: S::Event) -> Self {
        Self::stream(key, move |_ctx| async move {
            stream::once(async move {
                while let Ok(remaining) = (at - Utc::now()).to_std() {
                    if remaining.is_zero() {
                        break;
                    }
                    time::sleep(remaining.min(RECHECK)).await;
                }
                event
            })
        })
    }

    pub fn watch<T>(
        key: S::SubKey,
        receiver: watch::Receiver<T>,
        map: impl Fn(T) -> S::Event + Send + 'static,
        closed: S::Event,
    ) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        Self {
            key,
            start: Box::new(move |ctx| {
                ctx.stream(move |_ctx| async move {
                    futures_util::stream::unfold(
                        (receiver, map, true),
                        |(mut receiver, map, first)| async move {
                            if !first && receiver.changed().await.is_err() {
                                return None;
                            }
                            let value = receiver.borrow_and_update().clone();
                            Some((map(value), (receiver, map, false)))
                        },
                    )
                    .chain(futures_util::stream::once(async move { closed }))
                })
            }),
        }
    }
}

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
}

#[cfg(test)]
mod tests;
