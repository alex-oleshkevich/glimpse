use std::time::{Duration, Instant};

use chrono::{DateTime, TimeZone, Utc};
use glimpse_config::{WeatherProvider as ConfiguredProvider, WeatherUnits as ConfiguredUnits};
use glimpse_contracts::{
    Command as _, Condition, CurrentWeather, DayForecast, GeoCoordinates, GeolocationStatus,
    HourForecast, Message, PlaceWeather, UnitSystem, WatchedPlace, WeatherRefresh, WeatherStatus,
    WeatherWatch,
};
use glimpse_ipc::{CallError, ErrorCode};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, Service, ServiceError, decode_args, unknown_command},
    subscription::Sub,
};

/// The provider recomputes current conditions every fifteen minutes, so a shorter interval asks
/// again for data that provably has not moved and spends a shared free-tier budget doing it.
const MIN_POLL: u64 = 600;
const MAX_DAYS: u8 = 10;
const HOURS: usize = 24;
const MOST_WATCHED: usize = 8;

/// How long a `weather.watch` is honoured without being asked again. Nothing tells this service
/// that a client went away, so a registration that never expired would pin a location and keep
/// fetching for it until the daemon restarted.
const LEASE: Duration = Duration::from_secs(1800);

/// How far a fix has to move before it is worth asking again. Below this the provider would answer
/// from the same grid cell, and a fix that jitters by metres would refetch for ever.
const MOVED: f64 = 1000.0;

const EARTH: f64 = 6_371_000.0;
const TIMEOUT: Duration = Duration::from_secs(20);
const AGENT: &str = concat!("glimpse/", env!("CARGO_PKG_VERSION"));
const ENDPOINT: &str = "https://api.open-meteo.com/v1/forecast";
const REASON: usize = 240;

const CURRENT: &str = "temperature_2m,apparent_temperature,relative_humidity_2m,precipitation,weather_code,wind_speed_10m,wind_direction_10m,is_day";
const HOURLY: &str = "temperature_2m,weather_code,is_day";
const DAILY: &str = "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenMeteo,
}

impl Provider {
    async fn fetch(self, client: reqwest::Client, ask: Ask) -> Result<Vec<Reading>, String> {
        match self {
            Self::OpenMeteo => open_meteo(client, ask).await,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    provider: Provider,
    units: UnitSystem,
    poll_interval: u64,
    forecast_days: u8,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            provider: match document.weather.provider {
                ConfiguredProvider::OpenMeteo => Provider::OpenMeteo,
            },
            units: match document.weather.units {
                ConfiguredUnits::Metric => UnitSystem::Metric,
                ConfiguredUnits::Imperial => UnitSystem::Imperial,
            },
            poll_interval: document.weather.poll_interval.max(MIN_POLL),
            forecast_days: document.weather.forecast_days.clamp(1, MAX_DAYS),
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Watch(WatchedPlace),
    Refresh,
}

pub enum Event {
    Located(Option<GeoCoordinates>),
    Fetched {
        generation: u64,
        result: Result<Vec<Reading>, String>,
    },
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Location,
    Poll { generation: u64 },
}

#[derive(Debug, Clone)]
struct Lease {
    place: WatchedPlace,
    until: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    coordinates: Vec<GeoCoordinates>,
    units: UnitSystem,
    forecast_days: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    utc_offset_seconds: i32,
    current: Option<CurrentWeather>,
    hours: Vec<HourForecast>,
    days: Vec<DayForecast>,
}

pub struct Weather {
    status: Publisher<WeatherStatus>,
    client: Option<reqwest::Client>,
    config: Config,
    watched: Vec<Lease>,
    fix: Option<GeoCoordinates>,
    generation: u64,
    places: Vec<PlaceWeather>,
    failure: Option<String>,
}

impl Service for Weather {
    const NAME: &'static str = "weather";
    const TOPICS: &'static [&'static str] = &[WeatherStatus::NAME];
    const METHODS: &'static [&'static str] = &[WeatherWatch::NAME, WeatherRefresh::NAME];

    type Config = Config;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    fn decode(method: &str, args: Value) -> Result<Self::Command, CallError> {
        match method {
            WeatherWatch::NAME => {
                let WeatherWatch { place } = decode_args(args)?;
                Ok(Command::Watch(placed(place)?))
            }
            WeatherRefresh::NAME => Ok(Command::Refresh),
            _ => Err(unknown_command(Self::NAME, method)),
        }
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut declared = Vec::new();

        if self.wants_here() {
            declared.push(Sub::topic::<GeolocationStatus>(Watch::Location, |data| {
                Event::Located(data.coordinates)
            }));
        }

        let ask = self.ask();
        if !ask.coordinates.is_empty()
            && let Some(client) = self.client.clone()
        {
            let generation = self.generation;
            let provider = self.config.provider;

            declared.push(Sub::interval(
                Watch::Poll { generation },
                Duration::from_secs(self.config.poll_interval.max(MIN_POLL)),
                move |_ctx| {
                    let client = client.clone();
                    let ask = ask.clone();
                    async move {
                        Event::Fetched {
                            generation,
                            result: provider.fetch(client, ask).await,
                        }
                    }
                },
            ));
        }

        declared
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        let client = match reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(AGENT)
            .build()
        {
            Ok(client) => Some(client),
            Err(_) => {
                ctx.degraded("no http client; no forecast can be fetched");
                None
            }
        };

        let mut service = Self {
            status: ctx.publisher::<WeatherStatus>(),
            client,
            config,
            watched: Vec::new(),
            fix: None,
            generation: 0,
            places: Vec::new(),
            failure: None,
        };
        service.report(ctx);
        service.publish();
        Ok(service)
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Located(_)) if !self.wants_here() => {}
            Input::Event(Event::Fetched { generation, .. }) if generation != self.generation => {}

