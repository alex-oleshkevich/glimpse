use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use glimpse_contracts::{
    AlertSeverity, Condition, CurrentWeather, DayForecast, GeoCoordinates, HourForecast,
    PlaceWeather, UnitSystem, WatchedPlace, WeatherAlert, WeatherStatus,
};
use glimpse_utils::clean;
use tokio::sync::{RwLock, watch};
use zbus::proxy::CacheProperties;

pub const GLIMPSE_WEATHER_BUS_NAME: &str = "me.aresa.Glimpse.Weather";
pub const GLIMPSE_WEATHER_OBJECT_PATH: &str = "/me/aresa/Glimpse/Weather";

const MOST_PLACES: usize = 8;
const HOURS: usize = 24;
const DAYS: usize = 10;
const MOST_ALERTS: usize = 4;
const REASON: usize = 240;
const HEADLINE: usize = 120;
const DESCRIPTION: usize = 600;
const SOURCE: usize = 60;
const CITY: usize = 120;
const COUNTRY_CODE: usize = 2;

pub type OptionalDoubleWire = (
    bool, // present
    f64,  // value, ignored when absent
);
pub type OptionalByteWire = (
    bool, // present
    u8,   // value, ignored when absent
);
pub type OptionalU16Wire = (
    bool, // present
    u16,  // value, ignored when absent
);
pub type OptionalTimestampWire = (
    bool, // present
    i64,  // Unix microseconds, ignored when absent
);
pub type OptionalStringWire = (
    bool,   // present
    String, // value, ignored when absent
);
pub type CurrentWeatherWire = (
    i64,                // observation time in Unix microseconds
    u8,                 // condition: 0..=19 known, 255 unknown
    bool,               // daylight at the observation time
    f64,                // temperature in the snapshot's unit system
    OptionalDoubleWire, // apparent temperature in the snapshot's unit system
    OptionalByteWire,   // relative humidity percentage
    OptionalDoubleWire, // wind speed in the snapshot's unit system
    OptionalU16Wire,    // wind direction in degrees
    OptionalDoubleWire, // precipitation in the snapshot's unit system
);
pub type HourForecastWire = (
    i64,  // forecast time in Unix microseconds
    u8,   // condition: 0..=19 known, 255 unknown
    bool, // daylight at the forecast time
    f64,  // temperature in the snapshot's unit system
);
pub type DayForecastWire = (
    i64,                   // day start in Unix microseconds
    u8,                    // condition: 0..=19 known, 255 unknown
    f64,                   // low temperature in the snapshot's unit system
    f64,                   // high temperature in the snapshot's unit system
    OptionalByteWire,      // precipitation probability percentage
    OptionalTimestampWire, // sunrise
    OptionalTimestampWire, // sunset
);
pub type WeatherAlertWire = (
    u8,                    // severity: 0 minor, 1 moderate, 2 severe, 3 extreme, 255 unknown
    String,                // headline
    OptionalStringWire,    // description
    OptionalStringWire,    // issuing source
    OptionalTimestampWire, // start time
    OptionalTimestampWire, // expiration time
);
pub type PlaceWeatherWire = (
    u8,                         // requested place: 0 current location, 1 coordinates, 2 named location
    f64,                        // requested latitude, ignored for current and named locations
    f64,                        // requested longitude, ignored for current and named locations
    String,                     // requested location name, used only for named location
    f64,                        // resolved latitude
    f64,                        // resolved longitude
    String,                     // canonical city, empty when unavailable
    String,                     // canonical ISO 3166-1 alpha-2 country code, empty when unavailable
    i32,                        // UTC offset in seconds
    (bool, CurrentWeatherWire), // current conditions and whether they are present
    Vec<HourForecastWire>,      // hourly forecast
    Vec<DayForecastWire>,       // daily forecast
    Vec<WeatherAlertWire>,      // active alerts
);
pub type WeatherSnapshot = (
    bool,                  // weather data is available
    bool,                  // retained data may be stale
    String,                // unavailable or degraded reason, empty when healthy
    i64,                   // last successful update in Unix microseconds, or 0
    u8,                    // unit system: 0 metric, 1 imperial
    Vec<PlaceWeatherWire>, // watched places
);

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Weather1",
    default_service = "me.aresa.Glimpse.Weather",
    default_path = "/me/aresa/Glimpse/Weather"
)]
pub trait Weather1 {
    #[zbus(property)]
    fn snapshot(&self) -> zbus::Result<WeatherSnapshot>;

