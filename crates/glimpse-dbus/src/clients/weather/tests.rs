use super::*;
use chrono::TimeZone as _;
use zbus::zvariant::Type;

const EVERY: [Condition; 21] = [
    Condition::ClearSky,
    Condition::MainlyClear,
    Condition::PartlyCloudy,
    Condition::Overcast,
    Condition::Fog,
    Condition::Drizzle,
    Condition::FreezingDrizzle,
    Condition::LightRain,
    Condition::Rain,
    Condition::HeavyRain,
    Condition::FreezingRain,
    Condition::LightSnow,
    Condition::Snow,
    Condition::HeavySnow,
    Condition::SnowGrains,
    Condition::Sleet,
    Condition::RainShowers,
    Condition::SnowShowers,
    Condition::Thunderstorm,
    Condition::ThunderstormWithHail,
    Condition::Unknown,
];

#[test]
fn every_condition_maps_to_a_day_and_a_night_icon() {
    for condition in EVERY {
        for is_day in [true, false] {
            let name = condition.icon_name(is_day);
            assert!(
                name.ends_with("-symbolic") && name.len() > "-symbolic".len(),
                "{condition:?} at is_day={is_day} named {name:?}"
            );
        }
    }
}

#[test]
fn the_bar_and_the_hero_round_the_same_way() {
    for value in [18.4, 18.5, -0.4, -3.6, 0.0] {
        assert_eq!(reading(value), format!("{}{DEGREE}", rounded(value)));
    }
    assert_eq!(rounded(18.5), "19");
    assert_eq!(rounded(-0.4), "0");
}

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
fn a_requested_location_name_is_bounded_like_the_resolved_one() {
    let decoded = decode_place_kind(2, 0.0, 0.0, format!("{}\u{202e}", "n".repeat(CITY + 10)))
        .expect("a named place");

    assert_eq!(
        decoded,
        WatchedPlace::Location {
            name: format!("{}…", "n".repeat(CITY))
        }
    );
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
