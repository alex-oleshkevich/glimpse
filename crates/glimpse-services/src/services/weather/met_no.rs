use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

use chrono::{
    DateTime, FixedOffset, NaiveDate, NaiveTime, Offset as _, TimeZone, Timelike as _, Utc,
};
use glimpse_dbus::weather::{
    AlertSeverity, Condition, CurrentWeather, DayForecast, GeoCoordinates, HourForecast,
    UnitSystem, WeatherAlert,
};
use serde::Deserialize;
use tzf_rs::DefaultFinder;

use super::{Ask, HOURS, Reading, bearing, fetch_json, hour_floor, percent};

const MET_NO_FORECAST: &str = "https://api.met.no/weatherapi/locationforecast/2.0/complete";
const MET_NO_ALERTS: &str = "https://api.met.no/weatherapi/metalerts/2.0/current.json";

/// met.no's terms ask for coordinates truncated to four decimals, which is also what keeps their
/// cache from being keyed on noise.
const MET_NO_PRECISION: usize = 4;

pub(super) async fn met_no(client: reqwest::Client, ask: Ask) -> Result<Vec<Reading>, String> {
    let now = Utc::now();
    let mut readings = Vec::with_capacity(ask.coordinates.len());

    for pair in &ask.coordinates {
        let forecast: MetNoForecast = met_no_json(&client, MET_NO_FORECAST, pair).await?;
        let warnings: MetNoAlerts = met_no_json(&client, MET_NO_ALERTS, pair)
            .await
            .unwrap_or_default();
        let located = pair.clone();
        let seconds = match tokio::task::spawn_blocking(move || met_no_offset(&located, now)).await
        {
            Ok(seconds) => seconds,
            Err(error) => {
                tracing::warn!("timezone lookup failed: {error}");
                0
            }
        };
        readings.push(met_no_reading(&forecast, &warnings, &ask, now, seconds));
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

    fetch_json(client, url).await
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
    zone_name(pair)
        .parse::<chrono_tz::Tz>()
        .map(|zone| now.with_timezone(&zone).offset().fix().local_minus_utc())
        .unwrap_or(0)
}

fn zone_name(pair: &GeoCoordinates) -> String {
    let key = (pair.latitude.to_bits(), pair.longitude.to_bits());
    if let Some(name) = zone_names()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&key).cloned())
    {
        return name;
    }

    unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, 65_536) };
    let name = {
        let finder = DefaultFinder::new();
        finder.get_tz_name(pair.longitude, pair.latitude).to_owned()
    };
    unsafe { libc::malloc_trim(0) };
    if let Ok(mut guard) = zone_names().lock() {
        guard.entry(key).or_insert(name.clone());
    }
    name
}

fn zone_names() -> &'static Mutex<HashMap<(u64, u64), String>> {
    static NAMES: OnceLock<Mutex<HashMap<(u64, u64), String>>> = OnceLock::new();
    NAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn met_no_reading(
    forecast: &MetNoForecast,
    warnings: &MetNoAlerts,
    ask: &Ask,
    now: DateTime<Utc>,
    seconds: i32,
) -> Reading {
    let offset = FixedOffset::east_opt(seconds).unwrap_or_else(|| Utc.fix());
    let entries = &forecast.properties.timeseries;

    Reading {
        utc_offset_seconds: seconds,
        current: met_no_current(entries.first(), ask.units),
        hours: met_no_hours(entries, ask.units, now),
        days: met_no_days(entries, offset, ask.units),
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
        humidity: percent(details.relative_humidity),
        wind_speed: details.wind_speed.map(|speed| met_no_wind(speed, units)),
        wind_direction: bearing(details.wind_from_direction),
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
fn met_no_days(entries: &[MetNoEntry], offset: FixedOffset, units: UnitSystem) -> Vec<DayForecast> {
    let mut grouped: BTreeMap<NaiveDate, Vec<&MetNoEntry>> = BTreeMap::new();
    for entry in entries {
        grouped
            .entry(entry.time.with_timezone(&offset).date_naive())
            .or_default()
            .push(entry);
    }

    grouped
        .into_iter()
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
                    .filter_map(|entry| {
                        percent(entry.period()?.details.probability_of_precipitation)
                    })
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
mod tests;