    fn watch_place(
        &self,
        kind: u8,       // 0 here, 1 coordinates, 2 named location
        latitude: f64,  // degrees north, ignored for kinds 0 and 2
        longitude: f64, // degrees east, ignored for kinds 0 and 2
        location: &str, // requested name, used only for kind 2
    ) -> zbus::Result<()>;
    fn refresh(&self) -> zbus::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherProviderState {
    pub status: Option<WeatherStatus>,
    pub available: bool,
    pub stale: bool,
    pub reason: Option<String>,
    pub owner: bool,
}

impl WeatherProviderState {
    fn unavailable(reason: impl Into<String>, previous: Option<&Self>) -> Self {
        let status = previous.and_then(|state| state.status.clone());
        let reason = reason.into();
        Self {
            stale: status
                .as_ref()
                .is_some_and(|status| !status.places.is_empty()),
            status,
            available: false,
            reason: Some(clean(&reason, REASON)),
            owner: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WeatherProviderError {
    #[error("weather place is invalid: {0}")]
    InvalidPlace(String),
    #[error("weather place limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("weather provider unavailable: {0}")]
    Unavailable(String),
    #[error("weather provider call timed out")]
    TimedOut,
    #[error("weather provider call failed: {0}")]
    Call(String),
}

#[derive(Clone)]
pub struct WeatherProviderHandle {
    state: watch::Receiver<WeatherProviderState>,
    proxy: Arc<RwLock<Option<Weather1Proxy<'static>>>>,
}

pub struct WeatherProvider {
    handle: WeatherProviderHandle,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl WeatherProvider {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let (_, state) = watch::channel(WeatherProviderState::unavailable(reason, None));
        Self {
            handle: WeatherProviderHandle {
                state,
                proxy: Default::default(),
            },
            task: None,
        }
    }

    pub fn start(connection: zbus::Connection) -> Self {
        let (updates, state) = watch::channel(WeatherProviderState::unavailable(
            "provider has no bus owner",
            None,
        ));
        let proxy = Arc::new(RwLock::new(None));
        let task = tokio::spawn(follow_provider(connection, updates, proxy.clone()));
        Self {
            handle: WeatherProviderHandle { state, proxy },
            task: Some(task),
        }
    }

    pub fn handle(&self) -> WeatherProviderHandle {
        self.handle.clone()
    }

    pub async fn shutdown(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
            tracing::debug!("weather provider follower stopped");
        }
    }
}

impl Drop for WeatherProvider {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl WeatherProviderHandle {
    pub fn snapshot(&self) -> WeatherProviderState {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<WeatherProviderState> {
        self.state.clone()
    }

    async fn proxy(&self) -> Result<Weather1Proxy<'static>, WeatherProviderError> {
        self.proxy.read().await.clone().ok_or_else(|| {
            WeatherProviderError::Unavailable(
                self.state
                    .borrow()
                    .reason
                    .clone()
                    .unwrap_or_else(|| "provider has no bus owner".to_owned()),
            )
        })
    }

    pub async fn watch(&self, place: WatchedPlace) -> Result<(), WeatherProviderError> {
        let (kind, latitude, longitude, location) = place_wire(&place)?;
        tracing::debug!(place_kind = place_kind(&place), "renewing weather watch");
        call(
            self.proxy()
                .await?
                .watch_place(kind, latitude, longitude, &location),
        )
        .await
    }

    pub async fn refresh(&self) -> Result<(), WeatherProviderError> {
        tracing::debug!("requesting weather refresh");
        call(self.proxy().await?.refresh()).await
    }
}

pub fn encode_snapshot(
    status: &WeatherStatus,
    available: bool,
    stale: bool,
    reason: Option<&str>,
) -> WeatherSnapshot {
    (
        available,
        stale,
        reason.unwrap_or_default().to_owned(),
        status
            .updated_at
            .map(|updated| updated.timestamp_micros())
            .unwrap_or_default(),
        unit_code(status.units),
        status.places.iter().map(encode_place).collect(),
    )
}

async fn call(request: impl Future<Output = zbus::Result<()>>) -> Result<(), WeatherProviderError> {
    match tokio::time::timeout(Duration::from_secs(5), request).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(zbus::Error::MethodError(name, reason, _))) => {
            let reason = clean(&reason.unwrap_or_default(), REASON);
            match name.as_str() {
                "me.aresa.Glimpse.Weather1.Error.InvalidPlace" => {
                    Err(WeatherProviderError::InvalidPlace(reason))
                }
                "me.aresa.Glimpse.Weather1.Error.LimitExceeded" => {
                    Err(WeatherProviderError::LimitExceeded(reason))
                }
                "me.aresa.Glimpse.Weather1.Error.Unavailable"
                | "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
                    Err(WeatherProviderError::Unavailable(reason))
                }
                "org.freedesktop.DBus.Error.NoReply" => Err(WeatherProviderError::TimedOut),
                _ => Err(WeatherProviderError::Call(reason)),
            }
        }
        Ok(Err(error)) => Err(WeatherProviderError::Call(clean(
            &error.to_string(),
            REASON,
        ))),
        Err(_) => Err(WeatherProviderError::TimedOut),
    }
}

async fn follow_provider(
    connection: zbus::Connection,
    updates: watch::Sender<WeatherProviderState>,
    current: Arc<RwLock<Option<Weather1Proxy<'static>>>>,
) {
    let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, "cannot observe the weather provider owner");
            return;
        }
    };
    let mut owners = match dbus.receive_name_owner_changed().await {
        Ok(owners) => owners,
        Err(error) => {
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, "cannot observe weather provider owner changes");
            return;
        }
    };

    loop {
        let proxy = match Weather1Proxy::builder(&connection)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
        {
            Ok(proxy) => proxy,
            Err(error) => {
                *current.write().await = None;
                replace_unavailable(&updates, error.to_string());
                tracing::debug!(
                    reason = clean(&error.to_string(), REASON),
                    "weather provider has no owner; waiting"
                );
                if !wait_for_owner(&mut owners).await {
                    return;
                }
                continue;
            }
        };
        let mut snapshots = proxy.receive_snapshot_changed().await;
        let snapshot = match proxy.cached_snapshot() {
            Ok(Some(snapshot)) => Ok(snapshot),
            Ok(None) => proxy.snapshot().await,
            Err(error) => Err(error),
        };
        let state = match snapshot
            .and_then(|snapshot| decode_snapshot(snapshot).map_err(zbus::Error::Failure))
        {
            Ok(state) => state,
            Err(error) => {
                *current.write().await = None;
                let reason = clean(&error.to_string(), REASON);
                replace_unavailable(&updates, &reason);
                tracing::warn!(reason, "weather provider snapshot is unavailable");
                if !wait_for_owner(&mut owners).await {
                    return;
                }
                continue;
            }
        };
        *current.write().await = Some(proxy.clone());
        tracing::info!(
            available = state.available,
            stale = state.stale,
            places = state
                .status
                .as_ref()
                .map_or(0, |status| status.places.len()),
            "weather provider connected"
        );
        updates.send_replace(state);

        let reason = loop {
            tokio::select! {
                changed = snapshots.next() => {
                    let Some(changed) = changed else {
                        break "weather snapshot stream ended".to_owned();
                    };
                    match changed
                        .get()
                        .await
                        .and_then(|snapshot| decode_snapshot(snapshot).map_err(zbus::Error::Failure))
                    {
                        Ok(state) => {
                            tracing::debug!(
                                available = state.available,
                                stale = state.stale,
                                places = state.status.as_ref().map_or(0, |status| status.places.len()),
                                "weather snapshot changed"
                            );
                            updates.send_replace(state);
                        }
                        Err(error) => {
                            let reason = clean(&error.to_string(), REASON);
                            tracing::warn!(reason, "weather snapshot change failed");
                            break reason;
                        }
                    }
                }
                owner = owners.next() => {
                    let Some(owner) = owner else {
                        break "weather provider owner stream ended".to_owned();
                    };
                    let Ok(args) = owner.args() else {
                        continue;
                    };
                    if args.name().as_str() == GLIMPSE_WEATHER_BUS_NAME {
                        break match args.new_owner().is_some() {
                            true => "provider owner changed",
                            false => "provider has no bus owner",
                        }.to_owned();
                    }
                }
            }
        };
        *current.write().await = None;
        replace_unavailable(&updates, &reason);
        tracing::warn!(reason, "weather provider disconnected");
    }
}

