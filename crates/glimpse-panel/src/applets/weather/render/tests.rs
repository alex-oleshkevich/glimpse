use chrono::TimeZone as _;
use glimpse_dbus::weather::{GeoCoordinates, HourForecast, WatchedPlace};

use super::*;

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 8, hour, minute, 0)
        .single()
        .expect("a real instant")
}

fn current(condition: Condition) -> CurrentWeather {
    CurrentWeather {
        observed_at: at(12, 0),
        condition,
        is_day: true,
        temperature: 18.4,
        apparent_temperature: Some(16.2),
        humidity: Some(72),
        wind_speed: Some(11.7),
        wind_direction: Some(230),
        precipitation: Some(0.4),
    }
}

fn today() -> DayForecast {
    DayForecast {
        start: at(0, 0),
        condition: Condition::LightRain,
        low: 11.6,
        high: 18.4,
        precipitation_chance: Some(60),
        sunrise: Some(at(4, 30)),
        sunset: Some(at(17, 15)),
    }
}

fn hour(minutes: i64, condition: Condition) -> HourForecast {
    HourForecast {
        time: at(12, 0) + TimeDelta::minutes(minutes),
        condition,
        is_day: true,
        temperature: 17.0,
    }
}

fn an_alert() -> WeatherAlert {
    WeatherAlert {
        severity: AlertSeverity::Severe,
        headline: "Thunderstorm warning until 21:00".to_owned(),
        description: Some("Hail and gusts to 25 m/s.".to_owned()),
        source: Some("LHMT".to_owned()),
        starts_at: Some(at(12, 0)),
        expires_at: Some(at(18, 0)),
    }
}

fn place() -> PlaceWeather {
    PlaceWeather {
        place: WatchedPlace::Here,
        coordinates: GeoCoordinates {
            latitude: 54.6872,
            longitude: 25.2797,
        },
        city: None,
        country_code: None,
        utc_offset_seconds: 10_800,
        current: Some(current(Condition::PartlyCloudy)),
        hours: Vec::new(),
        days: vec![today()],
        alerts: Vec::new(),
    }
}

fn labels(facts: &[Fact]) -> Vec<String> {
    facts.iter().map(|fact| fact.label.clone()).collect()
}

fn value(facts: &[Fact], label: &str) -> Option<String> {
    facts
        .iter()
        .find(|fact| fact.label == label)
        .map(|fact| fact.value.clone())
}

#[test]
fn an_unknown_condition_still_renders() {
    assert!(!Condition::Unknown.icon_name(true).is_empty());
    assert_eq!(
        wording(Condition::Unknown),
        None,
        "a condition with no wording drops the clause rather than inventing one"
    );

    let mut unknown = current(Condition::Unknown);
    unknown.apparent_temperature = None;
    assert_eq!(subtitle(&unknown), None);
}

#[test]
fn the_nowcast_names_the_first_wet_hour_and_nothing_when_it_is_already_wet() {
    let mut dry = place();
    dry.hours = vec![
        hour(0, Condition::PartlyCloudy),
        hour(25, Condition::LightRain),
        hour(85, Condition::HeavyRain),
    ];
    let coming = nowcast(&dry, at(12, 0)).expect("rain is coming");
    assert_eq!(coming.title, "Rain starting in 25 minutes");
    assert_eq!(coming.subtitle.as_deref(), Some("Light rain"));
    assert_eq!(
        coming.page, None,
        "a nowcast states something and leads nowhere"
    );

    let mut raining = dry.clone();
    raining.current = Some(current(Condition::Rain));
    assert!(
        nowcast(&raining, at(12, 0)).is_none(),
        "being rained on already is not news"
    );

    let mut clear = place();
    clear.hours = vec![hour(0, Condition::ClearSky), hour(60, Condition::Overcast)];
    assert!(nowcast(&clear, at(12, 0)).is_none());
}

#[test]
fn a_nowcast_an_hour_or_more_away_is_told_in_hours() {
    let title = |minutes| {
        let mut dry = place();
        dry.hours = vec![
            hour(0, Condition::PartlyCloudy),
            hour(minutes, Condition::LightRain),
        ];
        nowcast(&dry, at(12, 0)).expect("rain is coming").title
    };
    assert_eq!(title(59), "Rain starting in 59 minutes");
    assert_eq!(title(60), "Rain starting in 1 hour");
    assert_eq!(title(146), "Rain starting in 2 hours");
    assert_eq!(title(165), "Rain starting in 3 hours");
}

