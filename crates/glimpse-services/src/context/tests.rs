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