fn replace_unavailable(updates: &watch::Sender<WeatherProviderState>, reason: impl Into<String>) {
    let previous = updates.borrow().clone();
    updates.send_replace(WeatherProviderState::unavailable(reason, Some(&previous)));
}

async fn wait_for_owner(owners: &mut zbus::fdo::NameOwnerChangedStream) -> bool {
    while let Some(owner) = owners.next().await {
        let Ok(args) = owner.args() else {
            continue;
        };
        if args.name().as_str() == GLIMPSE_WEATHER_BUS_NAME && args.new_owner().is_some() {
            return true;
        }
    }
    false
}

fn decode_snapshot(snapshot: WeatherSnapshot) -> Result<WeatherProviderState, String> {
    let (available, stale, reason, updated_at, units, places) = snapshot;
    let updated_at = optional_epoch(updated_at)?;
    let reason = clean(&reason, REASON);
    Ok(WeatherProviderState {
        status: Some(WeatherStatus {
            units: decode_units(units)?,
            places: places
                .into_iter()
                .take(MOST_PLACES)
                .map(decode_place)
                .collect::<Result<_, _>>()?,
            updated_at,
        }),
        available,
        stale,
        reason: (!reason.is_empty()).then_some(reason),
        owner: true,
    })
}

fn encode_place(place: &PlaceWeather) -> PlaceWeatherWire {
    let (kind, latitude, longitude, location) = match &place.place {
        WatchedPlace::Here => (0, 0.0, 0.0, String::new()),
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        } => (1, *latitude, *longitude, String::new()),
        WatchedPlace::Location { name } => (2, 0.0, 0.0, name.clone()),
    };
    (
        kind,
        latitude,
        longitude,
        location,
        place.coordinates.latitude,
        place.coordinates.longitude,
        place.city.clone().unwrap_or_default(),
        place.country_code.clone().unwrap_or_default(),
        place.utc_offset_seconds,
        encode_optional_current(place.current.as_ref()),
        place.hours.iter().map(encode_hour).collect(),
        place.days.iter().map(encode_day).collect(),
        place.alerts.iter().map(encode_alert).collect(),
    )
}

