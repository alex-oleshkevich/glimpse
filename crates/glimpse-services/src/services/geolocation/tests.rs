use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::ServiceState;
use crate::service::ServiceRuntime;

fn manual(latitude: f64, longitude: f64) -> Config {
    Config {
        provider: Provider::Manual(Some(GeoCoordinates {
            latitude,
            longitude,
        })),
    }
}

fn document(geolocation: ConfiguredGeolocation) -> glimpse_config::Config {
    glimpse_config::Config {
        geolocation,
        ..Default::default()
    }
}

fn configured(latitude: f64, longitude: f64) -> ConfiguredGeolocation {
    ConfiguredGeolocation::Manual {
        latitude,
        longitude,
    }
}

#[test]
fn a_manual_table_maps_to_the_pair_it_names() {
    let mapped = Config::from(&document(configured(51.5074, -0.1278)));
    assert_eq!(mapped.provider, manual(51.5074, -0.1278).provider);
}

#[test]
fn geoclue_carries_no_coordinates() {
    let mapped = Config::from(&document(ConfiguredGeolocation::Geoclue));
    assert_eq!(mapped.provider, Provider::Geoclue);
}

#[test]
fn coordinates_outside_their_ranges_are_refused() {
    for (latitude, longitude) in [
        (91.0, 0.0),
        (-91.0, 0.0),
        (0.0, 181.0),
        (0.0, -181.0),
        (f64::NAN, 0.0),
    ] {
        let mapped = Config::from(&document(configured(latitude, longitude)));
        assert_eq!(
            mapped.provider,
            Provider::Manual(None),
            "expected no coordinates from {latitude}/{longitude}"
        );
    }
}

#[test]
fn the_edges_of_the_ranges_are_inside_them() {
    let mapped = Config::from(&document(configured(90.0, 180.0)));
    assert_eq!(mapped.provider, manual(90.0, 180.0).provider);
}

#[tokio::test]
async fn a_geoclue_event_arriving_after_a_switch_to_manual_is_ignored() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Geolocation>::new(
        Config {
            provider: Provider::Geoclue,
        },
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );

    let sender = runtime.sender();
    sender
        .send(Input::Config(manual(51.5074, -0.1278)))
        .await
        .expect("queued");
    sender
        .send(Input::Event(Event::Located(GeoCoordinates {
            latitude: 52.2297,
            longitude: 21.0122,
        })))
        .await
        .expect("queued");

    let running = tokio::spawn(async move {
        runtime.run(GeolocationDependencies::default()).await;
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let _ = running.await;

    assert_eq!(
        handle.snapshot().coordinates,
        Some(GeoCoordinates {
            latitude: 51.5074,
            longitude: -0.1278,
        }),
        "the manual pair must survive the straggler"
    );
}

fn boxed(
    outcomes: Vec<Result<Option<GeoCoordinates>, String>>,
) -> BoxStream<'static, Result<Option<GeoCoordinates>, String>> {
    Box::pin(stream::iter(outcomes))
}

#[tokio::test]
async fn first_fix_skips_no_fix_updates_and_returns_the_first_real_one() {
    let coordinates = GeoCoordinates {
        latitude: 51.5074,
        longitude: -0.1278,
    };
    let updates = boxed(vec![Ok(None), Ok(None), Ok(Some(coordinates.clone()))]);

    match first_fix(updates).await {
        Some(Event::Located(got)) => assert_eq!(got, coordinates),
        other => panic!("expected a fix, got {other:?}"),
    }
}

#[tokio::test]
async fn first_fix_surfaces_an_unreadable_change_instead_of_looping_on_it() {
    let updates = boxed(vec![Err("permission denied".to_owned())]);

    match first_fix(updates).await {
        Some(Event::Unavailable(reason)) => assert_eq!(reason, "permission denied"),
        other => panic!("expected an error, got {other:?}"),
    }
}

#[tokio::test]
async fn first_fix_ends_quietly_once_the_signal_stream_is_gone() {
    let updates = boxed(vec![Ok(None)]);
    assert!(first_fix(updates).await.is_none());
}

#[tokio::test]
async fn a_fix_that_never_arrives_is_bounded_rather_than_awaited_forever() {
    let updates: BoxStream<'static, Result<Option<GeoCoordinates>, String>> =
        Box::pin(stream::pending());

    let outcome = timeout(Duration::from_millis(20), first_fix(updates)).await;
    assert!(
        outcome.is_err(),
        "an unanswered request must not be awaited forever, or InUse would stay pinned"
    );
}

