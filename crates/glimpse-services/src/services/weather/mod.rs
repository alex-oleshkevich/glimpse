use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset, Offset as _, Utc};
use glimpse_config::WeatherProvider as ConfiguredProvider;
use glimpse_dbus::weather::{
    CurrentWeather, DayForecast, GeoCoordinates, HourForecast, PlaceWeather, UnitSystem,
    WatchedPlace, WeatherAlert, WeatherStatus,
};
use glimpse_utils::clean;
use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
    services::geolocation::GeolocationHandle,
    subscription::Sub,
};

mod met_no;
mod open_meteo;
mod place;

use met_no::met_no;
use open_meteo::open_meteo;
use place::ResolvedPlace;

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
const REASON: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenMeteo,
    MetNo,
}

impl Provider {
    async fn fetch(self, client: reqwest::Client, request: Request) -> Result<Fetch, String> {
        let mut targets = Vec::with_capacity(request.targets.len());
        let mut failure = None;
        for target in request.targets {
            let resolved = match target.resolved {
                Some(place) => Ok(place),
                None => place::resolve(&client, &target.place, target.coordinates).await,
            };
            match resolved {
                Ok(resolved) => targets.push(ResolvedTarget {
                    place: target.place,
                    resolved,
                }),
                Err(reason) => {
                    failure.get_or_insert(reason);
                }
            };
        }

        if targets.is_empty() {
            return Ok(Fetch {
                targets,
                readings: Vec::new(),
                failure,
            });
        }

        let ask = Ask {
            coordinates: targets
                .iter()
                .map(|target| target.resolved.coordinates.clone())
                .collect(),
            units: request.units,
            forecast_days: request.forecast_days,
        };
        let readings = match self {
            Self::OpenMeteo => open_meteo(client, ask).await,
            Self::MetNo => met_no(client, ask).await,
        }?;
        Ok(Fetch {
            targets,
            readings,
            failure,
        })
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
            units: match document.regional.is_metric() {
                true => UnitSystem::Metric,
                false => UnitSystem::Imperial,
            },
            poll_interval: document.weather.poll_interval.max(MIN_POLL),
            forecast_days: document.weather.forecast_days.clamp(1, MAX_DAYS),
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Watch {
        place: WatchedPlace,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Refresh {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(Clone)]
pub struct WeatherHandle(ServiceEndpoint<Weather>);

impl WeatherHandle {
    pub fn snapshot(&self) -> WeatherStatus {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<WeatherStatus> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn watch(&self, place: WatchedPlace) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Watch { place, reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("weather stopped before accepting the place".to_owned())
        })?
    }

    pub async fn refresh(&self) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::Refresh { reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("weather stopped before refreshing".to_owned())
        })?
    }
}

#[derive(Clone)]
pub struct WeatherDependencies {
    pub geolocation: GeolocationHandle,
}

fn initial_state(config: &Config) -> WeatherStatus {
    WeatherStatus {
        units: config.units,
        places: Vec::new(),
        updated_at: None,
    }
}