fn decode_place(place: PlaceWeatherWire) -> Result<PlaceWeather, String> {
    let (
        kind,
        latitude,
        longitude,
        location,
        resolved_latitude,
        resolved_longitude,
        city,
        country_code,
        utc_offset_seconds,
        current,
        hours,
        days,
        alerts,
    ) = place;
    Ok(PlaceWeather {
        place: decode_place_kind(kind, latitude, longitude, location)?,
        coordinates: GeoCoordinates {
            latitude: resolved_latitude,
            longitude: resolved_longitude,
        },
        city: optional_clean(city, CITY),
        country_code: optional_country_code(country_code),
        utc_offset_seconds,
        current: decode_optional_current(current)?,
        hours: hours
            .into_iter()
            .take(HOURS)
            .map(decode_hour)
            .collect::<Result<_, _>>()?,
        days: days
            .into_iter()
            .take(DAYS)
            .map(decode_day)
            .collect::<Result<_, _>>()?,
        alerts: alerts
            .into_iter()
            .take(MOST_ALERTS)
            .map(decode_alert)
            .collect::<Result<_, _>>()?,
    })
}

fn encode_optional_current(current: Option<&CurrentWeather>) -> (bool, CurrentWeatherWire) {
    match current {
        Some(current) => (true, encode_current(current)),
        None => (false, empty_current()),
    }
}