#[test]
fn the_nowcast_ignores_an_hour_beyond_its_window() {
    let mut later = place();
    later.hours = vec![
        hour(0, Condition::PartlyCloudy),
        hour(NOWCAST + 1, Condition::Rain),
    ];
    assert!(
        nowcast(&later, at(12, 0)).is_none(),
        "otherwise the popover announces tomorrow's rain"
    );

    later.hours = vec![hour(NOWCAST, Condition::Rain)];
    assert!(
        nowcast(&later, at(12, 0)).is_some(),
        "the window is inclusive"
    );
}

#[test]
fn the_nowcast_names_snow_as_snow() {
    let mut snowing = place();
    snowing.hours = vec![hour(30, Condition::HeavySnow)];
    let coming = nowcast(&snowing, at(12, 0)).expect("snow is coming");
    assert!(coming.title.starts_with("Snow"), "{}", coming.title);
}

#[test]
fn day_length_is_the_span_between_sunrise_and_sunset() {
    assert_eq!(day_length(&today()).as_deref(), Some("12 h 45 min"));

    let mut polar = today();
    polar.sunrise = None;
    polar.sunset = None;
    assert_eq!(
        day_length(&polar),
        None,
        "a day carrying neither prints no length rather than a zero one"
    );

    let mut backwards = today();
    backwards.sunset = backwards.sunrise;
    assert_eq!(day_length(&backwards), None);
}

/// `WeatherStatus.units` says which system the numbers already are in, so the page prints
/// what the payload declares rather than what a configuration says one round trip later.
#[test]
fn a_day_page_prints_the_units_the_payload_declares() {
    let mut week = place();
    week.days = vec![today(), today()];

    let page = |units| {
        pages(&week, units, 7, false)
            .into_iter()
            .find(|page| page.key == "day1")
            .expect("tomorrow has a page")
            .facts
    };

    assert_eq!(
        value(&page(UnitSystem::Metric), "High").as_deref(),
        Some("18 °C")
    );
    assert_eq!(
        value(&page(UnitSystem::Metric), "Low").as_deref(),
        Some("12 °C")
    );
    assert_eq!(
        value(&page(UnitSystem::Imperial), "High").as_deref(),
        Some("18 °F")
    );
}

#[test]
fn a_fact_the_provider_omitted_is_left_out_rather_than_printed_as_zero() {
    let mut sparse = place();
    let bare = DayForecast {
        precipitation_chance: None,
        sunrise: None,
        sunset: None,
        ..today()
    };
    sparse.days = vec![bare.clone(), bare];

    let printed = labels(
        &pages(&sparse, UnitSystem::Metric, 7, false)
            .into_iter()
            .find(|page| page.key == "day1")
            .expect("tomorrow has a page")
            .facts,
    );

    for absent in ["Chance of rain", "Sunrise", "Sunset", "Day length"] {
        assert!(
            !printed.iter().any(|label| label == absent),
            "{absent} was printed anyway"
        );
    }
    assert_eq!(printed, ["High", "Low"], "what is known still prints");
}

#[test]
fn hours_and_days_are_capped_by_the_configuration() {
    let mut long = place();
    long.hours = (0..24).map(|n| hour(n * 60, Condition::ClearSky)).collect();
    long.days = (0..10).map(|_| today()).collect();

    assert_eq!(hours(&long, 4, false).len(), 4);
    assert_eq!(days(&long, 7).len(), 8, "today, then the configured count");
    assert_eq!(hours(&long, 0, false).len(), 0);
    assert_eq!(
        days(&long, 30).len(),
        10,
        "a cap wider than the payload takes what is there"
    );
    assert_eq!(
        pages(&long, UnitSystem::Metric, 7, false).len(),
        8,
        "one page per shown day, and nothing that is not a row you can see"
    );
}

#[test]
fn the_day_list_leads_with_today_and_marks_the_current_reading() {
    let mut week = place();
    week.days = (0..8)
        .map(|n| DayForecast {
            start: at(0, 0) + TimeDelta::days(n),
            high: 10.0 + n as f64,
            ..today()
        })
        .collect();

    let listed = days(&week, 6);
    assert_eq!(listed.len(), 7);
    assert_eq!(listed[0].label, "Today");
    assert_eq!(listed[1].label, "Tomorrow");
    assert_eq!(listed[0].high, 10.0);
    assert_eq!(
        listed[0].now,
        Some(18.4),
        "today carries the hero's reading"
    );
    assert!(
        listed[1..].iter().all(|day| day.now.is_none()),
        "only today has a now"
    );

    week.current = None;
    assert_eq!(days(&week, 6)[0].now, None);
}

