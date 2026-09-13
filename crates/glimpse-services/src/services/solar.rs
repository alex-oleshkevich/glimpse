use std::f64::consts::TAU;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use glimpse_contracts::{GeoCoordinates, GeolocationStatus, SolarPhase, SolarStatus};
use tokio::{
    sync::{oneshot, watch},
    time,
};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint, ServiceError},
    subscription::Sub,
};

const TICK: time::Duration = time::Duration::from_secs(60);

pub enum Command {
    Refresh {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    Tick,
    Update(Option<GeoCoordinates>),
}

pub struct Solar {
    status: Publisher<Option<SolarStatus>>,
    coordinates: Option<GeoCoordinates>,
    location: watch::Receiver<GeolocationStatus>,
}

pub struct SolarDependencies {
    pub geolocation: super::geolocation::GeolocationHandle,
}

#[derive(Clone)]
pub struct SolarHandle(ServiceEndpoint<Solar>);

impl SolarHandle {
    pub fn snapshot(&self) -> Option<SolarStatus> {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<SolarStatus>> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn refresh(&self) -> Result<(), CommandError> {
        let (reply, answer) = oneshot::channel();
        self.0.command(Command::Refresh { reply })?;
        answer.await.map_err(|_| {
            CommandError::Unavailable("`solar` stopped before refreshing".to_owned())
        })?
    }
}

impl Solar {
    pub fn initial_state() -> Option<SolarStatus> {
        None
    }
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Tick,
    Location,
}

impl Service for Solar {
    const NAME: &'static str = "solar";

    type Config = NoConfig;
    type State = Option<SolarStatus>;
    type Handle = SolarHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = SolarDependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        SolarHandle(endpoint)
    }

    /// The tick re-evaluates a phase that only a location can produce, so without one it would wake
    /// every minute to return immediately.
    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut declared = vec![Sub::watch(
            Watch::Location,
            self.location.clone(),
            |data| Event::Update(data.coordinates),
            Event::Update(None),
        )];
        if self.coordinates.is_some() {
            declared.push(Sub::interval(Watch::Tick, TICK, |_ctx| async {
                Event::Tick
            }));
        }
        declared
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        ctx.degraded("no location yet");
        Ok(Self {
            coordinates: None,
            status: ctx.publisher(),
            location: dependencies.geolocation.subscribe(),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Update(coordinates)) => {
                match &coordinates {
                    Some(_) => ctx.running(),
                    None => ctx.degraded("no location; the solar phase is unknown"),
                }
                self.coordinates = coordinates;
                if self.coordinates.is_none() {
                    self.status.set(None);
                }
                self.refresh();
            }
            Input::Event(Event::Tick) => self.refresh(),
            Input::Command(Command::Refresh { reply }) => {
                self.refresh();
                let _ = reply.send(Ok(()));
            }
            Input::Config(NoConfig) => {}
        }
    }
}

impl Solar {
    fn refresh(&mut self) {
        let Some(coordinates) = self.coordinates.as_ref() else {
            return;
        };
        let now = Local::now();
        if let Some(phase) = phase_at(now.with_timezone(&Utc), now.date_naive(), coordinates) {
            self.status.set(Some(SolarStatus { phase }));
        }
    }
}

/// `None` only for coordinates out of range, which `geolocation` already refuses.
fn phase_at(
    now: DateTime<Utc>,
    date: NaiveDate,
    coordinates: &GeoCoordinates,
) -> Option<SolarPhase> {
    // Both events are offsets from the same solar noon, so sunrise precedes sunset by construction.
    let phase = match crate::sun::events(coordinates, date)? {
        (Some(sunrise), Some(sunset)) => match (sunrise..sunset).contains(&now) {
            true => SolarPhase::Day,
            false => SolarPhase::Night,
        },
        _ => polar_phase(date, coordinates.latitude),
    };
    Some(phase)
}

/// Above the polar circles a date has neither event, and which way it goes follows from whether
/// that hemisphere is in its own summer. Only the sign of the solar declination is asked for, so
/// the axial tilt that Cooper's equation scales it by drops out and this is one cosine. The
/// approximation costs accuracy within about a day of an equinox, a window that reaches nowhere but
/// the poles themselves.
fn polar_phase(date: NaiveDate, latitude: f64) -> SolarPhase {
    let declination = -(TAU * (f64::from(date.ordinal()) + 10.0) / 365.0).cos();
    match declination.signum() == latitude.signum() {
        true => SolarPhase::Day,
        false => SolarPhase::Night,
    }
}

#[cfg(test)]
mod tests {
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
            Geolocation::initial_state(),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let location_config = Config {
            provider: Provider::Manual(coordinates),
        };
        let location_task = tokio::spawn(async move {
            let _ = location_runtime.run(location_config, ()).await;
        });
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }

        let (mut runtime, handle) = ServiceRuntime::<Solar>::new(
            Solar::initial_state(),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let health = handle.health();

        let running = tokio::spawn(async move {
            let _ = runtime
                .run(
                    NoConfig,
                    SolarDependencies {
                        geolocation: location,
                    },
                )
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

    #[tokio::test]
    async fn losing_the_location_invalidates_the_phase() {
        let cancel = CancellationToken::new();
        let (location_runtime, location) = ServiceRuntime::<Geolocation>::new(
            Geolocation::initial_state(),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        drop(location_runtime);
        let (events, _inbox) = tokio::sync::mpsc::channel(4);
        let (state, state_rx) = tokio::sync::watch::channel(Solar::initial_state());
        let (health, _health_rx) = tokio::sync::watch::channel(ServiceState::Starting);
        let ctx = Ctx::<Solar>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let mut solar = Solar::start(
            &ctx,
            NoConfig,
            SolarDependencies {
                geolocation: location,
            },
        )
        .await
        .expect("starts");

        solar
            .handle(&ctx, Input::Event(Event::Update(Some(LONDON))))
            .await;
        assert!(state_rx.borrow().is_some());

        solar.handle(&ctx, Input::Event(Event::Update(None))).await;
        assert_eq!(*state_rx.borrow(), None);
        assert!(ctx.is_degraded());
    }
}