fn decode_optional_current(
    (present, current): (bool, CurrentWeatherWire),
) -> Result<Option<CurrentWeather>, String> {
    present.then(|| decode_current(current)).transpose()
}

fn encode_current(current: &CurrentWeather) -> CurrentWeatherWire {
    (
        current.observed_at.timestamp_micros(),
        condition_code(current.condition),
        current.is_day,
        current.temperature,
        optional_double(current.apparent_temperature),
        optional_byte(current.humidity),
        optional_double(current.wind_speed),
        optional_u16(current.wind_direction),
        optional_double(current.precipitation),
    )
}

fn decode_current(current: CurrentWeatherWire) -> Result<CurrentWeather, String> {
    let (
        observed_at,
        condition,
        is_day,
        temperature,
        apparent_temperature,
        humidity,
        wind_speed,
        wind_direction,
        precipitation,
    ) = current;
    Ok(CurrentWeather {
        observed_at: epoch(observed_at)?,
        condition: decode_condition(condition),
        is_day,
        temperature,
        apparent_temperature: decode_optional(apparent_temperature),
        humidity: decode_optional(humidity),
        wind_speed: decode_optional(wind_speed),
        wind_direction: decode_optional(wind_direction),
        precipitation: decode_optional(precipitation),
    })
}

fn empty_current() -> CurrentWeatherWire {
    (
        0,
        255,
        false,
        0.0,
        (false, 0.0),
        (false, 0),
        (false, 0.0),
        (false, 0),
        (false, 0.0),
    )
}

fn encode_hour(hour: &HourForecast) -> HourForecastWire {
    (
        hour.time.timestamp_micros(),
        condition_code(hour.condition),
        hour.is_day,
        hour.temperature,
    )
}

fn decode_hour(hour: HourForecastWire) -> Result<HourForecast, String> {
    Ok(HourForecast {
        time: epoch(hour.0)?,
        condition: decode_condition(hour.1),
        is_day: hour.2,
        temperature: hour.3,
    })
}

fn encode_day(day: &DayForecast) -> DayForecastWire {
    (
        day.start.timestamp_micros(),
        condition_code(day.condition),
        day.low,
        day.high,
        optional_byte(day.precipitation_chance),
        optional_timestamp(day.sunrise.as_ref()),
        optional_timestamp(day.sunset.as_ref()),
    )
}

fn decode_day(day: DayForecastWire) -> Result<DayForecast, String> {
    Ok(DayForecast {
        start: epoch(day.0)?,
        condition: decode_condition(day.1),
        low: day.2,
        high: day.3,
        precipitation_chance: decode_optional(day.4),
        sunrise: decode_optional_timestamp(day.5)?,
        sunset: decode_optional_timestamp(day.6)?,
    })
}

fn encode_alert(alert: &WeatherAlert) -> WeatherAlertWire {
    (
        severity_code(alert.severity),
        alert.headline.clone(),
        optional_string(alert.description.as_ref()),
        optional_string(alert.source.as_ref()),
        optional_timestamp(alert.starts_at.as_ref()),
        optional_timestamp(alert.expires_at.as_ref()),
    )
}

