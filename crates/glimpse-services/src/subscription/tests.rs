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

#[tokio::test]
async fn a_watch_delivers_its_snapshot_then_changes() {
    let (ctx, mut received) = probe();
    let (state, receiver) = watch::channel(3);
    let mut live = Live::new();

    live.reconcile(
        &ctx,
        vec![Sub::watch(Watch::First, receiver, |value| value, 9)],
    );
    assert_eq!(event(&mut received).await, Some(3));

    state.send_replace(5);
    assert_eq!(event(&mut received).await, Some(5));
}

#[tokio::test]
async fn a_watch_reports_when_its_sender_closes() {
    let (ctx, mut received) = probe();
    let (state, receiver) = watch::channel(3);
    let mut live = Live::new();

    live.reconcile(
        &ctx,
        vec![Sub::watch(Watch::First, receiver, |value| value, 9)],
    );
    assert_eq!(event(&mut received).await, Some(3));

    drop(state);
    assert_eq!(event(&mut received).await, Some(9));
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
