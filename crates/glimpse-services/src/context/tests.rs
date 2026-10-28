use super::*;
use crate::testing::{probe, wired_probe};

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
async fn a_panicking_source_degrades_its_service() {
    let (ctx, _received, health) = wired_probe();

    ctx.spawn_detached(|_ctx| async { panic!("the backend sent nonsense") });
    tokio::task::yield_now().await;
    ctx.shutdown().await;

    assert!(matches!(
        &*health.borrow(),
        ServiceState::Degraded { reason } if reason.contains("nonsense")
    ));
}

#[tokio::test]
async fn a_clone_shares_the_degraded_flag_with_the_original() {
    let (ctx, _received) = probe();
    assert!(!ctx.is_degraded());

    ctx.clone().degraded("no bus");
    assert!(ctx.is_degraded());
}