            Input::Event(Event::Located(coordinates)) => {
                if self.moved(coordinates.as_ref()) {
                    self.fix = coordinates;
                    self.generation += 1;
                    self.places
                        .retain(|shown| shown.place != WatchedPlace::Here);
                    self.report(ctx);
                    self.publish();
                }
            }

            Input::Event(Event::Fetched { result, .. }) => {
                match result {
                    Ok(readings) if readings.len() == self.targets().len() => {
                        self.failure = None;
                        self.absorb(&readings);
                    }
                    Ok(_) => {
                        self.failure =
                            Some("the provider answered for a different set of places".to_owned());
                    }
                    Err(reason) => self.failure = Some(cap(&reason, REASON)),
                }
                if self.sweep(Instant::now()) {
                    self.generation += 1;
                }
                self.forget_unwatched();
                self.report(ctx);
                self.publish();
            }

            Input::Config(config) => {
                if config.units != self.config.units {
                    self.places.clear();
                }
                self.config = config;
                self.generation += 1;
                self.report(ctx);
                self.publish();
            }

            Input::Command(Command::Watch(place), responder) => {
                let now = Instant::now();
                let asked = self.ask().coordinates;
                let mut changed = self.sweep(now);

                let outcome = self.lease(place, now);
                changed |= matches!(outcome, Ok(true));

                if changed {
                    if self.ask().coordinates != asked {
                        self.generation += 1;
                    }
                    self.forget_unwatched();
                    self.report(ctx);
                    self.publish();
                }

                match outcome {
                    Ok(_) => responder.ok(()),
                    Err(error) => responder.fail(error),
                }
            }

            Input::Command(Command::Refresh, responder) => {
                self.generation += 1;
                responder.ok(());
            }
        }
    }
}

impl Weather {
    fn wants_here(&self) -> bool {
        self.watched
            .iter()
            .any(|lease| lease.place == WatchedPlace::Here)
    }

    /// The comparison is against the last *accepted* fix, so drift that never clears the threshold
    /// never accumulates into a refetch.
    fn moved(&self, coordinates: Option<&GeoCoordinates>) -> bool {
        match (self.fix.as_ref(), coordinates) {
            (Some(old), Some(new)) => metres(old, new) >= MOVED,
            (old, new) => old.is_some() != new.is_some(),
        }
    }

    fn targets(&self) -> Vec<(WatchedPlace, GeoCoordinates)> {
        self.watched
            .iter()
            .filter_map(|lease| match &lease.place {
                WatchedPlace::Here => Some((WatchedPlace::Here, self.fix.clone()?)),
                WatchedPlace::Coordinates {
                    latitude,
                    longitude,
                } => Some((
                    lease.place.clone(),
                    GeoCoordinates {
                        latitude: *latitude,
                        longitude: *longitude,
                    },
                )),
            })
            .collect()
    }

    fn ask(&self) -> Ask {
        Ask {
            coordinates: self
                .targets()
                .into_iter()
                .map(|(_, coordinates)| coordinates)
                .collect(),
            units: self.config.units,
            forecast_days: self.config.forecast_days,
        }
    }

    /// `Ok(true)` when the set of places actually grew; a renewal answers `Ok(false)` so it cannot
    /// restart the poll, which would turn a fifteen-minute interval into whatever the renewal
    /// cadence happens to be.
    fn lease(&mut self, place: WatchedPlace, now: Instant) -> Result<bool, CallError> {
        let until = now + LEASE;

        if let Some(held) = self.watched.iter_mut().find(|held| held.place == place) {
            held.until = until;
            return Ok(false);
        }

        if self.watched.len() >= MOST_WATCHED {
            return Err(CallError::new(
                ErrorCode::LimitExceeded,
                format!("no more than {MOST_WATCHED} places are reported at once"),
            ));
        }

        self.watched.push(Lease { place, until });
        Ok(true)
    }

    fn sweep(&mut self, now: Instant) -> bool {
        let before = self.watched.len();
        self.watched.retain(|lease| lease.until > now);
        self.watched.len() != before
    }

    fn forget_unwatched(&mut self) {
        self.places
            .retain(|shown| self.watched.iter().any(|lease| lease.place == shown.place));
    }

    /// The caller has already checked that the provider answered for exactly the places asked
    /// about, which is what makes pairing them by position safe.
    fn absorb(&mut self, readings: &[Reading]) {
        self.places = self
            .targets()
            .into_iter()
            .zip(readings)
            .map(|((place, coordinates), reading)| PlaceWeather {
                place,
                coordinates,
                utc_offset_seconds: reading.utc_offset_seconds,
                current: reading.current.clone(),
                hours: reading.hours.clone(),
                days: reading.days.clone(),
            })
            .collect();
    }

    fn report(&self, ctx: &Ctx<Self>) {
        let mut reasons = Vec::new();

        if let Some(failure) = &self.failure {
            reasons.push(failure.clone());
        }
        if self.watched.is_empty() {
            reasons.push("nothing is being watched".to_owned());
        } else if self.wants_here() && self.fix.is_none() {
            reasons.push("there is no location fix yet".to_owned());
        }

        match reasons.is_empty() {
            true => ctx.running(),
            false => ctx.degraded(cap(&reasons.join("; "), REASON)),
        }
    }

    fn publish(&mut self) {
        self.status.set(WeatherStatus {
            units: self.config.units,
            places: self.places.clone(),
        });
    }
}

