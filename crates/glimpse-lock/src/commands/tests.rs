use super::*;
use futures_util::stream;

#[tokio::test]
async fn the_lock_waits_for_the_hint_to_turn_true() {
    let cap = Duration::from_millis(200);
    assert!(turns_true(stream::iter([false, true]), cap).await);
    assert!(!turns_true(stream::iter([false, false]), cap).await);
    assert!(!turns_true(stream::pending(), Duration::from_millis(20)).await);
}
