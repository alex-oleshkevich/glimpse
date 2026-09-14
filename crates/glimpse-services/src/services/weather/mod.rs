use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset, Offset as _, Utc};
use glimpse_config::WeatherProvider as ConfiguredProvider;
use glimpse_contracts::{
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

pub fn initial_state(config: &Config) -> WeatherStatus {
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
mod tests {

    use chrono::{NaiveDate, TimeZone as _};
    use glimpse_contracts::{AlertSeverity, Condition};
    use glimpse_dbus::Buses;
    use tokio::sync::{mpsc, oneshot};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{ServiceState, service::ServiceRuntime, services::geolocation::Geolocation};

    enum Asked {
        Watch(WatchedPlace),
        Refresh,
    }

    struct Harness {
        service: Weather,
        ctx: Ctx<Weather>,
        health: tokio::sync::watch::Receiver<ServiceState>,
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
        let (location_runtime, location) = ServiceRuntime::<Geolocation>::new(
            Geolocation::initial_state(),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        drop(location_runtime);
        let (health, health_rx) = tokio::sync::watch::channel(ServiceState::Starting);
        let (state, _state_rx) = tokio::sync::watch::channel(initial_state(&config));
        let ctx = Ctx::<Weather>::new(
            events,
            &cancel,
            state,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let service = Weather::start(
            &ctx,
            config,
            WeatherDependencies {
                geolocation: location,
            },
        )
        .await
        .expect("the service starts");

        Harness {
            service,
            ctx,
            health: health_rx,
            _inbox: inbox,
            _cancel: cancel,
        }
    }

    async fn call(harness: &mut Harness, asked: Asked) -> Result<(), CommandError> {
        let (reply, answer) = oneshot::channel();
        let command = match asked {
            Asked::Watch(place) => Command::Watch { place, reply },
            Asked::Refresh => Command::Refresh { reply },
        };
        harness
            .service
            .handle(&harness.ctx, Input::Command(command))
            .await;
        answer.await.expect("the command answers")
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

    fn reason(health: &tokio::sync::watch::Receiver<ServiceState>) -> Option<String> {
        match health.borrow().clone() {
            ServiceState::Degraded { reason } => Some(reason),
            _ => None,
        }
    }

    fn document(weather: glimpse_config::WeatherConfig) -> glimpse_config::Config {
        glimpse_config::Config {
            weather,
            ..Default::default()
        }
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

    /// The privacy property the lease design exists to give: until something asks, no request is
    /// made, so no coordinate leaves the machine. It says nothing about GeoClue, which the
    /// `geolocation` service runs on its own account.
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

    /// A `Here` lease with no fix contributes no coordinate, so nothing is asked for and the
    /// service says which half is missing.
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

    /// A consumer renews on its own tick. If a renewal restarted the poll — `ctx.interval` starts
    /// at `Instant::now()` — the service would fetch at the renewal cadence instead of its own.
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

    /// Refusing a renewal at the cap would expire the very leases holding it full.
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

        call(&mut harness, Asked::Watch(at(60.0, 0.0)))
            .await
            .expect("the cap counts live leases rather than dead ones");

        assert_eq!(harness.service.watched.len(), 1);
    }

    /// A `here` lease with no fix resolves to no coordinate, so it changes the lease set without
    /// changing the request. Bumping the generation for it would refetch the same places.
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

    /// The pairs are written literally rather than built through `placed`, which is the function
    /// under test: a helper that called it would compare its output against itself.
    #[test]
    fn a_watch_on_coordinates_outside_their_ranges_is_refused() {
        for (latitude, longitude) in [(91.0, 0.0), (-91.0, 0.0), (0.0, 181.0), (0.0, -181.0)] {
            let refused = placed(at(latitude, longitude)).expect_err("not on Earth");

            assert!(matches!(refused, CommandError::InvalidArgument(_)));
        }

        for (latitude, longitude) in [(90.0, 180.0), (-90.0, -180.0), (0.0, 0.0)] {
            placed(at(latitude, longitude)).expect("the edges are on Earth");
        }
    }

    #[tokio::test]
    async fn a_named_watch_is_normalized_and_requested_without_panel_coordinates() {
        let mut harness = harness().await;

        call(
            &mut harness,
            Asked::Watch(WatchedPlace::Location {
                name: "Vilnius, lt".to_owned(),
            }),
        )
        .await
        .expect("a named place");

        assert_eq!(
            harness.service.request().targets,
            vec![Target {
                place: WatchedPlace::Location {
                    name: "Vilnius, LT".to_owned(),
                },
                coordinates: None,
                resolved: None,
            }]
        );
    }

    #[test]
    fn a_named_watch_needs_a_city_and_two_letter_country_code() {
        for name in ["Vilnius", ", LT", "Vilnius, Lithuania"] {
            let refused = placed(WatchedPlace::Location {
                name: name.to_owned(),
            })
            .expect_err("not a City, CC location");

            assert!(matches!(refused, CommandError::InvalidArgument(_)));
        }
    }

    #[tokio::test]
    async fn resolved_place_metadata_is_owned_by_the_weather_result() {
        let mut harness = harness().await;
        call(
            &mut harness,
            Asked::Watch(WatchedPlace::Location {
                name: "Wilno, LT".to_owned(),
            }),
        )
        .await
        .expect("a named place");

        harness.service.absorb(&Fetch {
            targets: vec![ResolvedTarget {
                place: WatchedPlace::Location {
                    name: "Wilno, LT".to_owned(),
                },
                resolved: ResolvedPlace {
                    coordinates: fix(54.6872, 25.2797),
                    city: "Vilnius".to_owned(),
                    country_code: "LT".to_owned(),
                },
            }],
            readings: vec![a_reading()],
            failure: None,
        });

        assert_eq!(harness.service.places[0].city.as_deref(), Some("Vilnius"));
        assert_eq!(
            harness.service.places[0].country_code.as_deref(),
            Some("LT")
        );
        assert_eq!(
            harness.service.places[0].place,
            WatchedPlace::Location {
                name: "Wilno, LT".to_owned(),
            }
        );
    }

    /// Without the threshold a fix that jitters by metres bumps the generation on every update and
    /// refetches for ever.
    #[tokio::test]
    async fn a_fix_that_moves_less_than_a_kilometre_does_not_refetch() {
        let mut harness = harness().await;
        call(&mut harness, Asked::Watch(WatchedPlace::Here))
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
        call(&mut harness, Asked::Watch(WatchedPlace::Here))
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
        call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a place to hold a reading");
        harness.service.places = vec![PlaceWeather {
            place: at(47.3769, 8.5417),
            coordinates: fix(47.3769, 8.5417),
            city: None,
            country_code: None,
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
            city: None,
            country_code: None,
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

        let [lit] = sunlit(std::slice::from_ref(&bare), 7, &vilnius, 3 * 3600)
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
        let [unlit] = sunlit(&[bare], 7, &nowhere, 3 * 3600)
            .try_into()
            .expect("one day back");
        assert_eq!(
            (unlit.sunrise, unlit.sunset),
            (None, None),
            "coordinates that are not on Earth lose the sun rather than the day"
        );
    }

    #[tokio::test]
    async fn the_days_published_are_cut_to_the_ask() {
        let mut harness = harness_with(Config {
            provider: Provider::OpenMeteo,
            units: UnitSystem::Metric,
            poll_interval: 900,
            forecast_days: 2,
        })
        .await;
        call(&mut harness, Asked::Watch(at(54.6872, 25.2797)))
            .await
            .expect("a place to hold a reading");

        let day = |number: u32| DayForecast {
            start: Utc
                .with_ymd_and_hms(2026, 9, number, 21, 0, 0)
                .single()
                .expect("a real instant"),
            condition: Condition::ClearSky,
            low: 8.0,
            high: 18.0,
            precipitation_chance: None,
            sunrise: None,
            sunset: None,
        };

        let fetch = fetched(
            &harness.service,
            vec![Reading {
                days: (1..=5).map(day).collect(),
                ..a_reading()
            }],
        );
        harness.service.absorb(&fetch);

        assert_eq!(
            harness.service.places[0].days.len(),
            2,
            "the ask cuts the list once, for whichever provider answered"
        );
    }

    /// A daemon that learns a new condition must not make an older reader fail to decode the whole
    /// payload. Adding a field is already safe; adding a variant is only safe because of `other`.
    #[test]
    fn an_unknown_condition_variant_decodes_rather_than_failing_the_payload() {
        let decoded: Condition = serde_json::from_str(r#"{"condition":"volcanic_ash"}"#)
            .expect("an unrecognized condition still decodes");

        assert_eq!(decoded, Condition::Unknown);
    }

    /// A provider that answers with alerts reaches a payload through `absorb`, which is where
    /// `sanitized` runs — so hostile prose off a national feed is capped and bidi-stripped whichever
    /// provider produced it.
    #[tokio::test]
    async fn a_second_providers_alerts_go_through_the_same_gate() {
        let mut harness = harness().await;
        call(&mut harness, Asked::Watch(at(54.6872, 25.2797)))
            .await
            .expect("a place to hold a reading");

        let fetch = fetched(
            &harness.service,
            vec![Reading {
                alerts: vec![WeatherAlert {
                    severity: AlertSeverity::Severe,
                    headline: "Gale\u{202e}gpj.exe".to_owned(),
                    description: None,
                    source: None,
                    starts_at: None,
                    expires_at: None,
                }],
                ..a_reading()
            }],
        );
        harness.service.absorb(&fetch);

        assert_eq!(harness.service.places.len(), 1);
        assert_eq!(
            harness.service.places[0].alerts[0].headline, "Gale gpj.exe",
            "the override is stripped whichever provider sent it"
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

    fn fetched(service: &Weather, readings: Vec<Reading>) -> Fetch {
        let targets = service
            .targets()
            .into_iter()
            .zip(&readings)
            .enumerate()
            .map(|(index, (target, _))| ResolvedTarget {
                place: target.place,
                resolved: ResolvedPlace {
                    coordinates: fix(54.6872 + index as f64, 25.2797),
                    city: format!("City {index}"),
                    country_code: "LT".to_owned(),
                },
            })
            .collect();
        Fetch {
            targets,
            readings,
            failure: None,
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

    #[tokio::test]
    async fn a_failed_fetch_keeps_the_last_reading_and_degrades() {
        let mut harness = harness().await;
        call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a place to hold a reading");

        let generation = harness.service.generation;
        let fetch = fetched(&harness.service, vec![a_reading()]);
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation,
                    result: Ok(fetch),
                }),
            )
            .await;
        assert_eq!(harness.service.places.len(), 1);
        let updated_at = harness
            .service
            .updated_at
            .expect("a successful update time");

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
        assert_eq!(harness.service.updated_at, Some(updated_at));
        assert_eq!(
            reason(&harness.health).as_deref(),
            Some("the request timed out")
        );
    }

    #[tokio::test]
    async fn an_unresolved_named_place_keeps_other_places_current() {
        let mut harness = harness().await;
        let valid = at(47.3769, 8.5417);
        call(&mut harness, Asked::Watch(valid.clone()))
            .await
            .expect("a fixed place");
        call(
            &mut harness,
            Asked::Watch(WatchedPlace::Location {
                name: "Missing, ZZ".to_owned(),
            }),
        )
        .await
        .expect("a syntactically valid named place");

        let fetch = Fetch {
            targets: vec![ResolvedTarget {
                place: valid.clone(),
                resolved: ResolvedPlace {
                    coordinates: fix(47.3769, 8.5417),
                    city: "Zurich".to_owned(),
                    country_code: "CH".to_owned(),
                },
            }],
            readings: vec![a_reading()],
            failure: Some("no location matches the requested city and country".to_owned()),
        };
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(fetch),
                }),
            )
            .await;

        assert_eq!(
            harness
                .service
                .places
                .iter()
                .map(|place| place.place.clone())
                .collect::<Vec<_>>(),
            vec![valid]
        );
        assert_eq!(harness.health.borrow().clone(), ServiceState::Running);
    }

    #[tokio::test]
    async fn an_unresolved_only_place_degrades_without_echoing_its_name() {
        let mut harness = harness().await;
        call(
            &mut harness,
            Asked::Watch(WatchedPlace::Location {
                name: "Private Place, ZZ".to_owned(),
            }),
        )
        .await
        .expect("a syntactically valid named place");

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(Fetch {
                        targets: Vec::new(),
                        readings: Vec::new(),
                        failure: Some(
                            "no location matches the requested city and country".to_owned(),
                        ),
                    }),
                }),
            )
            .await;

        let failure = reason(&harness.health).expect("an unresolved-only fetch degrades");
        assert_eq!(
            failure,
            "no location matches the requested city and country"
        );
        assert!(!failure.contains("Private Place"));
    }

    /// The query string carries the user's latitude and longitude, so a reason that quoted the
    /// request would be a location leak wherever it was pasted.
    #[tokio::test]
    async fn a_failure_reason_names_neither_the_host_nor_a_coordinate() {
        let mut harness = harness().await;
        call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
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

        let rendered = reason(&harness.health).expect("a degraded reason");
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
        call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
            .await
            .expect("a watched place");

        let stale = harness.service.generation;
        call(&mut harness, Asked::Refresh)
            .await
            .expect("refresh answers");
        assert!(harness.service.generation > stale);

        let fetch = fetched(&harness.service, vec![a_reading()]);
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: stale,
                    result: Ok(fetch),
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
            call(&mut harness, Asked::Watch(at(pair.0, pair.1)))
                .await
                .expect("a watched place");
        }

        let mut fetch = fetched(&harness.service, vec![a_reading()]);
        fetch.targets.push(ResolvedTarget {
            place: at(-33.8688, 151.2093),
            resolved: ResolvedPlace {
                coordinates: fix(-33.8688, 151.2093),
                city: "Sydney".to_owned(),
                country_code: "AU".to_owned(),
            },
        });
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(fetch),
                }),
            )
            .await;

        assert!(harness.service.places.is_empty());
        assert_eq!(
            reason(&harness.health).as_deref(),
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

        let fetch = fetched(&harness.service, vec![a_reading()]);
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(fetch),
                }),
            )
            .await;
        assert_eq!(harness.service.places.len(), 1);

        harness.service.watched[0].until = now;
        let fetch = fetched(&harness.service, Vec::new());
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(fetch),
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

        let fetch = fetched(&harness.service, vec![a_reading(), a_reading()]);
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Fetched {
                    generation: harness.service.generation,
                    result: Ok(fetch),
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
}