#[test]
fn today_opens_on_the_current_conditions_and_the_sun() {
    let built = pages(&place(), UnitSystem::Metric, 6, false);
    let today = built
        .iter()
        .find(|page| page.key == "day0")
        .expect("today has a page");
    assert_eq!(today.title, "Today");
    assert_eq!(
        labels(&today.facts),
        [
            "Wind",
            "Humidity",
            "Precipitation",
            "Sunrise",
            "Sunset",
            "Day length"
        ]
    );
    assert_eq!(value(&today.facts, "Wind").as_deref(), Some("12 km/h SW"));
    assert_eq!(value(&today.facts, "Humidity").as_deref(), Some("72%"));
    assert_eq!(
        value(&today.facts, "Precipitation").as_deref(),
        Some("0.4 mm")
    );

    let imperial = pages(&place(), UnitSystem::Imperial, 6, false);
    let facts = &imperial[0].facts;
    assert_eq!(value(facts, "Wind").as_deref(), Some("12 mph SW"));
    assert_eq!(value(facts, "Precipitation").as_deref(), Some("0.40 in"));
}

#[test]
fn a_wind_direction_rounds_to_the_nearest_of_eight_points() {
    for (degrees, point) in [
        (0, "N"),
        (22, "N"),
        (23, "NE"),
        (180, "S"),
        (315, "NW"),
        (338, "N"),
        (359, "N"),
    ] {
        assert_eq!(compass(degrees), point, "{degrees}°");
    }
}

/// The hour standing is the hero's job. A column repeating it is the second telling, and it
/// costs one of the hours the strip exists to look ahead at.
#[test]
fn the_strip_starts_at_the_next_hour() {
    let mut strip = place();
    strip.hours = vec![
        hour(0, Condition::ClearSky),
        hour(60, Condition::ClearSky),
        hour(120, Condition::ClearSky),
    ];
    let rendered = hours(&strip, 4, false);

    assert_eq!(rendered.len(), 2, "the hour standing is not one of them");
    assert_eq!(
        rendered[0].label, "16:00",
        "the first column is the next hour, read in the place's own zone rather than the \
             panel's"
    );
    assert_eq!(rendered[1].label, "17:00");
    assert!(
        rendered.iter().all(|hour| !hour.now),
        "no column is the hour standing, so none of them is marked as it"
    );
}

#[test]
fn every_alert_severity_maps_to_a_notice_severity() {
    assert_eq!(severity(AlertSeverity::Minor), Severity::Info);
    assert_eq!(severity(AlertSeverity::Moderate), Severity::Warning);
    assert_eq!(severity(AlertSeverity::Severe), Severity::Error);
    assert_eq!(severity(AlertSeverity::Extreme), Severity::Error);
    assert_eq!(
        severity(AlertSeverity::Unknown),
        Severity::Info,
        "a severity a newer provider invented states itself rather than alarming"
    );
}

#[test]
fn an_alert_becomes_a_notice_that_leads_to_its_own_page() {
    let mut warned = place();
    warned.alerts = vec![an_alert()];

    let [notice] = alerts(&warned).try_into().expect("one notice");
    assert_eq!(notice.severity, Severity::Error);
    assert_eq!(notice.title, "Thunderstorm warning until 21:00");
    assert_eq!(notice.page.as_deref(), Some("alert0"));

    let built = pages(&warned, UnitSystem::Metric, 5, false);
    let page = built
        .iter()
        .find(|page| page.key == "alert0")
        .expect("the page the notice names exists");
    assert_eq!(
        page.description.as_deref(),
        Some("Hail and gusts to 25 m/s.")
    );
    assert!(
        page.facts.is_empty(),
        "an alert drawer is its description alone"
    );
}

#[test]
fn a_place_with_no_alerts_builds_no_notice() {
    assert!(alerts(&place()).is_empty());
    assert_eq!(worst(&[]), None, "no alert is no condition to report");
}

/// The chip has room for one condition. Reporting the first alert listed would let a minor one
/// hide a severe one behind it.
#[test]
fn the_chip_reports_the_worst_alert_standing() {
    let alert = |severity| WeatherAlert {
        severity,
        ..an_alert()
    };

    assert_eq!(worst(&[alert(AlertSeverity::Minor)]), Some(Severity::Info));
    assert_eq!(
        worst(&[alert(AlertSeverity::Moderate)]),
        Some(Severity::Warning)
    );
    assert_eq!(
        worst(&[
            alert(AlertSeverity::Minor),
            alert(AlertSeverity::Extreme),
            alert(AlertSeverity::Moderate),
        ]),
        Some(Severity::Error),
        "the severe one is reported however late it was listed"
    );
}

#[test]
fn a_tooltip_substitutes_the_tokens_it_is_given() {
    assert_eq!(
        tooltip(
            "{place}: {temperature}, feels like {feels_like} ({condition})",
            "Vilnius",
            &current(Condition::LightRain)
        ),
        "Vilnius: 18°, feels like 16° (Light rain)"
    );
}
