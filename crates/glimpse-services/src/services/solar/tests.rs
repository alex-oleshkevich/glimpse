use chrono::{TimeDelta, TimeZone};
use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;

use super::super::geolocation::{Config, Geolocation, Provider};
use super::*;
use crate::{ServiceState, service::ServiceRuntime};

const LONDON: GeoCoordinates = GeoCoordinates {
    latitude: 51.5074,
    longitude: -0.1278,
};

const SVALBARD: GeoCoordinates = GeoCoordinates {
    latitude: 78.2232,
    longitude: 15.6267,
};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("a real date")
}

fn midsummer() -> NaiveDate {
    date(2026, 6, 21)
}

fn on_midsummer(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 21, hour, minute, 0)
        .single()
        .expect("one instant")
}

fn phase_at(
    now: DateTime<Utc>,
    date: NaiveDate,
    coordinates: &GeoCoordinates,
) -> Option<SolarPhase> {
    status_at(now, date, coordinates).map(|status| status.phase)
}

#[test]
fn midday_is_day_and_the_hours_either_side_of_it_are_night() {
    assert_eq!(
        phase_at(on_midsummer(12, 0), midsummer(), &LONDON),
        Some(SolarPhase::Day)
    );
    assert_eq!(
        phase_at(on_midsummer(1, 0), midsummer(), &LONDON),
        Some(SolarPhase::Night)
    );
    assert_eq!(
        phase_at(on_midsummer(23, 0), midsummer(), &LONDON),
        Some(SolarPhase::Night)
    );
}

/// The instant a night-light ramp is anchored on. Each of the three arms answers with a
/// different event, and the last one has to leave today altogether.
#[test]
fn the_next_change_is_the_boundary_still_ahead() {
    let (sunrise, sunset) = crate::sun::events(&LONDON, midsummer()).expect("in range");
    let sunrise = sunrise.expect("London has a sunrise in June");
    let sunset = sunset.expect("London has a sunset in June");
    let minute = TimeDelta::minutes(1);
    let next = |now| {
        status_at(now, midsummer(), &LONDON)
            .expect("in range")
            .next_change
    };

    assert_eq!(next(sunrise - minute), Some(sunrise));
    assert_eq!(next(sunrise + minute), Some(sunset));
    let tomorrow = next(sunset + minute).expect("tomorrow rises too");
    assert!(tomorrow > sunset, "the boundary is always still ahead");
}

/// Neither event exists, so there is no instant to name. `None` is the answer rather than a
/// missing one: above the polar circles the phase genuinely does not change today.
#[test]
fn a_polar_date_has_no_next_change() {
    assert_eq!(
        status_at(on_midsummer(2, 0), midsummer(), &SVALBARD)
            .expect("in range")
            .next_change,
        None
    );
}

/// The boundary is the whole of the function: a phase that is `Day` at every instant passes any
/// test that only looks at midday.
#[test]
fn the_phase_flips_across_sunrise() {
    let (sunrise, _) = crate::sun::events(&LONDON, midsummer()).expect("in range");
    let sunrise = sunrise.expect("London has a sunrise in June");
    let minute = TimeDelta::minutes(1);

    assert_eq!(
        phase_at(sunrise - minute, midsummer(), &LONDON),
        Some(SolarPhase::Night)
    );
    assert_eq!(
        phase_at(sunrise + minute, midsummer(), &LONDON),
        Some(SolarPhase::Day)
    );
}

/// All four combinations, because the sign test is the whole of the polar branch and having it
/// backwards is invisible everywhere else.
#[test]
fn a_polar_date_reads_its_own_hemispheres_season() {
    let june = date(2026, 6, 21);
    let december = date(2026, 12, 21);

    assert_eq!(polar_phase(june, 78.0), SolarPhase::Day);
    assert_eq!(polar_phase(december, 78.0), SolarPhase::Night);
    assert_eq!(polar_phase(june, -78.0), SolarPhase::Night);
    assert_eq!(polar_phase(december, -78.0), SolarPhase::Day);
}

/// The branch `sunrise` cannot answer: neither event exists, so an instant that would be the
/// middle of the night anywhere else has to come back `Day`.
#[test]
fn the_midnight_sun_is_day_at_two_in_the_morning() {
    assert_eq!(
        phase_at(on_midsummer(2, 0), midsummer(), &SVALBARD),
        Some(SolarPhase::Day)
    );
}

