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

#[test]
fn one_place_and_several_places_both_decode() {
    assert_eq!(decoded(one_place()).len(), 1);
    assert_eq!(decoded(&many_places()).len(), 2);
}

fn moment_of(epoch: i64) -> DateTime<Utc> {
    moment(epoch).expect("a representable instant")
}

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

#[test]
fn units_choose_the_triple_that_goes_together() {
    assert_eq!(units(UnitSystem::Metric), ("celsius", "kmh", "mm"));
    assert_eq!(units(UnitSystem::Imperial), ("fahrenheit", "mph", "inch"));
}

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
