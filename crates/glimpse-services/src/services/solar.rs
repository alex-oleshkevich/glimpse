use std::f64::consts::TAU;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use glimpse_contracts::{
    Command as _, GeoCoordinates, GeolocationStatus, Message, SolarPhase, SolarRefresh, SolarStatus,
};
use glimpse_ipc::CallError;
use serde_json::Value;
use sunrise::{Coordinates as SunriseCoordinates, SolarDay, SolarEvent};
use tokio::time;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, NoConfig, Service, ServiceError, unknown_command},
    subscription::Sub,
};

const TICK: time::Duration = time::Duration::from_secs(60);

#[derive(Debug, PartialEq)]
pub enum Command {
    Refresh,
}

pub enum Event {
    Tick,
    Update(Option<GeoCoordinates>),
}

pub struct Solar {
    status: Publisher<SolarStatus>,
    coordinates: Option<GeoCoordinates>,
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Tick,
    Location,
}

impl Service for Solar {
    const NAME: &'static str = "solar";
    const TOPICS: &'static [&'static str] = &[SolarStatus::NAME];
    const METHODS: &'static [&'static str] = &[SolarRefresh::NAME];

    type Config = NoConfig;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    /// The tick re-evaluates a phase that only a location can produce, so without one it would wake
    /// every minute to return immediately.
    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut declared = vec![Sub::topic::<GeolocationStatus>(Watch::Location, |data| {
            Event::Update(data.coordinates)
        })];
        if self.coordinates.is_some() {
            declared.push(Sub::interval(Watch::Tick, TICK, |_ctx| async {
                Event::Tick
            }));
        }
        declared
    }

    fn decode(method: &str, _args: Value) -> Result<Self::Command, CallError> {
        match method {
            SolarRefresh::NAME => Ok(Command::Refresh),
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        ctx.degraded("no location yet");
        Ok(Self {
            coordinates: None,
            status: ctx.publisher::<SolarStatus>(),
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
                self.refresh();
            }
            Input::Event(Event::Tick) => self.refresh(),
            Input::Command(Command::Refresh, responder) => {
                self.refresh();
                responder.ok(());
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
            self.status.set(SolarStatus { phase });
        }
    }
}

/// `None` only for coordinates out of range, which `geolocation` already refuses.
fn phase_at(
    now: DateTime<Utc>,
    date: NaiveDate,
    coordinates: &GeoCoordinates,
) -> Option<SolarPhase> {
    let latlon = SunriseCoordinates::new(coordinates.latitude, coordinates.longitude)?;
    let day = SolarDay::new(latlon, date);

    // Both events are offsets from the same solar noon, so sunrise precedes sunset by construction.
    let phase = match (
        day.event_time(SolarEvent::Sunrise),
        day.event_time(SolarEvent::Sunset),
    ) {
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
    use std::sync::Arc;

    use chrono::{TimeDelta, TimeZone};
    use glimpse_dbus::Buses;
    use glimpse_ipc::ErrorCode;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{BrokerHandle, MockBroker, ServiceState, service::ServiceRuntime};

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
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Solar>();
    }

    #[test]
    fn decode_answers_the_method_it_declares_and_refuses_the_rest() {
        assert_eq!(
            Solar::decode(SolarRefresh::NAME, Value::Null).expect("declared"),
            Command::Refresh
        );
        assert_eq!(
            Solar::decode("solar.set_phase", Value::Null)
                .expect_err("never declared")
                .code,
            ErrorCode::UnknownCommand
        );
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
        let latlon = SunriseCoordinates::new(LONDON.latitude, LONDON.longitude).expect("in range");
        let sunrise = SolarDay::new(latlon, midsummer())
            .event_time(SolarEvent::Sunrise)
            .expect("London has one in June");
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

    fn phases(mock: &MockBroker) -> Vec<SolarPhase> {
        mock.published()
            .into_iter()
            .filter(|(topic, _)| topic == SolarStatus::NAME)
            .filter_map(|(_, data)| serde_json::from_value::<SolarStatus>(data).ok())
            .map(|status| status.phase)
            .collect()
    }

    async fn located(coordinates: Option<GeoCoordinates>) -> Arc<MockBroker> {
        let mock = Arc::new(MockBroker::default());
        let broker: Arc<dyn BrokerHandle> = mock.clone();
        let cancel = CancellationToken::new();
        let mut runtime = ServiceRuntime::<Solar>::new(
            broker,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );

        let running = tokio::spawn(async move {
            let _ = runtime.run(NoConfig).await;
        });
        // The subscription is declared after `start`, so it has to exist before anything is
        // delivered into it.
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }

        let payload =
            serde_json::to_value(GeolocationStatus { coordinates }).expect("a wire payload");
        mock.deliver(GeolocationStatus::NAME, &payload);
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }

        cancel.cancel();
        let _ = running.await;
        mock
    }

    /// The phase lands on the location rather than up to a tick later — the tick is not even
    /// declared until there are coordinates.
    #[tokio::test]
    async fn a_location_publishes_a_phase_without_waiting_for_a_tick() {
        let mock = located(Some(LONDON)).await;

        assert_eq!(phases(&mock).len(), 1, "one phase, on the location");
        // `start` degrades, so the runtime never reports `Running` over the top of it: a `Running`
        // anywhere in the log can only be the one the location withdrew it with.
        assert!(
            mock.health()
                .iter()
                .any(|(_, state)| *state == ServiceState::Running),
            "a located service withdraws its degraded state, got {:?}",
            mock.health()
        );
    }

    #[tokio::test]
    async fn without_a_location_the_service_degrades_and_publishes_nothing() {
        let mock = located(None).await;

        assert!(
            phases(&mock).is_empty(),
            "there is no honest phase to publish"
        );
        // The reason, not just the state: `start` degrades too, so anything vaguer passes whether
        // or not the location branch ever ran.
        assert!(
            mock.health().iter().any(|(_, state)| matches!(
                state,
                ServiceState::Degraded { reason } if reason.contains("phase is unknown")
            )),
            "expected a Degraded naming the unknown phase, got {:?}",
            mock.health()
        );
    }
}