#[test]
fn a_refresh_under_geoclue_advances_the_subscription_key_and_keeps_the_cache() {
    let (sender, receiver) = watch::channel(GeolocationStatus::default());
    let mut service = Geolocation {
        status: Publisher::new(sender),
        provider: Provider::Geoclue,
        cache: None,
        last: None,
        saving: None,
        saves: 0,
        attempt: 0,
        has_fix: true,
    };
    let fix = GeoCoordinates {
        latitude: 48.8566,
        longitude: 2.3522,
    };
    service.publish(Some(fix.clone()));

    let before = service.subscriptions();
    assert_eq!(before.len(), 2);
    assert_eq!(*before[0].key(), Watch::Geoclue { attempt: 0 });
    assert_eq!(*before[1].key(), Watch::Refresh);

    service.refresh();

    let after = service.subscriptions();
    assert_eq!(*after[0].key(), Watch::Geoclue { attempt: 1 });
    assert_ne!(
        before[0].key(),
        after[0].key(),
        "a refresh must change the subscription key, or the runtime leaves the old \
             transient client running instead of releasing it and starting a fresh one"
    );
    assert_eq!(
        *after[1].key(),
        Watch::Refresh,
        "a refresh does not itself change whether a fix is held, so the refresh cadence \
             must not be disturbed by it"
    );
    assert_eq!(
        receiver.borrow().coordinates,
        Some(fix),
        "the cached fix must survive a refresh, which is exactly when the previous \
             client's transient lifetime ends"
    );
}

#[tokio::test]
async fn retry_ticks_re_arms_after_its_period_elapses() {
    let mut ticks = std::pin::pin!(retry_ticks(Duration::from_millis(5)));
    assert!(matches!(ticks.next().await, Some(Event::Retry)));
    assert!(
        matches!(ticks.next().await, Some(Event::Retry)),
        "a retry ticker must re-arm rather than firing once"
    );
}

