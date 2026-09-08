use chrono::{DateTime, TimeZone as _, Utc};
use glimpse_contracts::{
    Condition, CurrentWeather, DayForecast, GeoCoordinates, HourForecast, UnitSystem,
};
use serde::Deserialize;

use super::{Ask, HOURS, Reading, bearing, hour_floor, humidity, transport};

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

    let response = client.get(url).send().await.map_err(transport)?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("the provider answered {status}"));
    }

    let body = response.text().await.map_err(transport)?;
    let payload: Payload = serde_json::from_str(&body)
        .map_err(|_| "the provider answered something else".to_owned())?;

    Ok(readings(payload.forecasts(), Utc::now(), ask.forecast_days))
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

fn readings(forecasts: Vec<Forecast>, now: DateTime<Utc>, cap: u8) -> Vec<Reading> {
    forecasts
        .into_iter()
        .map(|forecast| Reading {
            utc_offset_seconds: forecast.utc_offset_seconds,
            current: forecast.current.and_then(current),
            hours: hours(forecast.hourly.unwrap_or_default(), now),
            days: days(forecast.daily.unwrap_or_default(), cap),
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
        humidity: humidity(block.relative_humidity_2m),
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

/// The provider is asked for `forecast_days` and nothing downstream re-checks it, so the ask is
/// enforced here as well: days are chronological, which is what makes taking the first `cap` of
/// them mean the same thing as asking for `cap` of them.
fn days(block: DailyBlock, cap: u8) -> Vec<DayForecast> {
    block
        .time
        .iter()
        .enumerate()
        .take(cap as usize)
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
mod tests {
    use super::*;

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
        let read = readings(decoded(one_place()), moment_of(1757262000), 7);

        let hours = &read[0].hours;
        assert_eq!(hours[0].time, moment_of(1757260800));
        assert_eq!(hours[0].temperature, 18.4);
        assert!(hours[0].is_day);
    }

    #[test]
    fn an_hour_the_provider_left_null_is_skipped_rather_than_zeroed() {
        let read = readings(decoded(one_place()), moment_of(1757262000), 7);

        let times: Vec<i64> = read[0]
            .hours
            .iter()
            .map(|hour| hour.time.timestamp())
            .collect();
        assert_eq!(times, vec![1757260800]);
    }

    #[test]
    fn an_hour_before_now_is_not_published() {
        let read = readings(decoded(one_place()), moment_of(1757264400), 7);

        assert!(
            read[0]
                .hours
                .iter()
                .all(|hour| hour.time.timestamp() >= 1757264400)
        );
    }

    #[test]
    fn a_day_carries_its_span_and_its_chance() {
        let read = readings(decoded(one_place()), moment_of(1757262000), 7);

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

    /// Nothing downstream trims the day list: `absorb` copies it onto the payload as it stands and
    /// the panel renders what arrives. met.no caps its own aggregation by the same ask, so a
    /// provider answering with more days than it was asked for is capped on both paths.
    #[test]
    fn the_ask_caps_the_days_published() {
        let week = r#"{"latitude": 47.375, "longitude": 8.5, "utc_offset_seconds": 0,
                       "daily": {"time": [1757196000, 1757282400, 1757368800, 1757455200],
                                 "weather_code": [0, 1, 2, 3],
                                 "temperature_2m_max": [21.0, 22.0, 23.0, 24.0],
                                 "temperature_2m_min": [12.0, 13.0, 14.0, 15.0],
                                 "precipitation_probability_max": [10, 20, 30, 40]}}"#;

        let read = readings(decoded(week), moment_of(1757262000), 2);

        let days = &read[0].days;
        assert_eq!(days.len(), 2, "the ask caps the days, not the response");
        assert_eq!(
            days[0].start,
            moment_of(1757196000),
            "the cap keeps the first days rather than an arbitrary two"
        );
    }

    #[test]
    fn the_current_block_carries_the_provider_s_own_observation_time() {
        let read = readings(decoded(one_place()), moment_of(1757262000), 7);

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

        let read = readings(decoded(sparse), moment_of(1757262000), 7);
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

        let read = readings(decoded(calm), moment_of(1757262000), 7);
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

    #[test]
    fn units_choose_the_triple_that_goes_together() {
        assert_eq!(units(UnitSystem::Metric), ("celsius", "kmh", "mm"));
        assert_eq!(units(UnitSystem::Imperial), ("fahrenheit", "mph", "inch"));
    }

    /// Only a missing temperature is worth dropping an hour over. A missing code reads as unknown
    /// and a missing day flag as daylight; dropping the hour instead loses the whole strip when a
    /// provider stops sending one companion array.
    #[test]
    fn an_hour_missing_only_its_companions_is_still_published() {
        let sparse = r#"{"latitude": 47.375, "longitude": 8.5, "utc_offset_seconds": 0,
                         "hourly": {"time": [1757260800], "temperature_2m": [18.4]}}"#;

        let read = readings(decoded(sparse), moment_of(1757262000), 7);

        let hour = &read[0].hours[0];
        assert_eq!(hour.temperature, 18.4);
        assert_eq!(hour.condition, Condition::Unknown);
        assert!(hour.is_day);
    }
}