/// Presence is the wire format's job. What is left is range: a mistyped latitude is a mistake worth
/// refusing at the call rather than a request for somewhere that is not on Earth.
fn placed(place: WatchedPlace) -> Result<WatchedPlace, CallError> {
    match place {
        WatchedPlace::Here => Ok(place),
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        } if (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) => {
            Ok(place)
        }
        WatchedPlace::Coordinates { .. } => Err(CallError::new(
            ErrorCode::InvalidArgs,
            "those coordinates are not on Earth",
        )),
    }
}

fn metres(from: &GeoCoordinates, to: &GeoCoordinates) -> f64 {
    let (lat1, lat2) = (from.latitude.to_radians(), to.latitude.to_radians());
    let dlat = (to.latitude - from.latitude).to_radians() / 2.0;
    let dlon = (to.longitude - from.longitude).to_radians() / 2.0;
    let inner = dlat.sin().powi(2) + lat1.cos() * lat2.cos() * dlon.sin().powi(2);
    2.0 * EARTH * inner.sqrt().clamp(0.0, 1.0).asin()
}

fn cap(text: &str, most: usize) -> String {
    match text.chars().count() > most {
        true => text.chars().take(most).collect(),
        false => text.to_owned(),
    }
}

fn units(system: UnitSystem) -> (&'static str, &'static str, &'static str) {
    match system {
        UnitSystem::Metric => ("celsius", "kmh", "mm"),
        UnitSystem::Imperial => ("fahrenheit", "mph", "inch"),
    }
}

