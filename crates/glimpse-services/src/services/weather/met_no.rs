use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{
    DateTime, FixedOffset, NaiveDate, NaiveTime, Offset as _, TimeZone, Timelike as _, Utc,
};
use glimpse_contracts::{
    AlertSeverity, Condition, CurrentWeather, DayForecast, GeoCoordinates, HourForecast,
    UnitSystem, WeatherAlert,
};
use serde::Deserialize;
use tzf_rs::DefaultFinder;

use super::{Ask, HOURS, Reading, hour_floor, transport};

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
    use super::*;

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
}
