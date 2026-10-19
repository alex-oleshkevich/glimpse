use super::*;

#[test]
fn a_poll_interval_under_the_floor_is_raised_to_it() {
    for (written, expected) in [(0, 600), (599, 600), (600, 600), (900, 900), (7200, 7200)] {
        let projected = Config::from(&document(glimpse_config::WeatherConfig {
            poll_interval: written,
            ..Default::default()
        }));

        assert_eq!(projected.poll_interval, expected, "for {written}");
    }
}

#[test]
fn forecast_days_is_clamped_to_what_a_provider_will_answer() {
    for (written, expected) in [(0, 1), (1, 1), (7, 7), (10, 10), (99, 10)] {
        let projected = Config::from(&document(glimpse_config::WeatherConfig {
            forecast_days: written,
            ..Default::default()
        }));

        assert_eq!(projected.forecast_days, expected, "for {written}");
    }
}

#[test]
fn the_initial_snapshot_uses_the_configured_units() {
    let config = Config {
        provider: Provider::OpenMeteo,
        units: UnitSystem::Imperial,
        poll_interval: 900,
        forecast_days: 7,
    };

    assert_eq!(initial_state(&config).units, UnitSystem::Imperial);
}

#[tokio::test]
async fn nothing_watched_means_no_request_and_no_location_subscription() {
    let harness = harness().await;

    assert!(harness.service.subscriptions().is_empty());
    assert!(harness.service.request().targets.is_empty());
    assert!(!harness.service.wants_here());
    assert_eq!(
        reason(&harness.health).as_deref(),
        Some("nothing is being watched")
    );
}

#[tokio::test]
async fn the_location_topic_is_subscribed_only_while_something_watches_here() {
    let mut harness = harness().await;

    call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
        .await
        .expect("coordinates are watchable");
    assert!(!harness.service.wants_here());
    assert_eq!(harness.service.subscriptions().len(), 1);

    call(&mut harness, Asked::Watch(WatchedPlace::Here))
        .await
        .expect("here is watchable");
    assert!(harness.service.wants_here());
    assert_eq!(harness.service.subscriptions().len(), 2);
}

#[tokio::test]
async fn here_without_a_fix_asks_for_nothing_and_says_so() {
    let mut harness = harness().await;

    call(&mut harness, Asked::Watch(WatchedPlace::Here))
        .await
        .expect("here is watchable");

    assert!(harness.service.request().targets.is_empty());
    assert_eq!(
        reason(&harness.health).as_deref(),
        Some("there is no location fix yet")
    );
}

#[tokio::test]
async fn a_lease_that_is_not_renewed_ages_out() {
    let mut harness = harness().await;
    let now = Instant::now();

    harness
        .service
        .lease(at(47.3769, 8.5417), now)
        .expect("the first lease");

    assert!(!harness.service.sweep(now + LEASE - Duration::from_secs(1)));
    assert_eq!(harness.service.watched.len(), 1);

    assert!(harness.service.sweep(now + LEASE + Duration::from_secs(1)));
    assert!(harness.service.watched.is_empty());
}

#[tokio::test]
async fn renewing_a_lease_does_not_duplicate_it() {
    let mut harness = harness().await;
    let now = Instant::now();

    assert_eq!(harness.service.lease(WatchedPlace::Here, now), Ok(true));
    assert_eq!(harness.service.lease(WatchedPlace::Here, now), Ok(false));
    assert_eq!(harness.service.watched.len(), 1);
}

#[tokio::test]
async fn renewing_a_lease_does_not_restart_the_poll() {
    let mut harness = harness().await;

    call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
        .await
        .expect("the first watch");
    let after_first = harness.service.generation;

    for _ in 0..5 {
        call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a renewal");
    }

    assert_eq!(harness.service.generation, after_first);
}

#[tokio::test]
async fn watching_past_the_cap_is_refused_but_a_renewal_at_the_cap_still_succeeds() {
    let mut harness = harness().await;

    for index in 0..MOST_WATCHED {
        call(&mut harness, Asked::Watch(at(index as f64, 0.0)))
            .await
            .expect("within the cap");
    }

    let refused = call(&mut harness, Asked::Watch(at(60.0, 0.0)))
        .await
        .expect_err("past the cap");
    assert!(matches!(refused, CommandError::LimitExceeded(_)));

    call(&mut harness, Asked::Watch(at(0.0, 0.0)))
        .await
        .expect("a renewal at the cap");
    assert_eq!(harness.service.watched.len(), MOST_WATCHED);
}

#[tokio::test]
async fn a_stale_lease_is_swept_by_a_watch_rather_than_only_by_a_fetch() {
    let mut harness = harness().await;
    let now = Instant::now();

    for index in 0..MOST_WATCHED {
        harness
            .service
            .lease(at(index as f64, 0.0), now)
            .expect("within the cap");
    }
    for held in &mut harness.service.watched {
        held.until = now;
    }

    call(&mut harness, Asked::Watch(at(60.0, 0.0)))
        .await
        .expect("the cap counts live leases rather than dead ones");

    assert_eq!(harness.service.watched.len(), 1);
}

#[tokio::test]
async fn watching_here_without_a_fix_does_not_restart_the_poll() {
    let mut harness = harness().await;

    call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
        .await
        .expect("the first watch");
    let after_first = harness.service.generation;

    call(&mut harness, Asked::Watch(WatchedPlace::Here))
        .await
        .expect("here is watchable");

    assert!(harness.service.wants_here());
    assert_eq!(harness.service.generation, after_first);
}