fn decode_alert(alert: WeatherAlertWire) -> Result<WeatherAlert, String> {
    Ok(WeatherAlert {
        severity: decode_severity(alert.0),
        headline: clean(&alert.1, HEADLINE),
        description: decode_optional(alert.2).map(|value| clean(&value, DESCRIPTION)),
        source: decode_optional(alert.3).map(|value| clean(&value, SOURCE)),
        starts_at: decode_optional_timestamp(alert.4)?,
        expires_at: decode_optional_timestamp(alert.5)?,
    })
}

fn place_wire(place: &WatchedPlace) -> Result<(u8, f64, f64, String), WeatherProviderError> {
    match place {
        WatchedPlace::Here => Ok((0, 0.0, 0.0, String::new())),
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        } if (-90.0..=90.0).contains(latitude) && (-180.0..=180.0).contains(longitude) => {
            Ok((1, *latitude, *longitude, String::new()))
        }
        WatchedPlace::Coordinates { .. } => Err(WeatherProviderError::InvalidPlace(
            "those coordinates are not on Earth".to_owned(),
        )),
        WatchedPlace::Location { name } if !name.trim().is_empty() => {
            Ok((2, 0.0, 0.0, name.clone()))
        }
        WatchedPlace::Location { .. } => Err(WeatherProviderError::InvalidPlace(
            "location must not be empty".to_owned(),
        )),
    }
}

fn decode_place_kind(
    kind: u8,
    latitude: f64,
    longitude: f64,
    location: String,
) -> Result<WatchedPlace, String> {
    match kind {
        0 => Ok(WatchedPlace::Here),
        1 if (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) => {
            Ok(WatchedPlace::Coordinates {
                latitude,
                longitude,
            })
        }
        1 => Err("weather snapshot contains invalid coordinates".to_owned()),
        2 if !location.trim().is_empty() => Ok(WatchedPlace::Location { name: location }),
        2 => Err("weather snapshot contains empty named location".to_owned()),
        _ => Err(format!(
            "weather snapshot contains unknown place kind {kind}"
        )),
    }
}

fn place_kind(place: &WatchedPlace) -> &'static str {
    match place {
        WatchedPlace::Here => "here",
        WatchedPlace::Coordinates { .. } => "coordinates",
        WatchedPlace::Location { .. } => "location",
    }
}

fn optional_double(value: Option<f64>) -> OptionalDoubleWire {
    value.map_or((false, 0.0), |value| (true, value))
}

fn optional_byte(value: Option<u8>) -> OptionalByteWire {
    value.map_or((false, 0), |value| (true, value))
}

fn optional_u16(value: Option<u16>) -> OptionalU16Wire {
    value.map_or((false, 0), |value| (true, value))
}

fn optional_timestamp(value: Option<&DateTime<Utc>>) -> OptionalTimestampWire {
    value.map_or((false, 0), |value| (true, value.timestamp_micros()))
}

fn optional_string(value: Option<&String>) -> OptionalStringWire {
    value.map_or((false, String::new()), |value| (true, value.clone()))
}

fn decode_optional<T>((present, value): (bool, T)) -> Option<T> {
    present.then_some(value)
}

fn optional_clean(value: String, limit: usize) -> Option<String> {
    let value = clean(&value, limit);
    (!value.is_empty()).then_some(value)
}

fn optional_country_code(value: String) -> Option<String> {
    let value = clean(&value, COUNTRY_CODE);
    (value.len() == COUNTRY_CODE && value.bytes().all(|byte| byte.is_ascii_uppercase()))
        .then_some(value)
}

fn decode_optional_timestamp(
    value: OptionalTimestampWire,
) -> Result<Option<DateTime<Utc>>, String> {
    match value {
        (true, value) => epoch(value).map(Some),
        (false, _) => Ok(None),
    }
}

fn optional_epoch(value: i64) -> Result<Option<DateTime<Utc>>, String> {
    match value {
        0 => Ok(None),
        value => epoch(value).map(Some),
    }
}