fn joined(coordinates: &[GeoCoordinates], pick: fn(&GeoCoordinates) -> f64) -> String {
    coordinates
        .iter()
        .map(|pair| pick(pair).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// A transport failure must not carry the request: the query string holds the user's latitude and
/// longitude, so a logged URL is a location leak.
fn transport(error: reqwest::Error) -> String {
    match error.is_timeout() {
        true => "the request timed out".to_owned(),
        false => error.without_url().to_string(),
    }
}

async fn open_meteo(client: reqwest::Client, ask: Ask) -> Result<Vec<Reading>, String> {
    let (temperature, wind, precipitation) = units(ask.units);
    let zones = vec!["auto"; ask.coordinates.len()].join(",");

    let url = reqwest::Url::parse_with_params(
        ENDPOINT,
        &[
            ("latitude", joined(&ask.coordinates, |pair| pair.latitude)),
            ("longitude", joined(&ask.coordinates, |pair| pair.longitude)),
            ("current", CURRENT.to_owned()),
            ("hourly", HOURLY.to_owned()),
            ("daily", DAILY.to_owned()),
            ("forecast_days", ask.forecast_days.to_string()),
            ("timezone", zones),
            ("timeformat", "unixtime".to_owned()),
            ("temperature_unit", temperature.to_owned()),
            ("wind_speed_unit", wind.to_owned()),
            ("precipitation_unit", precipitation.to_owned()),
        ],
    )
    .map_err(|_| "the request could not be built".to_owned())?;

    let response = client.get(url).send().await.map_err(transport)?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("the provider answered {status}"));
    }

    let body = response.text().await.map_err(transport)?;
    let payload: Payload = serde_json::from_str(&body)
        .map_err(|_| "the provider answered something else".to_owned())?;

    Ok(readings(payload.forecasts(), Utc::now()))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Payload {
    One(Box<Forecast>),
    Many(Vec<Forecast>),
}

impl Payload {
    fn forecasts(self) -> Vec<Forecast> {
        match self {
            Self::One(forecast) => vec![*forecast],
            Self::Many(forecasts) => forecasts,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Forecast {
    #[serde(default)]
    utc_offset_seconds: i32,
    current: Option<CurrentBlock>,
    hourly: Option<HourlyBlock>,
    daily: Option<DailyBlock>,
}

#[derive(Debug, Deserialize)]
struct CurrentBlock {
    time: i64,
    temperature_2m: Option<f64>,
    apparent_temperature: Option<f64>,
    relative_humidity_2m: Option<f64>,
    precipitation: Option<f64>,
    weather_code: Option<u32>,
    wind_speed_10m: Option<f64>,
    wind_direction_10m: Option<f64>,
    is_day: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
struct HourlyBlock {
    #[serde(default)]
    time: Vec<i64>,
    #[serde(default)]
    temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    weather_code: Vec<Option<u32>>,
    #[serde(default)]
    is_day: Vec<Option<u8>>,
}

#[derive(Debug, Default, Deserialize)]
struct DailyBlock {
    #[serde(default)]
    time: Vec<i64>,
    #[serde(default)]
    weather_code: Vec<Option<u32>>,
    #[serde(default)]
    temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<u8>>,
    #[serde(default)]
    sunrise: Vec<Option<i64>>,
    #[serde(default)]
    sunset: Vec<Option<i64>>,
}

fn moment(epoch: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(epoch, 0).single()
}

fn readings(forecasts: Vec<Forecast>, now: DateTime<Utc>) -> Vec<Reading> {
    forecasts
        .into_iter()
        .map(|forecast| Reading {
            utc_offset_seconds: forecast.utc_offset_seconds,
            current: forecast.current.and_then(current),
            hours: hours(forecast.hourly.unwrap_or_default(), now),
            days: days(forecast.daily.unwrap_or_default()),
        })
        .collect()
}

fn current(block: CurrentBlock) -> Option<CurrentWeather> {
    Some(CurrentWeather {
        observed_at: moment(block.time)?,
        condition: condition(block.weather_code),
        is_day: block.is_day != Some(0),
        temperature: block.temperature_2m?,
        apparent_temperature: block.apparent_temperature,
        humidity: block
            .relative_humidity_2m
            .map(|reading| reading.round().clamp(0.0, 100.0) as u8),
        wind_speed: block.wind_speed_10m,
        wind_direction: block
            .wind_direction_10m
            .map(|reading| reading.round().rem_euclid(360.0) as u16),
        precipitation: block.precipitation,
    })
}

/// The strip labels its first column "now", so the window opens at the hour standing rather than
/// the one after it.
fn hours(block: HourlyBlock, now: DateTime<Utc>) -> Vec<HourForecast> {
    let epoch = now.timestamp();
    let floor = epoch - epoch.rem_euclid(3600);

    block
        .time
        .iter()
        .enumerate()
        .filter(|(_, at)| **at >= floor)
        .filter_map(|(index, at)| {
            Some(HourForecast {
                time: moment(*at)?,
                condition: condition(block.weather_code.get(index).copied().flatten()),
                is_day: block.is_day.get(index).copied().flatten() != Some(0),
                temperature: (*block.temperature_2m.get(index)?)?,
            })
        })
        .take(HOURS)
        .collect()
}

fn days(block: DailyBlock) -> Vec<DayForecast> {
    block
        .time
        .iter()
        .enumerate()
        .filter_map(|(index, at)| {
            Some(DayForecast {
                start: moment(*at)?,
                condition: condition(block.weather_code.get(index).copied().flatten()),
                low: (*block.temperature_2m_min.get(index)?)?,
                high: (*block.temperature_2m_max.get(index)?)?,
                precipitation_chance: block
                    .precipitation_probability_max
                    .get(index)
                    .copied()
                    .flatten(),
                sunrise: block.sunrise.get(index).copied().flatten().and_then(moment),
                sunset: block.sunset.get(index).copied().flatten().and_then(moment),
            })
        })
        .collect()
}

/// WMO 4677, as Open-Meteo reports it. A provider that speaks another vocabulary maps into the same
/// closed set rather than translating into this one.
fn condition(code: Option<u32>) -> Condition {
    match code {
        Some(0) => Condition::ClearSky,
        Some(1) => Condition::MainlyClear,
        Some(2) => Condition::PartlyCloudy,
        Some(3) => Condition::Overcast,
        Some(45 | 48) => Condition::Fog,
        Some(51 | 53 | 55) => Condition::Drizzle,
        Some(56 | 57) => Condition::FreezingDrizzle,
        Some(61) => Condition::LightRain,
        Some(63) => Condition::Rain,
        Some(65) => Condition::HeavyRain,
        Some(66 | 67) => Condition::FreezingRain,
        Some(71) => Condition::LightSnow,
        Some(73) => Condition::Snow,
        Some(75) => Condition::HeavySnow,
        Some(77) => Condition::SnowGrains,
        Some(80..=82) => Condition::RainShowers,
        Some(85 | 86) => Condition::SnowShowers,
        Some(95) => Condition::Thunderstorm,
        Some(96 | 99) => Condition::ThunderstormWithHail,
        _ => Condition::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glimpse_dbus::Buses;
    use glimpse_ipc::CallError;
    use tokio::sync::{mpsc, oneshot};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{BrokerHandle, MockBroker, Responder, ServiceState};

    struct Harness {
        service: Weather,
        ctx: Ctx<Weather>,
        broker: Arc<MockBroker>,
        _inbox: mpsc::Receiver<Input<Weather>>,
        _cancel: CancellationToken,
    }

    async fn harness() -> Harness {
        harness_with(Config {
            provider: Provider::OpenMeteo,
            units: UnitSystem::Metric,
            poll_interval: 900,
            forecast_days: 7,
        })
        .await
    }

    async fn harness_with(config: Config) -> Harness {
        let (events, inbox) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let broker = Arc::new(MockBroker::default());
        let handle: Arc<dyn BrokerHandle> = broker.clone();
        let ctx = Ctx::<Weather>::new(
            events,
            &cancel,
            handle,
            Buses::unavailable("no bus in tests"),
        );
        let service = Weather::start(&ctx, config)
            .await
            .expect("the service starts");

        Harness {
            service,
            ctx,
            broker,
            _inbox: inbox,
            _cancel: cancel,
        }
    }

    async fn call(harness: &mut Harness, command: Command) -> Result<Value, CallError> {
        let (reply, answer) = oneshot::channel();
        harness
            .service
            .handle(&harness.ctx, Input::Command(command, Responder::new(reply)))
            .await;
        answer.await.expect("the responder answers")
    }

    fn at(latitude: f64, longitude: f64) -> WatchedPlace {
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        }
    }

    fn fix(latitude: f64, longitude: f64) -> GeoCoordinates {
        GeoCoordinates {
            latitude,
            longitude,
        }
    }

    fn reason(broker: &MockBroker) -> Option<String> {
        broker
            .health()
            .into_iter()
            .last()
            .and_then(|(_, state)| match state {
                ServiceState::Degraded { reason } => Some(reason),
                _ => None,
            })
    }

    fn document(weather: glimpse_config::WeatherConfig) -> glimpse_config::Config {
        glimpse_config::Config {
            weather,
            ..Default::default()
        }
    }

    #[test]
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Weather>();
    }

    #[test]
    fn decode_answers_both_methods_and_refuses_the_rest() {
        assert!(matches!(
            Weather::decode(WeatherRefresh::NAME, serde_json::json!({})),
            Ok(Command::Refresh)
        ));
        assert!(matches!(
            Weather::decode(
                WeatherWatch::NAME,
                serde_json::json!({ "place": { "at": "here" } })
            ),
            Ok(Command::Watch(WatchedPlace::Here))
        ));
        Weather::decode("weather.forget", serde_json::json!({}))
            .expect_err("`weather.forget` is not a command this service answers");
    }

    /// `Duration::from_secs(0)` panics `tokio::time::interval`, so the floor is not a preference.
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

    /// The privacy property the lease design exists to give: until something asks, no request is
    /// made, so no coordinate leaves the machine. It says nothing about GeoClue, which the
    /// `geolocation` service runs on its own account.
    #[tokio::test]
    async fn nothing_watched_means_no_request_and_no_location_subscription() {
        let harness = harness().await;

        assert!(harness.service.subscriptions().is_empty());
        assert!(harness.service.ask().coordinates.is_empty());
        assert!(!harness.service.wants_here());
        assert_eq!(
            reason(&harness.broker).as_deref(),
            Some("nothing is being watched")
        );
    }

    #[tokio::test]
    async fn the_location_topic_is_subscribed_only_while_something_watches_here() {
        let mut harness = harness().await;

        call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
            .await
            .expect("coordinates are watchable");
        assert!(!harness.service.wants_here());
        assert_eq!(harness.service.subscriptions().len(), 1);

        call(&mut harness, Command::Watch(WatchedPlace::Here))
            .await
            .expect("here is watchable");
        assert!(harness.service.wants_here());
        assert_eq!(harness.service.subscriptions().len(), 2);
    }

    /// A `Here` lease with no fix contributes no coordinate, so nothing is asked for and the
    /// service says which half is missing.
    #[tokio::test]
    async fn here_without_a_fix_asks_for_nothing_and_says_so() {
        let mut harness = harness().await;

        call(&mut harness, Command::Watch(WatchedPlace::Here))
            .await
            .expect("here is watchable");

        assert!(harness.service.ask().coordinates.is_empty());
        assert_eq!(
            reason(&harness.broker).as_deref(),
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

    /// A consumer renews on its own tick. If a renewal restarted the poll — `ctx.interval` starts
    /// at `Instant::now()` — the service would fetch at the renewal cadence instead of its own.
    #[tokio::test]
    async fn renewing_a_lease_does_not_restart_the_poll() {
        let mut harness = harness().await;

        call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
            .await
            .expect("the first watch");
        let after_first = harness.service.generation;

        for _ in 0..5 {
            call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
                .await
                .expect("a renewal");
        }

        assert_eq!(harness.service.generation, after_first);
    }

    /// Refusing a renewal at the cap would expire the very leases holding it full.
    #[tokio::test]
    async fn watching_past_the_cap_is_refused_but_a_renewal_at_the_cap_still_succeeds() {
        let mut harness = harness().await;

        for index in 0..MOST_WATCHED {
            call(&mut harness, Command::Watch(at(index as f64, 0.0)))
                .await
                .expect("within the cap");
        }

        let refused = call(&mut harness, Command::Watch(at(60.0, 0.0)))
            .await
            .expect_err("past the cap");
        assert_eq!(refused.code, ErrorCode::LimitExceeded);

        call(&mut harness, Command::Watch(at(0.0, 0.0)))
            .await
            .expect("a renewal at the cap");
        assert_eq!(harness.service.watched.len(), MOST_WATCHED);
    }

    /// Both no-fetch states are reachable — no http client, and a lone `here` lease with no fix —
    /// and in either one a sweep that only ran on `Fetched` would never run at all. The cap then
    /// counts dead registrations and refuses every later watch.
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

        call(&mut harness, Command::Watch(at(60.0, 0.0)))
            .await
            .expect("the cap counts live leases rather than dead ones");

        assert_eq!(harness.service.watched.len(), 1);
    }

    /// A `here` lease with no fix resolves to no coordinate, so it changes the lease set without
    /// changing the request. Bumping the generation for it would refetch the same places.
    #[tokio::test]
    async fn watching_here_without_a_fix_does_not_restart_the_poll() {
        let mut harness = harness().await;

        call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
            .await
            .expect("the first watch");
        let after_first = harness.service.generation;

        call(&mut harness, Command::Watch(WatchedPlace::Here))
            .await
            .expect("here is watchable");

        assert!(harness.service.wants_here());
        assert_eq!(harness.service.generation, after_first);
    }

    fn watch_args(latitude: f64, longitude: f64) -> Value {
        serde_json::json!({
            "place": { "at": "coordinates", "latitude": latitude, "longitude": longitude }
        })
    }

    /// The pairs are written literally rather than built through `placed`, which is the function
    /// under test: a helper that called it would compare its output against itself.
    #[test]
    fn a_watch_on_coordinates_outside_their_ranges_is_refused() {
        for (latitude, longitude) in [(91.0, 0.0), (-91.0, 0.0), (0.0, 181.0), (0.0, -181.0)] {
            let refused = Weather::decode(WeatherWatch::NAME, watch_args(latitude, longitude))
                .expect_err("not on Earth");

            assert_eq!(refused.code, ErrorCode::InvalidArgs);
        }

        for (latitude, longitude) in [(90.0, 180.0), (-90.0, -180.0), (0.0, 0.0)] {
            Weather::decode(WeatherWatch::NAME, watch_args(latitude, longitude))
                .expect("the edges are on Earth");
        }
    }

    /// Without the threshold a fix that jitters by metres bumps the generation on every update and
    /// refetches for ever.
    #[tokio::test]
    async fn a_fix_that_moves_less_than_a_kilometre_does_not_refetch() {
        let mut harness = harness().await;
        call(&mut harness, Command::Watch(WatchedPlace::Here))
            .await
            .expect("here is watchable");

        let settled = harness.service.generation;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(47.3769, 8.5417)))),
            )
            .await;
        let first = harness.service.generation;
        assert!(first > settled, "the first fix is a change");

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(47.3772, 8.5419)))),
            )
            .await;
        assert_eq!(harness.service.generation, first, "300 m is not a move");

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(47.3900, 8.5417)))),
            )
            .await;
        assert!(harness.service.generation > first, "1.5 km is a move");
    }

    /// Each step is far below the threshold, so an implementation comparing against the fix it last
    /// *saw* would never move at all. Comparing against the one it last *accepted* crosses once.
    #[tokio::test]
    async fn drift_is_measured_from_the_fix_that_was_accepted() {
        let mut harness = harness().await;
        call(&mut harness, Command::Watch(WatchedPlace::Here))
            .await
            .expect("here is watchable");

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(47.0, 8.0)))),
            )
            .await;
        let settled = harness.service.generation;

        for step in 1..=4 {
            harness
                .service
                .handle(
                    &harness.ctx,
                    Input::Event(Event::Located(Some(fix(
                        47.0 + 0.002 * f64::from(step),
                        8.0,
                    )))),
                )
                .await;
        }

        assert_eq!(
            harness.service.generation, settled,
            "four steps of about 222 m stay inside the threshold"
        );
        assert_eq!(harness.service.fix, Some(fix(47.0, 8.0)));

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(47.01, 8.0)))),
            )
            .await;

        assert_eq!(
            harness.service.generation,
            settled + 1,
            "the walk crosses the threshold measured from the accepted fix"
        );
        assert_eq!(harness.service.fix, Some(fix(47.01, 8.0)));
    }

    #[test]
    fn a_kilometre_is_a_kilometre() {
        let north = metres(&fix(0.0, 0.0), &fix(0.009, 0.0));
        assert!(
            (north - 1000.0).abs() < 5.0,
            "0.009 degrees of latitude is about a kilometre, got {north}"
        );
        assert_eq!(metres(&fix(51.5, -0.1), &fix(51.5, -0.1)), 0.0);
    }

    /// Every number held is in the old system. Relabelling them would put a wrong number on screen
    /// for the one round trip a correct one takes to arrive; clearing them shows a missing one.
    #[tokio::test]
    async fn changing_units_clears_the_readings_rather_than_relabelling_them() {
        let mut harness = harness().await;
        call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a place to hold a reading");
        harness.service.places = vec![PlaceWeather {
            place: at(47.3769, 8.5417),
            coordinates: fix(47.3769, 8.5417),
            utc_offset_seconds: 7200,
            current: None,
            hours: Vec::new(),
            days: Vec::new(),
        }];

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Config(Config {
                    units: UnitSystem::Imperial,
                    ..harness.service.config.clone()
                }),
            )
            .await;

        assert!(harness.service.places.is_empty());
    }

    #[tokio::test]
    async fn a_poll_interval_change_keeps_the_readings() {
        let mut harness = harness().await;
        harness.service.places = vec![PlaceWeather {
            place: WatchedPlace::Here,
            coordinates: fix(47.3769, 8.5417),
            utc_offset_seconds: 7200,
            current: None,
            hours: Vec::new(),
            days: Vec::new(),
        }];

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Config(Config {
                    poll_interval: 1200,
                    ..harness.service.config.clone()
                }),
            )
            .await;

        assert_eq!(harness.service.places.len(), 1);
    }

    fn one_place() -> &'static str {
        r#"{
          "latitude": 47.375, "longitude": 8.5, "utc_offset_seconds": 7200,
          "current": {"time": 1757260800, "temperature_2m": 18.4, "apparent_temperature": 17.1,
                      "relative_humidity_2m": 62.0, "precipitation": 0.2, "weather_code": 61,
                      "wind_speed_10m": 14.0, "wind_direction_10m": 315.0, "is_day": 1},
          "hourly": {"time": [1757257200, 1757260800, 1757264400],
                     "temperature_2m": [17.0, 18.4, null],
                     "weather_code": [3, 61, 0],
                     "is_day": [1, 1, 0]},
          "daily": {"time": [1757196000], "weather_code": [61],
                    "temperature_2m_max": [21.0], "temperature_2m_min": [12.0],
                    "precipitation_probability_max": [40],
                    "sunrise": [1757218860], "sunset": [1757267640]}
        }"#
    }

    fn many_places() -> String {
        format!("[{}, {}]", one_place(), one_place())
    }

    fn decoded(text: &str) -> Vec<Forecast> {
        serde_json::from_str::<Payload>(text)
            .expect("the provider's shape decodes")
            .forecasts()
    }

    /// One location returns an object and several return an array. A single watch is the common
    /// case, so the untagged enum is what makes the ordinary configuration work at all.
    #[test]
    fn one_place_and_several_places_both_decode() {
        assert_eq!(decoded(one_place()).len(), 1);
        assert_eq!(decoded(&many_places()).len(), 2);
    }

    fn moment_of(epoch: i64) -> DateTime<Utc> {
        moment(epoch).expect("a representable instant")
    }

    /// The strip labels its first column "now". Legacy started at the hour *after* the current one
    /// by string-comparing timestamps, and lost it.
    #[test]
    fn the_hours_published_start_at_the_current_hour() {
        let read = readings(decoded(one_place()), moment_of(1757262000));

        let hours = &read[0].hours;
        assert_eq!(hours[0].time, moment_of(1757260800));
        assert_eq!(hours[0].temperature, 18.4);
        assert!(hours[0].is_day);
    }

    #[test]
    fn an_hour_the_provider_left_null_is_skipped_rather_than_zeroed() {
        let read = readings(decoded(one_place()), moment_of(1757262000));

        let times: Vec<i64> = read[0]
            .hours
            .iter()
            .map(|hour| hour.time.timestamp())
            .collect();
        assert_eq!(times, vec![1757260800]);
    }

    #[test]
    fn an_hour_before_now_is_not_published() {
        let read = readings(decoded(one_place()), moment_of(1757264400));

        assert!(
            read[0]
                .hours
                .iter()
                .all(|hour| hour.time.timestamp() >= 1757264400)
        );
    }

    #[test]
    fn a_day_carries_its_span_its_chance_and_its_sun() {
        let read = readings(decoded(one_place()), moment_of(1757262000));

        let day = &read[0].days[0];
        assert_eq!(day.low, 12.0);
        assert_eq!(day.high, 21.0);
        assert_eq!(day.precipitation_chance, Some(40));
        assert_eq!(day.sunrise, Some(moment_of(1757218860)));
        assert_eq!(day.sunset, Some(moment_of(1757267640)));
        assert_eq!(day.condition, Condition::LightRain);
    }

    #[test]
    fn the_current_block_carries_the_provider_s_own_observation_time() {
        let read = readings(decoded(one_place()), moment_of(1757262000));

        let current = read[0].current.as_ref().expect("a current block");
        assert_eq!(current.observed_at, moment_of(1757260800));
        assert_eq!(current.condition, Condition::LightRain);
        assert_eq!(current.humidity, Some(62));
        assert_eq!(current.wind_direction, Some(315));
        assert!(current.is_day);
    }

    /// A reading the provider did not send must not arrive as a plausible zero: 0% humidity and
    /// calm air are both real values, so `None` is the only honest way to say nothing came back.
    #[test]
    fn a_reading_the_provider_omitted_stays_absent_rather_than_becoming_zero() {
        let sparse = r#"{"latitude": 47.375, "longitude": 8.5, "utc_offset_seconds": 0,
                         "current": {"time": 1757260800, "temperature_2m": 18.4,
                                     "weather_code": 0, "is_day": 1}}"#;

        let read = readings(decoded(sparse), moment_of(1757262000));
        let current = read[0].current.as_ref().expect("a current block");

        assert_eq!(current.temperature, 18.4);
        assert_eq!(current.humidity, None);
        assert_eq!(current.wind_speed, None);
        assert_eq!(current.wind_direction, None);
        assert_eq!(current.precipitation, None);
        assert_eq!(current.apparent_temperature, None);
    }

    /// The provider really does answer 0 for some grid cells, so a zero has to survive as a zero.
    #[test]
    fn a_reading_the_provider_reports_as_zero_stays_zero() {
        let calm = r#"{"latitude": 52.23, "longitude": 21.01, "utc_offset_seconds": 0,
                       "current": {"time": 1757260800, "temperature_2m": 19.6,
                                   "relative_humidity_2m": 0, "wind_speed_10m": 0.0,
                                   "precipitation": 0.0, "weather_code": 1, "is_day": 1}}"#;

        let read = readings(decoded(calm), moment_of(1757262000));
        let current = read[0].current.as_ref().expect("a current block");

        assert_eq!(current.humidity, Some(0));
        assert_eq!(current.wind_speed, Some(0.0));
        assert_eq!(current.precipitation, Some(0.0));
    }

    #[test]
    fn every_documented_wmo_code_maps_and_an_unrecognized_one_is_unknown() {
        for (code, expected) in [
            (0, Condition::ClearSky),
            (1, Condition::MainlyClear),
            (2, Condition::PartlyCloudy),
            (3, Condition::Overcast),
            (45, Condition::Fog),
            (48, Condition::Fog),
            (51, Condition::Drizzle),
            (56, Condition::FreezingDrizzle),
            (61, Condition::LightRain),
            (63, Condition::Rain),
            (65, Condition::HeavyRain),
            (66, Condition::FreezingRain),
            (71, Condition::LightSnow),
            (73, Condition::Snow),
            (75, Condition::HeavySnow),
            (77, Condition::SnowGrains),
            (80, Condition::RainShowers),
            (85, Condition::SnowShowers),
            (95, Condition::Thunderstorm),
            (96, Condition::ThunderstormWithHail),
        ] {
            assert_eq!(condition(Some(code)), expected, "for code {code}");
        }

        assert_eq!(condition(Some(1234)), Condition::Unknown);
        assert_eq!(condition(None), Condition::Unknown);
    }

    /// A daemon that learns a new condition must not make an older reader fail to decode the whole
    /// payload. Adding a field is already safe; adding a variant is only safe because of `other`.
    #[test]
    fn an_unknown_condition_variant_decodes_rather_than_failing_the_payload() {
        let decoded: Condition = serde_json::from_str(r#"{"condition":"volcanic_ash"}"#)
            .expect("an unrecognized condition still decodes");

        assert_eq!(decoded, Condition::Unknown);
    }

    #[test]
    fn units_choose_the_triple_that_goes_together() {
        assert_eq!(units(UnitSystem::Metric), ("celsius", "kmh", "mm"));
        assert_eq!(units(UnitSystem::Imperial), ("fahrenheit", "mph", "inch"));
    }

    fn a_reading() -> Reading {
        Reading {
            utc_offset_seconds: 7200,
            current: None,
            hours: Vec::new(),
            days: Vec::new(),
        }
    }

    /// Legacy threw the snapshot away and blanked the bar. A degraded service is a running one, so
    /// the last good numbers stay published and staleness is read off `observed_at`.
    #[tokio::test]
    async fn a_failed_fetch_keeps_the_last_reading_and_degrades() {
        let mut harness = harness().await;
        call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a place to hold a reading");

        let generation = harness.service.generation;
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation,
                    result: Ok(vec![a_reading()]),
                }),
            )
            .await;
        assert_eq!(harness.service.places.len(), 1);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Err("the request timed out".to_owned()),
                }),
            )
            .await;

        assert_eq!(harness.service.places.len(), 1, "the last reading is kept");
        assert_eq!(
            reason(&harness.broker).as_deref(),
            Some("the request timed out")
        );
    }

    /// The query string carries the user's latitude and longitude, so a reason that quoted the
    /// request would be a location leak wherever it was pasted.
    #[tokio::test]
    async fn a_failure_reason_names_neither_the_host_nor_a_coordinate() {
        let mut harness = harness().await;
        call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a watched place");

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Err("the provider answered 500 Internal Server Error".to_owned()),
                }),
            )
            .await;

        let rendered = reason(&harness.broker).expect("a degraded reason");
        for secret in ["open-meteo", "47.3769", "8.5417", "latitude"] {
            assert!(
                !rendered.contains(secret),
                "the reason must not name `{secret}`, got {rendered}"
            );
        }
    }

    /// The closure captured the request as it stood; a result built from an older one must not
    /// overwrite the current places.
    #[tokio::test]
    async fn a_fetch_from_an_earlier_generation_is_dropped() {
        let mut harness = harness().await;
        call(&mut harness, Command::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a watched place");

        let stale = harness.service.generation;
        call(&mut harness, Command::Refresh)
            .await
            .expect("refresh answers");
        assert!(harness.service.generation > stale);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: stale,
                    result: Ok(vec![a_reading()]),
                }),
            )
            .await;

        assert!(
            harness.service.places.is_empty(),
            "a stale fetch must not be absorbed"
        );
    }

    /// A fix can be queued behind the change that stopped anyone watching `here`. Dropping the
    /// subscription does not un-queue it, so the guard is on the model.
    #[tokio::test]
    async fn a_geolocation_update_arriving_after_the_last_here_lease_expired_is_ignored() {
        let mut harness = harness().await;
        let now = Instant::now();
        harness
            .service
            .lease(WatchedPlace::Here, now)
            .expect("the lease");
        assert!(harness.service.sweep(now + LEASE + Duration::from_secs(1)));

        let settled = harness.service.generation;
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(47.3769, 8.5417)))),
            )
            .await;

        assert_eq!(harness.service.generation, settled);
        assert_eq!(harness.service.fix, None);
    }

    /// Places are paired with readings by position, so a response of the wrong length is not a
    /// partial answer — it would hand one place another place's weather.
    #[tokio::test]
    async fn a_response_that_does_not_cover_every_place_is_refused() {
        let mut harness = harness().await;
        for pair in [(47.3769, 8.5417), (-33.8688, 151.2093)] {
            call(&mut harness, Command::Watch(at(pair.0, pair.1)))
                .await
                .expect("a watched place");
        }

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(vec![a_reading()]),
                }),
            )
            .await;

        assert!(harness.service.places.is_empty());
        assert_eq!(
            reason(&harness.broker).as_deref(),
            Some("the provider answered for a different set of places")
        );
    }

    /// The sweep runs after the readings are absorbed, so without the retain a place whose lease
    /// had just expired would stay on screen for a whole poll.
    #[tokio::test]
    async fn a_place_whose_lease_expired_leaves_the_published_list() {
        let mut harness = harness().await;
        let now = Instant::now();
        harness
            .service
            .lease(at(47.3769, 8.5417), now)
            .expect("the lease");

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(vec![a_reading()]),
                }),
            )
            .await;
        assert_eq!(harness.service.places.len(), 1);

        harness.service.watched[0].until = now;
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(Vec::new()),
                }),
            )
            .await;

        assert!(harness.service.watched.is_empty());
        assert!(harness.service.places.is_empty());
    }

    /// Only `here` followed the fix. Clearing every reading would blank a place the user asked for
    /// by coordinate — whose numbers are still correct — for a whole round trip.
    #[tokio::test]
    async fn a_fix_that_moved_drops_only_the_place_that_followed_it() {
        let mut harness = harness().await;
        let now = Instant::now();

        harness
            .service
            .lease(at(47.3769, 8.5417), now)
            .expect("a place named by coordinate");
        harness
            .service
            .lease(WatchedPlace::Here, now)
            .expect("a place that follows the fix");
        harness.service.fix = Some(fix(51.5, -0.1));

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(vec![a_reading(), a_reading()]),
                }),
            )
            .await;
        assert_eq!(harness.service.places.len(), 2);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(52.0, -0.1)))),
            )
            .await;

        assert_eq!(
            harness
                .service
                .places
                .iter()
                .map(|shown| shown.place.clone())
                .collect::<Vec<_>>(),
            vec![at(47.3769, 8.5417)]
        );
    }

    /// Only a missing temperature is worth dropping an hour over. A missing code reads as unknown
    /// and a missing day flag as daylight; dropping the hour instead loses the whole strip when a
    /// provider stops sending one companion array.
    #[test]
    fn an_hour_missing_only_its_companions_is_still_published() {
        let sparse = r#"{"latitude": 47.375, "longitude": 8.5, "utc_offset_seconds": 0,
                         "hourly": {"time": [1757260800], "temperature_2m": [18.4]}}"#;

        let read = readings(decoded(sparse), moment_of(1757262000));

        let hour = &read[0].hours[0];
        assert_eq!(hour.temperature, 18.4);
        assert_eq!(hour.condition, Condition::Unknown);
        assert!(hour.is_day);
    }
}
