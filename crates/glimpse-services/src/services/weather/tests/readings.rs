use super::*;

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
