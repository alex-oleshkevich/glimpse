use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::service::ServiceRuntime;

async fn running() -> (
    HeartbeatHandle,
    CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Heartbeat>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let task = tokio::spawn(async move {
        runtime.run(()).await;
    });
    (handle, cancel, task)
}

#[tokio::test]
async fn set_interval_reports_the_period_it_replaced() {
    let (handle, cancel, task) = running().await;
    let value = handle.set_interval(250).await.expect("accepted");
    assert_eq!(value.previous_ms, DEFAULT_PERIOD_MS);
    cancel.cancel();
    let _ = task.await;
}

#[tokio::test]
async fn set_interval_refuses_a_period_outside_the_supported_range() {
    let (handle, cancel, task) = running().await;
    let error = handle.set_interval(0).await.expect_err("refused");
    assert!(matches!(error, CommandError::InvalidArgument(_)));
    cancel.cancel();
    let _ = task.await;
}
