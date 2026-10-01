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

    let days = met_no_days(&forecast.properties.timeseries, offset, UnitSystem::Metric);

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

    let days = met_no_days(&forecast.properties.timeseries, offset, UnitSystem::Metric);

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
