use super::*;
use crate::testing::PrivateBus;

#[tokio::test(start_paused = true)]
async fn retries_double_cap_reset_and_wake_on_owner_changes() {
    let mut owners = futures_util::stream::pending::<()>();
    let mut delay = RetryDelay::default();
    for milliseconds in [250, 500, 1000, 2000, 4000, 8000, 16000, 30000, 30000] {
        let before = tokio::time::Instant::now();
        assert!(delay.wait(&mut owners).await);
        assert_eq!(before.elapsed(), Duration::from_millis(milliseconds));
    }
    delay.reset();
    let before = tokio::time::Instant::now();
    assert!(delay.wait(&mut owners).await);
    assert_eq!(before.elapsed(), Duration::from_millis(250));
    let mut changed = futures_util::stream::iter([()]);
    let before = tokio::time::Instant::now();
    assert!(delay.wait(&mut changed).await);
    assert_eq!(before.elapsed(), Duration::ZERO);
    assert!(!delay.wait(&mut changed).await);
}

#[tokio::test]
async fn an_unowned_provider_parks_without_backoff() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let name = "me.aresa.Glimpse.RetryTest";
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let mut owners = dbus
        .receive_name_owner_changed_with_args(&[(0, name)])
        .await
        .unwrap();
    let mut delay = RetryDelay::default();
    let (retried, owned) = tokio::join!(
        retry_after_failure(&dbus, &mut owners, name, &mut delay),
        crate::own_name(&connection, name)
    );
    owned.unwrap();
    assert!(retried);
    assert_eq!(delay.0, Duration::from_millis(250));
}