async fn located(coordinates: Option<GeoCoordinates>) -> (Option<SolarStatus>, ServiceState) {
    let cancel = CancellationToken::new();
    let (mut location_runtime, location) = ServiceRuntime::<Geolocation>::new(
        Config {
            provider: Provider::Manual(coordinates),
        },
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let location_task = tokio::spawn(async move {
        location_runtime.run(Default::default()).await;
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }

    let (mut runtime, handle) = ServiceRuntime::<Solar>::new(
        NoConfig,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let health = handle.health();

    let running = tokio::spawn(async move {
        let _ = runtime
            .run(SolarDependencies {
                geolocation: location,
            })
            .await;
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    let current_health = health.borrow().clone();
    let current_state = handle.snapshot();

    cancel.cancel();
    let _ = running.await;
    let _ = location_task.await;
    (current_state, current_health)
}

/// The phase lands on the location rather than up to a tick later — the tick is not even
/// declared until there are coordinates.
#[tokio::test]
async fn a_location_publishes_a_phase_without_waiting_for_a_tick() {
    let (state, health) = located(Some(LONDON)).await;

    assert!(state.is_some());
    assert_eq!(health, ServiceState::Running);
}

#[tokio::test]
async fn without_a_location_the_service_degrades_and_publishes_nothing() {
    let (state, health) = located(None).await;

    assert_eq!(state, None, "there is no honest phase to publish");
    assert!(matches!(
        health,
        ServiceState::Degraded { reason } if reason.contains("phase is unknown")
    ));
}

async fn started() -> (
    Solar,
    Ctx<Solar>,
    tokio::sync::watch::Receiver<Option<SolarStatus>>,
    tokio::sync::mpsc::Receiver<Input<Solar>>,
) {
    let cancel = CancellationToken::new();
    let (location_runtime, location) = ServiceRuntime::<Geolocation>::new(
        <Geolocation as Service>::Config::from(&glimpse_config::Config::default()),
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    drop(location_runtime);
    let (events, inbox) = tokio::sync::mpsc::channel(4);
    let (state, state_rx) = tokio::sync::watch::channel(Solar::initial_state(&NoConfig));
    let (health, _health_rx) = tokio::sync::watch::channel(ServiceState::Starting);
    let ctx = Ctx::<Solar>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let solar = Solar::start(
        &ctx,
        NoConfig,
        SolarDependencies {
            geolocation: location,
        },
    )
    .await;
    (solar, ctx, state_rx, inbox)
}

#[tokio::test]
async fn losing_the_location_invalidates_the_phase() {
    let (mut solar, ctx, state_rx, _inbox) = started().await;

    solar
        .handle(&ctx, Input::Event(Event::Update(Some(LONDON))))
        .await;
    assert!(state_rx.borrow().is_some());

    solar.handle(&ctx, Input::Event(Event::Update(None))).await;
    assert_eq!(*state_rx.borrow(), None);
    assert!(ctx.is_degraded());
}

#[tokio::test]
async fn the_next_change_is_scheduled_beside_the_tick_and_a_resume() {
    let (mut solar, ctx, state_rx, _inbox) = started().await;

    solar
        .handle(&ctx, Input::Event(Event::Update(Some(LONDON))))
        .await;
    let next = state_rx
        .borrow()
        .as_ref()
        .and_then(|status| status.next_change)
        .expect("London has a next change");

    let declared = solar.subscriptions();
    let keys: Vec<_> = declared.iter().map(|sub| sub.key()).collect();
    assert!(keys.contains(&&Watch::Change(next)));
    assert!(keys.contains(&&Watch::Resume));
    assert!(
        keys.contains(&&Watch::Tick),
        "a clock set back after boot moves no deadline, so the tick stays"
    );
}

#[test]
fn the_solar_day_follows_the_location_not_the_host_clock() {
    let tokyo = GeoCoordinates {
        latitude: 35.6762,
        longitude: 139.6503,
    };
    let now = on_midsummer(21, 0);

    let status = status_at(now, solar_date(now, tokyo.longitude), &tokyo).expect("a status");

    assert_eq!(status.phase, SolarPhase::Day, "06:20 in Tokyo is daylight");
    assert!(status.next_change.is_some_and(|at| at > now));
}
