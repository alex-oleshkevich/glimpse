use chrono::{DateTime, TimeZone as _, Utc};
use glimpse_dbus::weather::{
    Condition, CurrentWeather, DayForecast, GeoCoordinates, HourForecast, UnitSystem,
};
use serde::Deserialize;

use super::{Ask, HOURS, Reading, bearing, fetch_json, hour_floor, percent};

const ENDPOINT: &str = "https://api.open-meteo.com/v1/forecast";

const CURRENT: &str = "temperature_2m,apparent_temperature,relative_humidity_2m,precipitation,weather_code,wind_speed_10m,wind_direction_10m,is_day";
const HOURLY: &str = "temperature_2m,weather_code,is_day";
const DAILY: &str =
    "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max";

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

pub(super) async fn open_meteo(client: reqwest::Client, ask: Ask) -> Result<Vec<Reading>, String> {
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

    let payload: Payload = fetch_json(&client, url).await?;

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
        humidity: percent(block.relative_humidity_2m),
        wind_speed: block.wind_speed_10m,
        wind_direction: bearing(block.wind_direction_10m),
        precipitation: block.precipitation,
    })
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

#[cfg(test)]
mod tests;