pub enum Event {
    Located(Option<GeoCoordinates>),
    Fetched {
        generation: u64,
        result: Result<Fetch, String>,
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
struct Target {
    place: WatchedPlace,
    coordinates: Option<GeoCoordinates>,
    resolved: Option<ResolvedPlace>,
}

#[derive(Debug, Clone, PartialEq)]
struct Request {
    targets: Vec<Target>,
    units: UnitSystem,
    forecast_days: u8,
}

#[derive(Debug)]
pub struct Fetch {
    targets: Vec<ResolvedTarget>,
    readings: Vec<Reading>,
    failure: Option<String>,
}

#[derive(Debug)]
struct ResolvedTarget {
    place: WatchedPlace,
    resolved: ResolvedPlace,
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
    location: GeolocationHandle,
    client: Option<reqwest::Client>,
    config: Config,
    watched: Vec<Lease>,
    fix: Option<GeoCoordinates>,
    generation: u64,
    places: Vec<PlaceWeather>,
    updated_at: Option<DateTime<Utc>>,
    failure: Option<String>,
}

impl Service for Weather {
    const NAME: &'static str = "weather";

    type Config = Config;
    type State = WeatherStatus;
    type Handle = WeatherHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = WeatherDependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        WeatherHandle(endpoint)
    }

    fn initial_state(config: &Self::Config) -> Self::State {
        initial_state(config)
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut declared = Vec::new();

        if self.wants_here() {
            declared.push(Sub::watch(
                Watch::Location,
                self.location.subscribe(),
                |data| Event::Located(data.coordinates),
                Event::Located(None),
            ));
        }

        let request = self.request();
        if !request.targets.is_empty()
            && let Some(client) = self.client.clone()
        {
            let generation = self.generation;
            let provider = self.config.provider;

            declared.push(Sub::interval(
                Watch::Poll { generation },
                Duration::from_secs(self.config.poll_interval.max(MIN_POLL)),
                move |_ctx| {
                    let client = client.clone();
                    let request = request.clone();
                    async move {
                        Event::Fetched {
                            generation,
                            result: provider.fetch(client, request).await,
                        }
                    }
                },
            ));
        }

        declared
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
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
            status: ctx.publisher(),
            client,
            config,
            watched: Vec::new(),
            fix: None,
            generation: 0,
            places: Vec::new(),
            updated_at: None,
            failure: None,
            location: dependencies.geolocation,
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
                    Ok(fetch) if fetch.readings.len() == fetch.targets.len() => {
                        tracing::debug!(places = fetch.readings.len(), "weather fetch completed");
                        let failure = fetch.failure.as_deref().map(|reason| clean(reason, REASON));
                        if let Some(reason) = &failure {
                            tracing::warn!(reason, "some weather places could not be resolved");
                        }
                        self.failure = failure.filter(|_| fetch.readings.is_empty());
                        self.absorb(&fetch);
                        if !fetch.readings.is_empty() {
                            self.updated_at = Some(Utc::now());
                        }
                    }
                    Ok(fetch) => {
                        tracing::warn!(
                            expected = fetch.targets.len(),
                            readings = fetch.readings.len(),
                            "weather provider returned an incomplete response"
                        );
                        self.failure =
                            Some("the provider answered for a different set of places".to_owned());
                    }
                    Err(reason) => {
                        let reason = clean(&reason, REASON);
                        tracing::warn!(reason, "weather fetch failed");
                        self.failure = Some(reason);
                    }
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
                    self.updated_at = None;
                }
                tracing::info!(
                    provider = ?config.provider,
                    units = ?config.units,
                    poll_interval = config.poll_interval,
                    forecast_days = config.forecast_days,
                    "weather configuration applied"
                );
                self.config = config;
                self.generation += 1;
                self.report(ctx);
                self.publish();
            }

