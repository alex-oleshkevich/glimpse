use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::ServiceState;
use crate::service::ServiceRuntime;

/// Built literally rather than through `coordinates`, which is the function under test: a
/// helper that called it would compare its output against itself and assert nothing.
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
    assert_eq!(mapped, manual(51.5074, -0.1278));
}

#[test]
fn geoclue_carries_no_coordinates() {
    let mapped = Config::from(&document(ConfiguredGeolocation::Geoclue));
    assert_eq!(
        mapped,
        Config {
            provider: Provider::Geoclue
        }
    );
}

/// A pair the configuration accepts structurally but that names nowhere on Earth. The service
/// reports it as degraded rather than guessing a location.
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
            mapped,
            Config {
                provider: Provider::Manual(None)
            },
            "expected no coordinates from {latitude}/{longitude}"
        );
    }
}

#[test]
fn the_edges_of_the_ranges_are_inside_them() {
    let mapped = Config::from(&document(configured(90.0, 180.0)));
    assert_eq!(mapped, manual(90.0, 180.0));
}

/// A watch that has been torn down can still have an event waiting in the inbox behind the
/// configuration that tore it down.
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
        .send(Input::Event(Event::Located(Some(GeoCoordinates {
            latitude: 52.2297,
            longitude: 21.0122,
        }))))
        .await
        .expect("queued");

    let running = tokio::spawn(async move {
        let _ = runtime.run(()).await;
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
        Some(Event::Located(Some(got))) => assert_eq!(got, coordinates),
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
        attempt: 0,
        has_fix: false,
    };
    let fix = GeoCoordinates {
        latitude: 48.8566,
        longitude: 2.3522,
    };
    service.publish(Some(fix.clone()));

    let before = service.subscriptions();
    assert_eq!(before.len(), 2);
    assert_eq!(*before[0].key(), Watch::Geoclue { attempt: 0 });
    assert_eq!(*before[1].key(), Watch::Retry { fixed: true });

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
        Watch::Retry { fixed: true },
        "a refresh does not itself change whether a fix is held, so the retry cadence \
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
        .send(Input::Event(Event::Located(Some(GeoCoordinates {
            latitude: 41.9028,
            longitude: 12.4964,
        }))))
        .await
        .expect("queued");
    sender
        .send(Input::Event(Event::Unavailable(
            "geoclued restarted".to_owned(),
        )))
        .await
        .expect("queued");

    let running = tokio::spawn(async move {
        let _ = runtime.run(()).await;
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