#[tokio::test]
async fn a_retry_event_advances_the_attempt_through_the_real_handler() {
    let (events, _inbox) = mpsc::channel(8);
    let (state, _state_rx) = watch::channel(GeolocationStatus::default());
    let (health, _health_rx) = watch::channel(ServiceState::Starting);
    let ctx = Ctx::<Geolocation>::new(
        events,
        &CancellationToken::new(),
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let mut service = Geolocation {
        status: ctx.publisher(),
        provider: Provider::Geoclue,
        cache: None,
        last: None,
        saving: None,
        saves: 0,
        attempt: 0,
        has_fix: false,
    };

    service.handle(&ctx, Input::Event(Event::Retry)).await;

    assert_eq!(
        service.attempt, 1,
        "a retry tick must reach `refresh` through the real handler, the same as an \
             explicit `geolocation.refresh` command"
    );
}

#[tokio::test]
async fn a_source_torn_down_mid_wait_still_runs_its_release_effect() {
    let (events, _inbox) = mpsc::channel(8);
    let (state, _state_rx) = watch::channel(GeolocationStatus::default());
    let (health, _health_rx) = watch::channel(ServiceState::Starting);
    let ctx = Ctx::<Geolocation>::new(
        events,
        &CancellationToken::new(),
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );

    let (sender, confirmed) = oneshot::channel::<()>();

    let guard = ctx.stream(move |ctx| async move {
        let _transient = TransientClient::new(ctx, move || async move {
            let _ = sender.send(());
        });
        std::future::pending::<BoxStream<'static, Event>>().await
    });

    tokio::task::yield_now().await;
    drop(guard);

    timeout(Duration::from_secs(1), confirmed)
        .await
        .expect(
            "the release effect must still run when the source is hard-aborted mid-wait, \
                 the same abort `Live::reconcile` uses when a subscription key stops being \
                 declared",
        )
        .expect("the release sender must not be dropped without sending");
}

#[tokio::test]
async fn an_unavailable_geoclue_source_marks_degraded_without_erasing_the_cached_fix() {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Geolocation>::new(
        Config {
            provider: Provider::Geoclue,
        },
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );

    let sender = runtime.sender();
    sender
        .send(Input::Event(Event::Located(GeoCoordinates {
            latitude: 41.9028,
            longitude: 12.4964,
        })))
        .await
        .expect("queued");
    sender
        .send(Input::Event(Event::Unavailable(
            "geoclued restarted".to_owned(),
        )))
        .await
        .expect("queued");

    let running = tokio::spawn(async move {
        runtime.run(GeolocationDependencies::default()).await;
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }

    assert_eq!(
        handle.snapshot().coordinates,
        Some(GeoCoordinates {
            latitude: 41.9028,
            longitude: 12.4964,
        }),
        "an unavailable source means the fix is not current, not that it never happened"
    );
    assert!(
        matches!(&*handle.health().borrow(), ServiceState::Degraded { .. }),
        "the service must still report the trouble even though the cache is untouched"
    );

    cancel.cancel();
    let _ = running.await;
}

fn context() -> (
    Ctx<Geolocation>,
    mpsc::Receiver<Input<Geolocation>>,
    watch::Receiver<GeolocationStatus>,
    watch::Receiver<ServiceState>,
) {
    let (events, inbox) = mpsc::channel(8);
    let (state, state_rx) = watch::channel(GeolocationStatus::default());
    let (health, health_rx) = watch::channel(ServiceState::Starting);
    let ctx = Ctx::<Geolocation>::new(
        events,
        &CancellationToken::new(),
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    (ctx, inbox, state_rx, health_rx)
}

const WARSAW: GeoCoordinates = GeoCoordinates {
    latitude: 52.2297,
    longitude: 21.0122,
};

#[tokio::test]
async fn waiting_for_a_first_fix_is_reported_rather_than_silent() {
    let (ctx, _inbox, state, health) = context();
    let config = Config {
        provider: Provider::Geoclue,
    };
    let dependencies = GeolocationDependencies { cache: None };

    let service = Geolocation::start(&ctx, config, dependencies).await;

    assert_eq!(state.borrow().coordinates, None);
    assert!(matches!(
        &*health.borrow(),
        ServiceState::Degraded { reason } if reason.contains("waiting for a GeoClue fix")
    ));
    assert_eq!(
        service
            .subscriptions()
            .iter()
            .map(|sub| sub.key())
            .collect::<Vec<_>>(),
        vec![&Watch::Geoclue { attempt: 0 }],
        "without a fix the client is held until GeoClue answers, so no ticker may cut it short"
    );
}

#[tokio::test]
async fn a_cold_start_publishes_the_remembered_fix_until_geoclue_answers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("location.json");
    store(&path, &dir.path().join("partial"), &WARSAW)
        .await
        .expect("stored");
    let (ctx, _inbox, state, health) = context();
    let config = Config {
        provider: Provider::Geoclue,
    };
    let dependencies = GeolocationDependencies { cache: Some(path) };

    Geolocation::start(&ctx, config, dependencies).await;

    assert_eq!(state.borrow().coordinates, Some(WARSAW));
    assert!(matches!(
        &*health.borrow(),
        ServiceState::Degraded { reason } if reason.contains("last known location")
    ));
}

#[tokio::test]
async fn a_manual_provider_never_reads_the_remembered_fix() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("location.json");
    store(&path, &dir.path().join("partial"), &WARSAW)
        .await
        .expect("stored");
    let (ctx, _inbox, state, _health) = context();
    let config = Config {
        provider: Provider::Manual(None),
    };
    let dependencies = GeolocationDependencies { cache: Some(path) };

    Geolocation::start(&ctx, config, dependencies).await;

    assert_eq!(state.borrow().coordinates, None);
}

#[tokio::test]
async fn a_geoclue_fix_is_written_private_and_starts_the_refresh_ticker() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("state").join("location.json");
    let (ctx, _inbox, _state, health) = context();
    let config = Config {
        provider: Provider::Geoclue,
    };
    let dependencies = GeolocationDependencies {
        cache: Some(path.clone()),
    };
    let mut service = Geolocation::start(&ctx, config, dependencies).await;

    service
        .handle(&ctx, Input::Event(Event::Located(WARSAW)))
        .await;

    assert_eq!(*health.borrow(), ServiceState::Running);
    assert!(
        service
            .subscriptions()
            .iter()
            .any(|sub| *sub.key() == Watch::Refresh)
    );
    timeout(Duration::from_secs(2), async {
        while load(&path).await.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the fix reaches the disk");
    assert_eq!(load(&path).await, Some(WARSAW));
    let mode = std::fs::metadata(&path)
        .expect("written")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "a location is readable by its owner only"
    );
}

#[tokio::test]
async fn a_remembered_fix_out_of_range_is_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("location.json");
    tokio::fs::write(&path, br#"{"latitude":91.0,"longitude":0.0}"#)
        .await
        .expect("written");

    assert_eq!(load(&path).await, None);
}

fn located(fix: GeoCoordinates) -> Attempt {
    Box::pin(async move { Ok(Some(Event::Located(fix))) })
}

#[tokio::test]
async fn a_failed_first_attempt_is_retried_until_a_fix_ends_the_stream() {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let locator = move || -> Attempt {
        match counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
            0 => Box::pin(async { Err("GeoClue not activatable".to_owned()) }),
            _ => located(WARSAW),
        }
    };

    let events: Vec<Event> = timeout(
        Duration::from_secs(2),
        until_fixed(locator, Duration::from_secs(60), Duration::from_millis(1)).collect(),
    )
    .await
    .expect("the stream ends at the fix");

    assert!(matches!(&events[..], [
        Event::Unavailable(reason),
        Event::Located(fix),
    ] if reason == "GeoClue not activatable" && *fix == WARSAW));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_silent_first_attempt_is_abandoned_at_its_bound_and_retried() {
    let (held, released) = oneshot::channel::<()>();
    let held = std::sync::Mutex::new(Some(held));
    let locator = move || -> Attempt {
        match held.lock().unwrap().take() {
            Some(held) => Box::pin(async move {
                let _held = held;
                std::future::pending().await
            }),
            None => located(WARSAW),
        }
    };

    let events: Vec<Event> = timeout(
        Duration::from_secs(2),
        until_fixed(locator, Duration::from_millis(20), Duration::from_millis(1)).collect(),
    )
    .await
    .expect("a silent attempt is bounded");

    assert!(matches!(&events[..], [
        Event::Unavailable(reason),
        Event::Located(_),
    ] if reason.contains("no fix")));
    assert!(
        released.await.is_err(),
        "the abandoned attempt is dropped, which releases its client"
    );
}

#[tokio::test]
async fn a_refresh_that_never_answers_says_so() {
    let event = bounded(Box::pin(std::future::pending()), Duration::from_millis(20)).await;
    assert!(matches!(event, Event::Unavailable(reason) if reason.contains("no fix")));
}
