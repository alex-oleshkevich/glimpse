use super::service::*;
use super::*;

async fn start(harness: &mut Harness) {
    let (reply, _result) = tokio::sync::oneshot::channel();
    harness
        .service
        .handle(&harness.ctx, Input::Command(Command::StartScan { reply }))
        .await;
}

#[tokio::test]
async fn a_scan_takes_its_deadline_from_the_configured_timeout() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    start(&mut harness).await;
    assert!(
        harness.service.deadline.is_some(),
        "the default [network] scan-timeout is what stops a scan nobody closed"
    );

    assert!(
        harness
            .service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::ScanDeadline(_))),
        "the deadline is declared as a subscription, not started by hand"
    );

    let forever = glimpse_config::Config {
        network: glimpse_config::NetworkSettings {
            scan_timeout: 0,
            ..glimpse_config::NetworkSettings::default()
        },
        ..glimpse_config::Config::default()
    };
    harness
        .service
        .handle(&harness.ctx, Input::Config(Config::from(&forever)))
        .await;
    start(&mut harness).await;
    assert!(
        harness.service.deadline.is_none(),
        "zero disables the timeout and leaves the scan running until the popover closes"
    );
}

#[tokio::test]
async fn a_radio_that_is_off_refuses_a_scan_rather_than_asking_networkmanager() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    harness.service.manager.wireless_enabled = Some(false);

    let (reply, result) = tokio::sync::oneshot::channel();
    harness
        .service
        .handle(&harness.ctx, Input::Command(Command::StartScan { reply }))
        .await;

    assert!(
        matches!(
            result.await.expect("the sender was not dropped"),
            Err(NetworkError::Failed(Failure::NoDevice))
        ),
        "a popover opening over a switched-off radio must not ask the bus to scan"
    );
    assert!(harness.service.scan.is_none());
}

#[tokio::test]
async fn a_stale_deadline_cannot_stop_the_scan_that_replaced_it() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    start(&mut harness).await;
    let first = harness.service.scan.expect("a scan");
    start(&mut harness).await;

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::ScanExpired(first)))
        .await;

    assert!(
        harness.service.scan.is_some(),
        "the generation is what makes an expired older scan harmless"
    );
}

#[tokio::test]
async fn its_own_deadline_stops_it() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    start(&mut harness).await;
    let scan = harness.service.scan.expect("a scan");

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::ScanExpired(scan)))
        .await;

    assert!(harness.service.scan.is_none());
    assert!(harness.service.deadline.is_none());
    assert!(!harness.state.borrow_and_update().scanning);
}

#[tokio::test]
async fn stopping_is_bookkeeping_and_answers_even_with_no_bus() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    start(&mut harness).await;

    let (reply, result) = tokio::sync::oneshot::channel();
    harness
        .service
        .handle(&harness.ctx, Input::Command(Command::StopScan { reply }))
        .await;

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), result)
        .await
        .expect("it answers rather than waiting for a bus")
        .expect("the sender lived");
    assert!(
        outcome.is_ok(),
        "NetworkManager has no explicit stop to call"
    );
    assert!(harness.service.scan.is_none());
}

#[tokio::test]
async fn the_radio_going_away_clears_a_running_scan() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    start(&mut harness).await;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::PropertiesChanged {
                path: nm::MANAGER.to_owned(),
                interface: nm::MANAGER1.to_owned(),
                changed: properties(vec![("WirelessEnabled", owned(false))]),
                invalidated: Vec::new(),
            }),
        )
        .await;

    assert!(
        harness.service.scan.is_none(),
        "a scan cannot outlive the radio it runs on"
    );
}

#[tokio::test]
async fn networkmanager_disappearing_clears_a_running_scan() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    start(&mut harness).await;

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::NameOwner(None)))
        .await;

    assert!(harness.service.scan.is_none());
}

#[tokio::test]
async fn missed_owner_messages_re_read_without_dropping_a_running_scan() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    start(&mut harness).await;
    let before = harness.service.generation;

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::OwnerMissed))
        .await;

    assert!(harness.service.scan.is_some());
    assert_ne!(harness.service.generation, before);
    assert!(!harness.service.adopted);
}