fn epoch(value: i64) -> Result<DateTime<Utc>, String> {
    DateTime::from_timestamp_micros(value)
        .ok_or_else(|| format!("weather snapshot contains invalid timestamp {value}"))
}

fn unit_code(units: UnitSystem) -> u8 {
    match units {
        UnitSystem::Metric => 0,
        UnitSystem::Imperial => 1,
    }
}

fn decode_units(units: u8) -> Result<UnitSystem, String> {
    match units {
        0 => Ok(UnitSystem::Metric),
        1 => Ok(UnitSystem::Imperial),
        _ => Err(format!(
            "weather snapshot contains unknown unit system {units}"
        )),
    }
}

fn condition_code(condition: Condition) -> u8 {
    match condition {
        Condition::ClearSky => 0,
        Condition::MainlyClear => 1,
        Condition::PartlyCloudy => 2,
        Condition::Overcast => 3,
        Condition::Fog => 4,
        Condition::Drizzle => 5,
        Condition::FreezingDrizzle => 6,
        Condition::LightRain => 7,
        Condition::Rain => 8,
        Condition::HeavyRain => 9,
        Condition::FreezingRain => 10,
        Condition::LightSnow => 11,
        Condition::Snow => 12,
        Condition::HeavySnow => 13,
        Condition::SnowGrains => 14,
        Condition::Sleet => 15,
        Condition::RainShowers => 16,
        Condition::SnowShowers => 17,
        Condition::Thunderstorm => 18,
        Condition::ThunderstormWithHail => 19,
        Condition::Unknown => 255,
    }
}

fn decode_condition(condition: u8) -> Condition {
    match condition {
        0 => Condition::ClearSky,
        1 => Condition::MainlyClear,
        2 => Condition::PartlyCloudy,
        3 => Condition::Overcast,
        4 => Condition::Fog,
        5 => Condition::Drizzle,
        6 => Condition::FreezingDrizzle,
        7 => Condition::LightRain,
        8 => Condition::Rain,
        9 => Condition::HeavyRain,
        10 => Condition::FreezingRain,
        11 => Condition::LightSnow,
        12 => Condition::Snow,
        13 => Condition::HeavySnow,
        14 => Condition::SnowGrains,
        15 => Condition::Sleet,
        16 => Condition::RainShowers,
        17 => Condition::SnowShowers,
        18 => Condition::Thunderstorm,
        19 => Condition::ThunderstormWithHail,
        _ => Condition::Unknown,
    }
}

fn severity_code(severity: AlertSeverity) -> u8 {
    match severity {
        AlertSeverity::Minor => 0,
        AlertSeverity::Moderate => 1,
        AlertSeverity::Severe => 2,
        AlertSeverity::Extreme => 3,
        AlertSeverity::Unknown => 255,
    }
}

