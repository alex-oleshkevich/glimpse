use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{
    DateTime, FixedOffset, NaiveDate, NaiveTime, Offset as _, TimeZone, Timelike as _, Utc,
};
use glimpse_config::{WeatherProvider as ConfiguredProvider, WeatherUnits as ConfiguredUnits};
use glimpse_contracts::{
    AlertSeverity, Command as _, Condition, CurrentWeather, DayForecast, GeoCoordinates,
    GeolocationStatus, HourForecast, Message, PlaceWeather, UnitSystem, WatchedPlace, WeatherAlert,
    WeatherRefresh, WeatherStatus, WeatherWatch,
};
use glimpse_ipc::{CallError, ErrorCode};
use glimpse_utils::clean;
use serde::Deserialize;
use serde_json::Value;
use tzf_rs::DefaultFinder;

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

/// An alert is the only prose in this payload that is not ours, so it is bounded by count as well
/// as by length: a feed answering with a thousand of them is as unbounded as one answering with a
/// megabyte of headline.
const MOST_ALERTS: usize = 4;
const HEADLINE: usize = 120;
const DESCRIPTION: usize = 600;
const SOURCE: usize = 60;

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
const DAILY: &str =
    "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenMeteo,
    MetNo,
}

impl Provider {
    async fn fetch(self, client: reqwest::Client, ask: Ask) -> Result<Vec<Reading>, String> {
        match self {
            Self::OpenMeteo => open_meteo(client, ask).await,
            Self::MetNo => met_no(client, ask).await,
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
                ConfiguredProvider::MetNo => Provider::MetNo,
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
    alerts: Vec<WeatherAlert>,
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
                    Err(reason) => self.failure = Some(clean(&reason, REASON)),
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
                days: sunlit(&reading.days, &coordinates, reading.utc_offset_seconds),
                coordinates,
                utc_offset_seconds: reading.utc_offset_seconds,
                current: reading.current.clone(),
                hours: reading.hours.clone(),
                alerts: sanitized(&reading.alerts),
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
            false => ctx.degraded(clean(&reasons.join("; "), REASON)),
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

/// Sun times are computed here rather than taken from a provider, for every provider. Open-Meteo
/// will send them and met.no cannot, so trusting the payload made the same fact arrive two ways and
/// disagree at the edges; `absorb` is the one path every reading takes into a payload, so a source
/// added later gets them without remembering to ask.
///
/// The date is the day in the *place's* own zone, which is what `start` already is.
fn sunlit(days: &[DayForecast], coordinates: &GeoCoordinates, seconds: i32) -> Vec<DayForecast> {
    let offset = FixedOffset::east_opt(seconds).unwrap_or_else(|| Utc.fix());

    days.iter()
        .map(|day| {
            let (sunrise, sunset) =
                crate::sun::events(coordinates, day.start.with_timezone(&offset).date_naive())
                    .unwrap_or((None, None));
            DayForecast {
                sunrise,
                sunset,
                ..day.clone()
            }
        })
        .collect()
}

/// The one gate between a national alert feed and a `Gtk.Label`. Every provider reaches the
/// payload through `absorb`, so a source that starts answering with alerts is sanitised without
/// having to remember to be.
fn sanitized(alerts: &[WeatherAlert]) -> Vec<WeatherAlert> {
    alerts
        .iter()
        .take(MOST_ALERTS)
        .map(|alert| WeatherAlert {
            severity: alert.severity,
            headline: clean(&alert.headline, HEADLINE),
            description: alert
                .description
                .as_deref()
                .map(|text| clean(text, DESCRIPTION)),
            source: alert.source.as_deref().map(|text| clean(text, SOURCE)),
            starts_at: alert.starts_at,
            expires_at: alert.expires_at,
        })
        .collect()
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
            alerts: Vec::new(),
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

/// The window opens at the hour standing rather than the one after it: a renderer that wants to
/// skip it can, and one that wants to read the current hour off the strip cannot get it back.
/// Every provider's hours are cut to the same window, so the rule lives here rather than in each.
fn hour_floor(now: DateTime<Utc>) -> i64 {
    let epoch = now.timestamp();
    epoch - epoch.rem_euclid(3600)
}

fn hours(block: HourlyBlock, now: DateTime<Utc>) -> Vec<HourForecast> {
    let floor = hour_floor(now);

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
                sunrise: None,
                sunset: None,
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

const MET_NO_FORECAST: &str = "https://api.met.no/weatherapi/locationforecast/2.0/complete";
const MET_NO_ALERTS: &str = "https://api.met.no/weatherapi/metalerts/2.0/current.json";

/// met.no's terms ask for coordinates truncated to four decimals, which is also what keeps their
/// cache from being keyed on noise.
const MET_NO_PRECISION: usize = 4;

async fn met_no(client: reqwest::Client, ask: Ask) -> Result<Vec<Reading>, String> {
    let now = Utc::now();
    let mut readings = Vec::with_capacity(ask.coordinates.len());

    for pair in &ask.coordinates {
        let forecast: MetNoForecast = met_no_json(&client, MET_NO_FORECAST, pair).await?;
        let warnings: MetNoAlerts = met_no_json(&client, MET_NO_ALERTS, pair)
            .await
            .unwrap_or_default();
        readings.push(met_no_reading(&forecast, &warnings, pair, &ask, now));
    }

    Ok(readings)
}

/// A place is one request, because met.no takes a single pair rather than Open-Meteo's parallel
/// lists. Alerts are a second request, and a failure there is not a failure of the forecast: the
/// numbers are still true, so it degrades to no warnings rather than to no weather.
async fn met_no_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    endpoint: &str,
    pair: &GeoCoordinates,
) -> Result<T, String> {
    let url = reqwest::Url::parse_with_params(
        endpoint,
        &[
            ("lat", format!("{:.*}", MET_NO_PRECISION, pair.latitude)),
            ("lon", format!("{:.*}", MET_NO_PRECISION, pair.longitude)),
        ],
    )
    .map_err(|_| "the request could not be built".to_owned())?;

    let response = client.get(url).send().await.map_err(transport)?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("the provider answered {status}"));
    }

    let body = response.text().await.map_err(transport)?;
    serde_json::from_str(&body).map_err(|_| "the provider answered something else".to_owned())
}

#[derive(Debug, Default, Deserialize)]
struct MetNoForecast {
    #[serde(default)]
    properties: MetNoProperties,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoProperties {
    #[serde(default)]
    timeseries: Vec<MetNoEntry>,
}

#[derive(Debug, Deserialize)]
struct MetNoEntry {
    time: DateTime<Utc>,
    #[serde(default)]
    data: MetNoData,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoData {
    #[serde(default)]
    instant: MetNoInstant,
    next_1_hours: Option<MetNoPeriod>,
    next_6_hours: Option<MetNoPeriod>,
    next_12_hours: Option<MetNoPeriod>,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoInstant {
    #[serde(default)]
    details: MetNoDetails,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoDetails {
    air_temperature: Option<f64>,
    relative_humidity: Option<f64>,
    wind_speed: Option<f64>,
    wind_from_direction: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoPeriod {
    #[serde(default)]
    summary: MetNoSummary,
    #[serde(default)]
    details: MetNoPeriodDetails,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoSummary {
    symbol_code: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoPeriodDetails {
    precipitation_amount: Option<f64>,
    probability_of_precipitation: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoAlerts {
    #[serde(default)]
    features: Vec<MetNoAlertFeature>,
}

#[derive(Debug, Deserialize)]
struct MetNoAlertFeature {
    #[serde(default)]
    properties: MetNoAlertProperties,
    when: Option<MetNoWhen>,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoAlertProperties {
    title: Option<String>,
    description: Option<String>,
    event: Option<String>,
    severity: Option<String>,
    sender: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct MetNoWhen {
    #[serde(default)]
    interval: Vec<DateTime<Utc>>,
}

impl MetNoEntry {
    /// The nearest period the entry carries. An hour-by-hour entry has `next_1_hours`; the
    /// six-hourly tail past two days has only the wider ones.
    fn period(&self) -> Option<&MetNoPeriod> {
        self.data
            .next_1_hours
            .as_ref()
            .or(self.data.next_6_hours.as_ref())
            .or(self.data.next_12_hours.as_ref())
    }

    fn symbol(&self) -> Option<&str> {
        self.period()?.summary.symbol_code.as_deref()
    }
}

/// met.no answers in Celsius, metres per second and millimetres whatever is asked of it, so the
/// conversion happens here rather than in the query. Open-Meteo is asked in the units it should
/// answer in; this provider has no such parameter.
fn met_no_temperature(celsius: f64, units: UnitSystem) -> f64 {
    match units {
        UnitSystem::Metric => celsius,
        UnitSystem::Imperial => celsius * 9.0 / 5.0 + 32.0,
    }
}

fn met_no_wind(metres_per_second: f64, units: UnitSystem) -> f64 {
    match units {
        UnitSystem::Metric => metres_per_second * 3.6,
        UnitSystem::Imperial => metres_per_second * 2.236_936,
    }
}

fn met_no_depth(millimetres: f64, units: UnitSystem) -> f64 {
    match units {
        UnitSystem::Metric => millimetres,
        UnitSystem::Imperial => millimetres / 25.4,
    }
}

/// met.no's own vocabulary, mapped into the closed set the wire carries. The `_day`, `_night` and
/// `_polartwilight` suffixes say which artwork to use rather than which weather it is, so they are
/// split off before the match; `andthunder` is checked first because it is a suffix on the others.
fn met_no_condition(symbol: Option<&str>) -> Condition {
    let Some(symbol) = symbol else {
        return Condition::Unknown;
    };
    let base = symbol.split('_').next().unwrap_or(symbol);

    if base.ends_with("andthunder") {
        return Condition::Thunderstorm;
    }

    match base {
        "clearsky" => Condition::ClearSky,
        "fair" => Condition::MainlyClear,
        "partlycloudy" => Condition::PartlyCloudy,
        "cloudy" => Condition::Overcast,
        "fog" => Condition::Fog,
        "lightrain" | "lightrainshowers" => Condition::LightRain,
        "rain" | "rainshowers" => Condition::Rain,
        "heavyrain" | "heavyrainshowers" => Condition::HeavyRain,
        "lightsleet" | "sleet" | "heavysleet" | "lightsleetshowers" | "sleetshowers"
        | "heavysleetshowers" => Condition::Sleet,
        "lightsnow" | "lightsnowshowers" => Condition::LightSnow,
        "snow" | "snowshowers" => Condition::Snow,
        "heavysnow" | "heavysnowshowers" => Condition::HeavySnow,
        _ => Condition::Unknown,
    }
}

fn met_no_is_day(symbol: Option<&str>) -> bool {
    !symbol.is_some_and(|symbol| symbol.ends_with("_night"))
}

/// Looking a place's zone up from its coordinates is the whole reason `tzf-rs` is here: met.no
/// reports instants in UTC and says nothing about where they were measured, while Open-Meteo
/// answers `utc_offset_seconds` outright. Without it every hour label would read in the panel's
/// zone rather than the place's.
fn met_no_offset(pair: &GeoCoordinates, now: DateTime<Utc>) -> i32 {
    static FINDER: OnceLock<DefaultFinder> = OnceLock::new();

    let name = FINDER
        .get_or_init(DefaultFinder::new)
        .get_tz_name(pair.longitude, pair.latitude);

    name.parse::<chrono_tz::Tz>()
        .map(|zone| now.with_timezone(&zone).offset().fix().local_minus_utc())
        .unwrap_or(0)
}

fn met_no_reading(
    forecast: &MetNoForecast,
    warnings: &MetNoAlerts,
    pair: &GeoCoordinates,
    ask: &Ask,
    now: DateTime<Utc>,
) -> Reading {
    let seconds = met_no_offset(pair, now);
    let offset = FixedOffset::east_opt(seconds).unwrap_or_else(|| Utc.fix());
    let entries = &forecast.properties.timeseries;

    Reading {
        utc_offset_seconds: seconds,
        current: met_no_current(entries.first(), ask.units),
        hours: met_no_hours(entries, ask.units, now),
        days: met_no_days(entries, offset, ask.units, ask.forecast_days),
        alerts: met_no_warnings(warnings),
    }
}

fn met_no_current(entry: Option<&MetNoEntry>, units: UnitSystem) -> Option<CurrentWeather> {
    let entry = entry?;
    let details = &entry.data.instant.details;

    Some(CurrentWeather {
        observed_at: entry.time,
        condition: met_no_condition(entry.symbol()),
        is_day: met_no_is_day(entry.symbol()),
        temperature: met_no_temperature(details.air_temperature?, units),
        apparent_temperature: None,
        humidity: details
            .relative_humidity
            .map(|reading| reading.round().clamp(0.0, 100.0) as u8),
        wind_speed: details.wind_speed.map(|speed| met_no_wind(speed, units)),
        wind_direction: details
            .wind_from_direction
            .map(|reading| reading.round().rem_euclid(360.0) as u16),
        precipitation: entry
            .data
            .next_1_hours
            .as_ref()
            .and_then(|period| period.details.precipitation_amount)
            .map(|depth| met_no_depth(depth, units)),
    })
}

fn met_no_hours(
    entries: &[MetNoEntry],
    units: UnitSystem,
    now: DateTime<Utc>,
) -> Vec<HourForecast> {
    let floor = hour_floor(now);

    entries
        .iter()
        .filter(|entry| entry.time.timestamp() >= floor)
        .filter_map(|entry| {
            Some(HourForecast {
                time: entry.time,
                condition: met_no_condition(entry.symbol()),
                is_day: met_no_is_day(entry.symbol()),
                temperature: met_no_temperature(entry.data.instant.details.air_temperature?, units),
            })
        })
        .take(HOURS)
        .collect()
}

/// met.no publishes a timeseries and no daily block, so the days are aggregated here. Open-Meteo
/// does that server-side, which is the one place these two providers are asked to agree by
/// construction rather than by arithmetic.
fn met_no_days(
    entries: &[MetNoEntry],
    offset: FixedOffset,
    units: UnitSystem,
    cap: u8,
) -> Vec<DayForecast> {
    let mut grouped: BTreeMap<NaiveDate, Vec<&MetNoEntry>> = BTreeMap::new();
    for entry in entries {
        grouped
            .entry(entry.time.with_timezone(&offset).date_naive())
            .or_default()
            .push(entry);
    }

    grouped
        .into_iter()
        .take(cap as usize)
        .filter_map(|(day, entries)| {
            let temperatures: Vec<f64> = entries
                .iter()
                .filter_map(|entry| entry.data.instant.details.air_temperature)
                .collect();
            let low = temperatures.iter().copied().fold(f64::INFINITY, f64::min);
            let high = temperatures
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            if !low.is_finite() || !high.is_finite() {
                return None;
            }

            let start = offset
                .from_local_datetime(&day.and_time(NaiveTime::MIN))
                .single()?
                .with_timezone(&Utc);

            Some(DayForecast {
                start,
                condition: met_no_daily_condition(&entries, offset),
                low: met_no_temperature(low, units),
                high: met_no_temperature(high, units),
                precipitation_chance: entries
                    .iter()
                    .filter_map(|entry| entry.period()?.details.probability_of_precipitation)
                    .map(|chance| chance.round().clamp(0.0, 100.0) as u8)
                    .max(),
                sunrise: None,
                sunset: None,
            })
        })
        .collect()
}

/// A day is named by the weather in the middle of it. Taking the first entry would let the small
/// hours name a day nobody is awake for, and the last would name it after the night that follows.
fn met_no_daily_condition(entries: &[&MetNoEntry], offset: FixedOffset) -> Condition {
    let midday = entries
        .iter()
        .filter(|entry| entry.symbol().is_some())
        .min_by_key(|entry| {
            let hour = entry.time.with_timezone(&offset).hour() as i64;
            (hour - 12).abs()
        });

    met_no_condition(midday.and_then(|entry| entry.symbol()))
}

fn met_no_severity(severity: Option<&str>) -> AlertSeverity {
    match severity.map(str::to_ascii_lowercase).as_deref() {
        Some("minor") => AlertSeverity::Minor,
        Some("moderate") => AlertSeverity::Moderate,
        Some("severe") => AlertSeverity::Severe,
        Some("extreme") => AlertSeverity::Extreme,
        _ => AlertSeverity::Unknown,
    }
}

/// Every string here is third-party prose off a national feed. `absorb` runs the whole list through
/// `sanitized` before it reaches a payload, which is why nothing is capped or stripped at this end.
fn met_no_warnings(alerts: &MetNoAlerts) -> Vec<WeatherAlert> {
    alerts
        .features
        .iter()
        .filter_map(|feature| {
            let properties = &feature.properties;
            let headline = properties
                .title
                .as_deref()
                .or(properties.event.as_deref())
                .filter(|text| !text.trim().is_empty())?;
            let interval = feature
                .when
                .as_ref()
                .map(|when| when.interval.as_slice())
                .unwrap_or_default();

            Some(WeatherAlert {
                severity: met_no_severity(properties.severity.as_deref()),
                headline: headline.to_owned(),
                description: properties.description.clone(),
                source: properties.sender.clone(),
                starts_at: interval.first().copied(),
                expires_at: interval.get(1).copied(),
            })
        })
        .collect()
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
            alerts: Vec::new(),
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
            alerts: Vec::new(),
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
    fn a_day_carries_its_span_and_its_chance() {
        let read = readings(decoded(one_place()), moment_of(1757262000));

        let day = &read[0].days[0];
        assert_eq!(day.low, 12.0);
        assert_eq!(day.high, 21.0);
        assert_eq!(day.precipitation_chance, Some(40));
        assert_eq!(day.condition, Condition::LightRain);
        assert_eq!(
            (day.sunrise, day.sunset),
            (None, None),
            "a provider's own sun times are not read; `sunlit` fills them for every provider"
        );
    }

    /// Open-Meteo will send sun times and met.no cannot, so trusting the payload made the same
    /// fact arrive two ways. It is computed on one path now, and the day it is computed for is the
    /// day in the place's own zone rather than in UTC.
    #[test]
    fn sun_times_are_computed_for_every_provider_on_one_path() {
        let vilnius = GeoCoordinates {
            latitude: 54.6872,
            longitude: 25.2797,
        };
        let bare = DayForecast {
            start: Utc
                .with_ymd_and_hms(2026, 9, 7, 21, 0, 0)
                .single()
                .expect("local midnight in Vilnius"),
            condition: Condition::ClearSky,
            low: 8.0,
            high: 18.0,
            precipitation_chance: None,
            sunrise: None,
            sunset: None,
        };

        let [lit] = sunlit(std::slice::from_ref(&bare), &vilnius, 3 * 3600)
            .try_into()
            .expect("one day back");
        let sunrise = lit.sunrise.expect("Vilnius has a sunrise in September");
        let sunset = lit.sunset.expect("and a sunset");

        assert_eq!(
            sunrise.date_naive(),
            NaiveDate::from_ymd_opt(2026, 9, 8).expect("a real date"),
            "the day is the one the place is in, not the one UTC is in"
        );
        assert!(sunrise < sunset);
        assert_eq!(
            (lit.low, lit.high, lit.condition),
            (bare.low, bare.high, bare.condition),
            "nothing else about the day is touched"
        );

        let nowhere = GeoCoordinates {
            latitude: 1000.0,
            longitude: 25.2797,
        };
        let [unlit] = sunlit(&[bare], &nowhere, 3 * 3600)
            .try_into()
            .expect("one day back");
        assert_eq!(
            (unlit.sunrise, unlit.sunset),
            (None, None),
            "coordinates that are not on Earth lose the sun rather than the day"
        );
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

    fn met_no_entry(time: &str, temperature: f64, symbol: &str, chance: f64) -> serde_json::Value {
        serde_json::json!({
            "time": time,
            "data": {
                "instant": { "details": {
                    "air_temperature": temperature,
                    "relative_humidity": 72.0,
                    "wind_speed": 3.0,
                    "wind_from_direction": 230.0
                }},
                "next_1_hours": {
                    "summary": { "symbol_code": symbol },
                    "details": { "precipitation_amount": 25.4, "probability_of_precipitation": chance }
                }
            }
        })
    }

    fn met_no_forecast(entries: Vec<serde_json::Value>) -> MetNoForecast {
        serde_json::from_value(serde_json::json!({
            "properties": { "timeseries": entries }
        }))
        .expect("met.no's own shape decodes")
    }

    fn vilnius() -> GeoCoordinates {
        GeoCoordinates {
            latitude: 54.6872,
            longitude: 25.2797,
        }
    }

    /// `Condition` is a closed set on purpose, so a second provider maps into it rather than
    /// putting its own vocabulary on the wire.
    #[test]
    fn every_met_no_symbol_maps_into_the_closed_set() {
        for (symbol, expected) in [
            ("clearsky_day", Condition::ClearSky),
            ("fair_night", Condition::MainlyClear),
            ("partlycloudy_polartwilight", Condition::PartlyCloudy),
            ("cloudy", Condition::Overcast),
            ("fog", Condition::Fog),
            ("lightrain", Condition::LightRain),
            ("lightrainshowers_day", Condition::LightRain),
            ("rain", Condition::Rain),
            ("heavyrain", Condition::HeavyRain),
            ("lightsnow", Condition::LightSnow),
            ("snowshowers_night", Condition::Snow),
            ("heavysnow", Condition::HeavySnow),
            ("rainandthunder", Condition::Thunderstorm),
            ("heavysleetshowersandthunder_day", Condition::Thunderstorm),
        ] {
            assert_eq!(met_no_condition(Some(symbol)), expected, "{symbol}");
        }

        assert_eq!(
            met_no_condition(Some("something_new_day")),
            Condition::Unknown,
            "a symbol this build has not heard of decodes rather than failing the payload"
        );
        assert_eq!(met_no_condition(None), Condition::Unknown);
    }

    /// The bead's one named gap: met.no has sleet and WMO 4677 does not, so mapping it onto
    /// freezing rain would print "Freezing rain" for wet snow.
    #[test]
    fn sleet_is_sleet_rather_than_the_nearest_wmo_code() {
        for symbol in [
            "lightsleet",
            "sleet",
            "heavysleet",
            "lightsleetshowers_day",
            "sleetshowers_night",
            "heavysleetshowers_day",
        ] {
            assert_eq!(met_no_condition(Some(symbol)), Condition::Sleet, "{symbol}");
        }
    }

    #[test]
    fn a_symbol_suffix_says_which_artwork_not_which_weather() {
        assert_eq!(
            met_no_condition(Some("clearsky_day")),
            met_no_condition(Some("clearsky_night")),
            "the suffix picks the artwork; the weather is the same"
        );
        assert!(met_no_is_day(Some("clearsky_day")));
        assert!(!met_no_is_day(Some("clearsky_night")));
        assert!(
            met_no_is_day(Some("clearsky_polartwilight")),
            "twilight is not night, and there is no third rendering"
        );
        assert!(met_no_is_day(None));
    }

    /// met.no answers in Celsius, metres per second and millimetres whatever is asked of it, so
    /// unlike Open-Meteo the conversion is ours to do.
    #[test]
    fn met_no_answers_in_metric_and_the_units_are_converted_here() {
        assert_eq!(met_no_temperature(0.0, UnitSystem::Metric), 0.0);
        assert_eq!(met_no_temperature(100.0, UnitSystem::Imperial), 212.0);
        assert_eq!(met_no_temperature(-40.0, UnitSystem::Imperial), -40.0);

        assert_eq!(met_no_wind(10.0, UnitSystem::Metric), 36.0);
        assert!((met_no_wind(10.0, UnitSystem::Imperial) - 22.369).abs() < 0.01);

        assert_eq!(met_no_depth(25.4, UnitSystem::Metric), 25.4);
        assert!((met_no_depth(25.4, UnitSystem::Imperial) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_current_reading_carries_the_units_it_was_asked_for() {
        let forecast = met_no_forecast(vec![met_no_entry(
            "2026-09-08T12:00:00Z",
            10.0,
            "rain",
            60.0,
        )]);
        let entry = forecast.properties.timeseries.first();

        let metric = met_no_current(entry, UnitSystem::Metric).expect("a reading");
        assert_eq!(metric.temperature, 10.0);
        assert_eq!(metric.wind_speed, Some(10.8));
        assert_eq!(metric.precipitation, Some(25.4));
        assert_eq!(metric.humidity, Some(72));
        assert_eq!(metric.condition, Condition::Rain);
        assert_eq!(
            metric.apparent_temperature, None,
            "met.no publishes no apparent temperature, and a guess would be a number we invented"
        );

        let imperial = met_no_current(entry, UnitSystem::Imperial).expect("a reading");
        assert_eq!(imperial.temperature, 50.0);
        assert!((imperial.precipitation.unwrap_or_default() - 1.0).abs() < 1e-9);
    }

    /// Open-Meteo aggregates days server-side and met.no does not, so this is the one place the
    /// two providers agree by arithmetic rather than by construction.
    #[test]
    fn days_are_aggregated_from_the_timeseries() {
        let forecast = met_no_forecast(vec![
            met_no_entry("2026-09-08T03:00:00Z", 8.0, "clearsky_night", 5.0),
            met_no_entry("2026-09-08T09:00:00Z", 14.0, "cloudy", 40.0),
            met_no_entry("2026-09-08T18:00:00Z", 11.0, "rain", 70.0),
            met_no_entry("2026-09-09T09:00:00Z", 16.0, "fair_day", 10.0),
        ]);
        let offset = FixedOffset::east_opt(10_800).expect("a real offset");

        let days = met_no_days(
            &forecast.properties.timeseries,
            offset,
            UnitSystem::Metric,
            7,
        );

        assert_eq!(
            days.len(),
            2,
            "two local dates, however many entries they hold"
        );
        assert_eq!((days[0].low, days[0].high), (8.0, 14.0));
        assert_eq!(
            days[0].precipitation_chance,
            Some(70),
            "the day's chance is the worst hour in it, not the last one"
        );
        assert_eq!(
            (days[0].sunrise, days[0].sunset),
            (None, None),
            "aggregation carries no sun; `sunlit` fills it once, for every provider"
        );
        assert_eq!((days[1].low, days[1].high), (16.0, 16.0));

        assert_eq!(
            met_no_days(
                &forecast.properties.timeseries,
                offset,
                UnitSystem::Metric,
                1
            )
            .len(),
            1,
            "the ask's forecast_days caps the aggregation"
        );
    }

    /// Taking the first entry would let the small hours name a day nobody is awake for.
    #[test]
    fn a_day_is_named_by_the_weather_in_the_middle_of_it() {
        let forecast = met_no_forecast(vec![
            met_no_entry("2026-09-08T00:00:00Z", 8.0, "heavyrain", 90.0),
            met_no_entry("2026-09-08T09:00:00Z", 14.0, "clearsky_day", 5.0),
            met_no_entry("2026-09-08T21:00:00Z", 9.0, "snow", 80.0),
        ]);
        let offset = FixedOffset::east_opt(10_800).expect("a real offset");

        let days = met_no_days(
            &forecast.properties.timeseries,
            offset,
            UnitSystem::Metric,
            7,
        );

        assert_eq!(
            days[0].condition,
            Condition::ClearSky,
            "09:00 UTC is midday in this zone; the downpour at midnight does not name the day"
        );
    }

    /// The offset is the whole reason `tzf-rs` is a dependency: met.no reports instants in UTC and
    /// says nothing about where they were measured.
    #[test]
    fn a_places_offset_is_looked_up_from_its_coordinates() {
        let summer = Utc
            .with_ymd_and_hms(2026, 7, 1, 12, 0, 0)
            .single()
            .expect("a real instant");
        let winter = Utc
            .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
            .single()
            .expect("a real instant");

        assert_eq!(met_no_offset(&vilnius(), summer), 3 * 3600);
        assert_eq!(
            met_no_offset(&vilnius(), winter),
            2 * 3600,
            "the zone is looked up, so daylight saving follows the date rather than being frozen"
        );

        let ocean = GeoCoordinates {
            latitude: 0.0,
            longitude: -30.0,
        };
        assert_eq!(
            met_no_offset(&ocean, summer),
            -2 * 3600,
            "open water is a nautical zone rather than a hole in the dataset, so there is no \
             coordinate a forecast can be asked for that has no offset"
        );
    }

    #[test]
    fn a_met_no_alert_becomes_a_wire_alert() {
        let alerts: MetNoAlerts = serde_json::from_value(serde_json::json!({
            "features": [
                {
                    "properties": {
                        "title": "Gale, yellow level",
                        "description": "Southwesterly gale.",
                        "severity": "Moderate",
                        "sender": "MET Norway"
                    },
                    "when": { "interval": ["2026-09-08T12:00:00Z", "2026-09-08T18:00:00Z"] }
                },
                {
                    "properties": { "event": "rain", "severity": "Extreme" }
                },
                {
                    "properties": { "severity": "Severe" }
                }
            ]
        }))
        .expect("met.no's alert shape decodes");

        let mapped = met_no_warnings(&alerts);

        assert_eq!(
            mapped.len(),
            2,
            "an alert with neither a title nor an event names nothing and is dropped"
        );
        assert_eq!(mapped[0].headline, "Gale, yellow level");
        assert_eq!(mapped[0].severity, AlertSeverity::Moderate);
        assert_eq!(mapped[0].source.as_deref(), Some("MET Norway"));
        assert!(mapped[0].starts_at.is_some() && mapped[0].expires_at.is_some());
        assert_eq!(
            mapped[1].headline, "rain",
            "an alert with no title falls back to the event it names"
        );
        assert_eq!(mapped[1].severity, AlertSeverity::Extreme);
        assert_eq!(mapped[1].starts_at, None);
    }

    #[test]
    fn an_unfamiliar_alert_severity_states_itself_rather_than_alarming() {
        assert_eq!(met_no_severity(Some("Minor")), AlertSeverity::Minor);
        assert_eq!(met_no_severity(Some("severe")), AlertSeverity::Severe);
        assert_eq!(
            met_no_severity(Some("catastrophic")),
            AlertSeverity::Unknown
        );
        assert_eq!(met_no_severity(None), AlertSeverity::Unknown);
    }

    /// A provider that answers with alerts reaches a payload through `absorb`, which is where
    /// `sanitized` runs — so hostile prose off a national feed is capped and bidi-stripped whichever
    /// provider produced it.
    #[tokio::test]
    async fn a_second_providers_alerts_go_through_the_same_gate() {
        let mut harness = harness().await;
        call(&mut harness, Command::Watch(at(54.6872, 25.2797)))
            .await
            .expect("a place to hold a reading");

        harness.service.absorb(&[Reading {
            alerts: vec![WeatherAlert {
                severity: AlertSeverity::Severe,
                headline: "Gale\u{202e}gpj.exe".to_owned(),
                description: None,
                source: None,
                starts_at: None,
                expires_at: None,
            }],
            ..a_reading()
        }]);

        assert_eq!(harness.service.places.len(), 1);
        assert_eq!(
            harness.service.places[0].alerts[0].headline, "Gale gpj.exe",
            "the override is stripped whichever provider sent it"
        );
    }

    #[test]
    fn hours_open_at_the_hour_standing_and_are_capped() {
        let entries: Vec<serde_json::Value> = (0..40)
            .map(|hour| {
                met_no_entry(
                    &format!("2026-09-08T{:02}:00:00Z", hour % 24),
                    10.0 + hour as f64,
                    "cloudy",
                    0.0,
                )
            })
            .collect();
        let forecast = met_no_forecast(entries);
        let now = Utc
            .with_ymd_and_hms(2026, 9, 8, 0, 30, 0)
            .single()
            .expect("a real instant");

        let hours = met_no_hours(&forecast.properties.timeseries, UnitSystem::Metric, now);

        assert_eq!(hours.len(), HOURS, "the window is capped at a day");
        assert_eq!(
            hours[0].time.hour(),
            0,
            "the window opens at the hour standing rather than the one after it"
        );
    }

    fn a_reading() -> Reading {
        Reading {
            utc_offset_seconds: 7200,
            current: None,
            hours: Vec::new(),
            days: Vec::new(),
            alerts: Vec::new(),
        }
    }

    fn an_alert(headline: &str) -> WeatherAlert {
        WeatherAlert {
            severity: AlertSeverity::Severe,
            headline: headline.to_owned(),
            description: None,
            source: None,
            starts_at: None,
            expires_at: None,
        }
    }

    /// The only test standing between a national alert feed and a `Gtk.Label`: an alert headline is
    /// third-party prose off the network, unlike every other string in this payload.
    #[test]
    fn an_alert_headline_is_capped_and_stripped_of_bidi_overrides() {
        let alert = WeatherAlert {
            description: Some("  line\tone\n\nline two  ".to_owned()),
            source: Some("LHMT\u{202e}".to_owned()),
            ..an_alert("Thunderstorm\u{202e}gpj.exe warning")
        };

        let [cleaned] = sanitized(&[alert]).try_into().expect("one alert back");

        assert_eq!(cleaned.headline, "Thunderstorm gpj.exe warning");
        assert_eq!(cleaned.description.as_deref(), Some("line one line two"));
        assert_eq!(cleaned.source.as_deref(), Some("LHMT"));
        assert_eq!(
            sanitized(&[an_alert(&"e".repeat(HEADLINE + 10))])[0].headline,
            format!("{}…", "e".repeat(HEADLINE))
        );
    }

    #[test]
    fn more_alerts_than_the_cap_are_dropped_rather_than_rendered() {
        let many: Vec<_> = (0..MOST_ALERTS + 5)
            .map(|index| an_alert(&format!("alert {index}")))
            .collect();

        assert_eq!(sanitized(&many).len(), MOST_ALERTS);
    }

    /// A newer panel must decode an older daemon's payload, which is what `serde(default)` buys.
    #[test]
    fn a_payload_without_an_alerts_field_still_decodes() {
        let older = serde_json::json!({
            "place": { "at": "here" },
            "coordinates": { "latitude": 54.6872, "longitude": 25.2797 },
            "utc_offset_seconds": 7200,
            "current": null,
            "hours": [],
            "days": []
        });

        let decoded: PlaceWeather =
            serde_json::from_value(older).expect("an older payload decodes");

        assert!(decoded.alerts.is_empty());
    }

    #[test]
    fn every_alert_severity_decodes_from_its_wire_name() {
        for (name, severity) in [
            ("minor", AlertSeverity::Minor),
            ("moderate", AlertSeverity::Moderate),
            ("severe", AlertSeverity::Severe),
            ("extreme", AlertSeverity::Extreme),
            ("something_new", AlertSeverity::Unknown),
        ] {
            let decoded: AlertSeverity =
                serde_json::from_value(serde_json::json!({ "severity": name }))
                    .unwrap_or_else(|_| panic!("{name} decodes"));
            assert_eq!(decoded, severity, "{name}");
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
