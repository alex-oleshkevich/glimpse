use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use glimpse_contracts::{
    AlertSeverity, Condition, CurrentWeather, DayForecast, GeoCoordinates, HourForecast,
    PlaceWeather, UnitSystem, WatchedPlace, WeatherAlert, WeatherStatus,
};
use tokio::sync::{RwLock, watch};
use zbus::proxy::CacheProperties;

pub const GLIMPSE_WEATHER_BUS_NAME: &str = "me.aresa.Glimpse.Weather";
pub const GLIMPSE_WEATHER_OBJECT_PATH: &str = "/me/aresa/Glimpse/Weather";

pub type OptionalDoubleWire = (bool, f64);
pub type OptionalByteWire = (bool, u8);
pub type OptionalU16Wire = (bool, u16);
pub type OptionalTimestampWire = (bool, i64);
pub type OptionalStringWire = (bool, String);
pub type CurrentWeatherWire = (
    i64,
    u8,
    bool,
    f64,
    OptionalDoubleWire,
    OptionalByteWire,
    OptionalDoubleWire,
    OptionalU16Wire,
    OptionalDoubleWire,
);
pub type HourForecastWire = (i64, u8, bool, f64);
pub type DayForecastWire = (
    i64,
    u8,
    f64,
    f64,
    OptionalByteWire,
    OptionalTimestampWire,
    OptionalTimestampWire,
);
pub type WeatherAlertWire = (
    u8,
    String,
    OptionalStringWire,
    OptionalStringWire,
    OptionalTimestampWire,
    OptionalTimestampWire,
);
pub type PlaceWeatherWire = (
    u8,
    f64,
    f64,
    f64,
    f64,
    i32,
    (bool, CurrentWeatherWire),
    Vec<HourForecastWire>,
    Vec<DayForecastWire>,
    Vec<WeatherAlertWire>,
);
pub type WeatherSnapshot = (bool, bool, String, i64, u8, Vec<PlaceWeatherWire>);

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Weather1",
    default_service = "me.aresa.Glimpse.Weather",
    default_path = "/me/aresa/Glimpse/Weather"
)]
pub trait Weather1 {
    #[zbus(property)]
    fn snapshot(&self) -> zbus::Result<WeatherSnapshot>;

    fn watch_place(&self, kind: u8, latitude: f64, longitude: f64) -> zbus::Result<()>;
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
        Self {
            stale: status
                .as_ref()
                .is_some_and(|status| !status.places.is_empty()),
            status,
            available: false,
            reason: Some(reason.into()),
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
        let (kind, latitude, longitude) = place_wire(&place)?;
        tracing::debug!(place_kind = place_kind(&place), "renewing weather watch");
        call(self.proxy().await?.watch_place(kind, latitude, longitude)).await
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
            let reason = reason.unwrap_or_default();
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
        Ok(Err(error)) => Err(WeatherProviderError::Call(error.to_string())),
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
                tracing::debug!(%error, "weather provider has no owner; waiting");
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
                replace_unavailable(&updates, error.to_string());
                *current.write().await = None;
                tracing::warn!(%error, "weather provider snapshot is unavailable");
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

        loop {
            tokio::select! {
                changed = snapshots.next() => {
                    let Some(changed) = changed else {
                        break;
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
                            replace_unavailable(&updates, error.to_string());
                            tracing::warn!(%error, "weather snapshot change failed");
                            break;
                        }
                    }
                }
                owner = owners.next() => {
                    let Some(owner) = owner else {
                        break;
                    };
                    let Ok(args) = owner.args() else {
                        continue;
                    };
                    if args.name().as_str() == GLIMPSE_WEATHER_BUS_NAME {
                        break;
                    }
                }
            }
        }
        *current.write().await = None;
        replace_unavailable(&updates, "provider has no bus owner");
        tracing::warn!("weather provider owner disappeared");
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
    Ok(WeatherProviderState {
        status: Some(WeatherStatus {
            units: decode_units(units)?,
            places: places
                .into_iter()
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
    let (kind, latitude, longitude) = match place.place {
        WatchedPlace::Here => (0, 0.0, 0.0),
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        } => (1, latitude, longitude),
    };
    (
        kind,
        latitude,
        longitude,
        place.coordinates.latitude,
        place.coordinates.longitude,
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
        resolved_latitude,
        resolved_longitude,
        utc_offset_seconds,
        current,
        hours,
        days,
        alerts,
    ) = place;
    Ok(PlaceWeather {
        place: decode_place_kind(kind, latitude, longitude)?,
        coordinates: GeoCoordinates {
            latitude: resolved_latitude,
            longitude: resolved_longitude,
        },
        utc_offset_seconds,
        current: decode_optional_current(current)?,
        hours: hours
            .into_iter()
            .map(decode_hour)
            .collect::<Result<_, _>>()?,
        days: days.into_iter().map(decode_day).collect::<Result<_, _>>()?,
        alerts: alerts
            .into_iter()
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
        headline: alert.1,
        description: decode_optional(alert.2),
        source: decode_optional(alert.3),
        starts_at: decode_optional_timestamp(alert.4)?,
        expires_at: decode_optional_timestamp(alert.5)?,
    })
}

fn place_wire(place: &WatchedPlace) -> Result<(u8, f64, f64), WeatherProviderError> {
    match place {
        WatchedPlace::Here => Ok((0, 0.0, 0.0)),
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        } if (-90.0..=90.0).contains(latitude) && (-180.0..=180.0).contains(longitude) => {
            Ok((1, *latitude, *longitude))
        }
        WatchedPlace::Coordinates { .. } => Err(WeatherProviderError::InvalidPlace(
            "those coordinates are not on Earth".to_owned(),
        )),
    }
}

fn decode_place_kind(kind: u8, latitude: f64, longitude: f64) -> Result<WatchedPlace, String> {
    match kind {
        0 => Ok(WatchedPlace::Here),
        1 if (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) => {
            Ok(WatchedPlace::Coordinates {
                latitude,
                longitude,
            })
        }
        1 => Err("weather snapshot contains invalid coordinates".to_owned()),
        _ => Err(format!(
            "weather snapshot contains unknown place kind {kind}"
        )),
    }
}

fn place_kind(place: &WatchedPlace) -> &'static str {
    match place {
        WatchedPlace::Here => "here",
        WatchedPlace::Coordinates { .. } => "coordinates",
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
            "(yddddi(b(xybd(bd)(by)(bd)(bq)(bd)))a(xybd)a(xydd(by)(bx)(bx))a(ys(bs)(bs)(bx)(bx)))"
        );
        assert_eq!(
            WeatherSnapshot::SIGNATURE,
            "(bbsxya(yddddi(b(xybd(bd)(by)(bd)(bq)(bd)))a(xybd)a(xydd(by)(bx)(bx))a(ys(bs)(bs)(bx)(bx))))"
        );
    }

    #[test]
    fn snapshots_round_trip_without_losing_domain_values() {
        let updated_at = Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap();
        let status = WeatherStatus {
            units: UnitSystem::Metric,
            places: vec![PlaceWeather {
                place: WatchedPlace::Here,
                coordinates: GeoCoordinates {
                    latitude: 54.7,
                    longitude: 25.3,
                },
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
}