fn decode_severity(severity: u8) -> AlertSeverity {
    match severity {
        0 => AlertSeverity::Minor,
        1 => AlertSeverity::Moderate,
        2 => AlertSeverity::Severe,
        3 => AlertSeverity::Extreme,
        _ => AlertSeverity::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use zbus::zvariant::Type;

    #[test]
    fn wire_signatures_match_the_versioned_contract() {
        assert_eq!(CurrentWeatherWire::SIGNATURE, "(xybd(bd)(by)(bd)(bq)(bd))");
        assert_eq!(HourForecastWire::SIGNATURE, "(xybd)");
        assert_eq!(DayForecastWire::SIGNATURE, "(xydd(by)(bx)(bx))");
        assert_eq!(WeatherAlertWire::SIGNATURE, "(ys(bs)(bs)(bx)(bx))");
        assert_eq!(
            PlaceWeatherWire::SIGNATURE,
            "(yddsddssi(b(xybd(bd)(by)(bd)(bq)(bd)))a(xybd)a(xydd(by)(bx)(bx))a(ys(bs)(bs)(bx)(bx)))"
        );
        assert_eq!(
            WeatherSnapshot::SIGNATURE,
            "(bbsxya(yddsddssi(b(xybd(bd)(by)(bd)(bq)(bd)))a(xybd)a(xydd(by)(bx)(bx))a(ys(bs)(bs)(bx)(bx))))"
        );
    }

    #[test]
    fn snapshots_round_trip_without_losing_domain_values() {
        let updated_at = Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap();
        let status = WeatherStatus {
            units: UnitSystem::Metric,
            places: vec![PlaceWeather {
                place: WatchedPlace::Location {
                    name: "Vilnius, LT".to_owned(),
                },
                coordinates: GeoCoordinates {
                    latitude: 54.7,
                    longitude: 25.3,
                },
                city: Some("Vilnius".to_owned()),
                country_code: Some("LT".to_owned()),
                utc_offset_seconds: 7_200,
                current: Some(CurrentWeather {
                    observed_at: updated_at,
                    condition: Condition::Rain,
                    is_day: true,
                    temperature: 12.5,
                    apparent_temperature: Some(11.0),
                    humidity: Some(80),
                    wind_speed: None,
                    wind_direction: Some(270),
                    precipitation: Some(0.4),
                }),
                hours: Vec::new(),
                days: Vec::new(),
                alerts: Vec::new(),
            }],
            updated_at: Some(updated_at),
        };

        let decoded = decode_snapshot(encode_snapshot(&status, true, false, None)).unwrap();

        assert_eq!(decoded.status, Some(status));
        assert!(decoded.available);
        assert!(!decoded.stale);
        assert!(decoded.owner);
    }

    #[test]
    fn untrusted_snapshot_text_and_collections_are_bounded() {
        let timestamp = 1_789_382_400_000_000;
        let alert: WeatherAlertWire = (
            2,
            "Storm\u{202e}gpj.exe".to_owned(),
            (true, "  line\tone\nline two  ".to_owned()),
            (true, format!("{}\u{202e}", "s".repeat(SOURCE + 10))),
            (false, 0),
            (false, 0),
        );
        let place: PlaceWeatherWire = (
            0,
            0.0,
            0.0,
            String::new(),
            54.7,
            25.3,
            format!("{}\u{202e}", "c".repeat(CITY + 10)),
            format!("{}\u{202e}", "p".repeat(COUNTRY_CODE + 10)),
            7_200,
            (false, empty_current()),
            vec![(timestamp, 0, true, 12.0); HOURS + 1],
            vec![(timestamp, 0, 5.0, 12.0, (false, 0), (false, 0), (false, 0)); DAYS + 1],
            vec![alert; MOST_ALERTS + 1],
        );
        let decoded = decode_snapshot((
            true,
            false,
            format!("{}\u{202e}", "r".repeat(REASON + 10)),
            timestamp,
            0,
            vec![place; MOST_PLACES + 1],
        ))
        .unwrap();

        assert_eq!(
            decoded.reason.as_deref(),
            Some(format!("{}…", "r".repeat(REASON)).as_str())
        );
        let status = decoded.status.unwrap();
        assert_eq!(status.places.len(), MOST_PLACES);
        let place = &status.places[0];
        assert_eq!(
            place.city.as_deref(),
            Some(format!("{}…", "c".repeat(CITY)).as_str())
        );
        assert_eq!(place.country_code, None);
        assert_eq!(place.hours.len(), HOURS);
        assert_eq!(place.days.len(), DAYS);
        assert_eq!(place.alerts.len(), MOST_ALERTS);
        assert_eq!(place.alerts[0].headline, "Storm gpj.exe");
        assert_eq!(
            place.alerts[0].description.as_deref(),
            Some("line one line two")
        );
        assert_eq!(
            place.alerts[0].source.as_deref(),
            Some(format!("{}…", "s".repeat(SOURCE)).as_str())
        );
    }
}