            Input::Command(Command::Watch { place, reply }) => {
                let now = Instant::now();
                let asked = self.request().targets;
                let mut changed = self.sweep(now);

                let outcome = placed(place).and_then(|place| self.lease(place, now));
                changed |= matches!(outcome, Ok(true));

                if changed {
                    if self.request().targets != asked {
                        self.generation += 1;
                    }
                    self.forget_unwatched();
                    self.report(ctx);
                    self.publish();
                    tracing::debug!(places = self.watched.len(), "weather watch set changed");
                }

                match outcome {
                    Ok(_) => {
                        let _ = reply.send(Ok(()));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }

            Input::Command(Command::Refresh { reply }) => {
                self.generation += 1;
                tracing::debug!(generation = self.generation, "weather refresh scheduled");
                let _ = reply.send(Ok(()));
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

    fn targets(&self) -> Vec<Target> {
        self.watched
            .iter()
            .filter_map(|lease| match &lease.place {
                WatchedPlace::Here => Some(self.target(WatchedPlace::Here, self.fix.clone()?)),
                WatchedPlace::Coordinates {
                    latitude,
                    longitude,
                } => Some(self.target(
                    lease.place.clone(),
                    GeoCoordinates {
                        latitude: *latitude,
                        longitude: *longitude,
                    },
                )),
                WatchedPlace::Location { .. } => Some(Target {
                    place: lease.place.clone(),
                    coordinates: None,
                    resolved: self.resolved(&lease.place),
                }),
            })
            .collect()
    }

    fn target(&self, place: WatchedPlace, coordinates: GeoCoordinates) -> Target {
        Target {
            resolved: self.resolved(&place),
            place,
            coordinates: Some(coordinates),
        }
    }

    fn resolved(&self, place: &WatchedPlace) -> Option<ResolvedPlace> {
        let shown = self.places.iter().find(|shown| &shown.place == place)?;
        Some(ResolvedPlace {
            coordinates: shown.coordinates.clone(),
            city: shown.city.clone()?,
            country_code: shown.country_code.clone()?,
        })
    }

    fn request(&self) -> Request {
        Request {
            targets: self.targets(),
            units: self.config.units,
            forecast_days: self.config.forecast_days,
        }
    }

    /// `Ok(true)` when the set of places actually grew; a renewal answers `Ok(false)` so it cannot
    /// restart the poll, which would turn a fifteen-minute interval into whatever the renewal
    /// cadence happens to be.
    fn lease(&mut self, place: WatchedPlace, now: Instant) -> Result<bool, CommandError> {
        let until = now + LEASE;

        if let Some(held) = self.watched.iter_mut().find(|held| held.place == place) {
            held.until = until;
            return Ok(false);
        }

        if self.watched.len() >= MOST_WATCHED {
            return Err(CommandError::LimitExceeded(format!(
                "no more than {MOST_WATCHED} places are reported at once"
            )));
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

    fn absorb(&mut self, fetch: &Fetch) {
        let cap = self.config.forecast_days as usize;

        self.places = fetch
            .targets
            .iter()
            .zip(&fetch.readings)
            .map(|(target, reading)| PlaceWeather {
                place: target.place.clone(),
                days: sunlit(
                    &reading.days,
                    cap,
                    &target.resolved.coordinates,
                    reading.utc_offset_seconds,
                ),
                coordinates: target.resolved.coordinates.clone(),
                city: Some(target.resolved.city.clone()),
                country_code: Some(target.resolved.country_code.clone()),
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
            updated_at: self.updated_at,
        });
    }
}

/// Presence is the wire format's job. What is left is range: a mistyped latitude is a mistake worth
/// refusing at the call rather than a request for somewhere that is not on Earth.
fn placed(place: WatchedPlace) -> Result<WatchedPlace, CommandError> {
    match place {
        WatchedPlace::Here => Ok(place),
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        } if (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) => {
            Ok(place)
        }
        WatchedPlace::Coordinates { .. } => Err(CommandError::InvalidArgument(
            "those coordinates are not on Earth".to_owned(),
        )),
        WatchedPlace::Location { name } => place::normalize(&name)
            .map(|name| WatchedPlace::Location { name })
            .map_err(CommandError::InvalidArgument),
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
fn sunlit(
    days: &[DayForecast],
    cap: usize,
    coordinates: &GeoCoordinates,
    seconds: i32,
) -> Vec<DayForecast> {
    let offset = FixedOffset::east_opt(seconds).unwrap_or_else(|| Utc.fix());

    days.iter()
        .take(cap)
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

/// A transport failure must not carry the request: the query string holds the user's latitude and
/// longitude, so a logged URL is a location leak.
fn transport(error: reqwest::Error) -> String {
    match error.is_timeout() {
        true => "the request timed out".to_owned(),
        false => error.without_url().to_string(),
    }
}

async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> Result<T, String> {
    let response = client.get(url).send().await.map_err(transport)?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("the provider answered {status}"));
    }

    let body = response.text().await.map_err(transport)?;
    serde_json::from_str(&body).map_err(|_| "the provider answered something else".to_owned())
}

/// The window opens at the hour standing rather than the one after it: a renderer that wants to
/// skip it can, and one that wants to read the current hour off the strip cannot get it back.
/// Every provider's hours are cut to the same window, so the rule lives here rather than in each.
fn hour_floor(now: DateTime<Utc>) -> i64 {
    let epoch = now.timestamp();
    epoch - epoch.rem_euclid(3600)
}

fn percent(value: Option<f64>) -> Option<u8> {
    value.map(|reading| reading.round().clamp(0.0, 100.0) as u8)
}

fn bearing(degrees: Option<f64>) -> Option<u16> {
    degrees.map(|reading| reading.round().rem_euclid(360.0) as u16)
}

#[cfg(test)]
mod tests;
