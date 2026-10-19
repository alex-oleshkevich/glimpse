use chrono::TimeZone as _;
use glimpse_dbus::weather::WeatherProvider;
use glimpse_dbus::weather::{AlertSeverity, CurrentWeather, GeoCoordinates, WeatherAlert};
use glimpse_widgets::Severity;

use super::*;

fn applet() -> Weather {
    let provider = WeatherProvider::unavailable("test provider unavailable");
    let mut applet = Weather::start(provider.handle());
    applet.trouble = None;
    applet
}

#[test]
fn an_unavailable_provider_renders_a_warning() {
    let provider = WeatherProvider::unavailable("weather provider unavailable");
    let mut applet = Weather::start(provider.handle());

    applet.refresh();

    let indicators = applet.indicators();
    assert_eq!(indicators.len(), 1);
    assert_eq!(indicators[0].severity, Some(Severity::Warning));
    assert_eq!(
        indicators[0].tooltip.as_deref(),
        Some("weather provider unavailable")
    );
}

#[test]
fn stale_weather_keeps_its_reading_and_renders_a_warning() {
    let mut applet = applet();
    let mut place = reading();
    place.alerts.push(WeatherAlert {
        severity: AlertSeverity::Minor,
        headline: "minor alert".to_owned(),
        description: None,
        source: None,
        starts_at: None,
        expires_at: None,
    });
    applet.place = Some(place);
    applet.stale = true;
    applet.trouble = Some("weather provider unavailable".to_owned());

    applet.refresh();

    let indicators = applet.indicators();
    assert_eq!(indicators.len(), 1);
    assert_eq!(indicators[0].severity, Some(Severity::Warning));
    assert!(indicators[0].label.is_some());
    assert_eq!(
        indicators[0].tooltip.as_deref(),
        Some("weather provider unavailable")
    );
}

#[test]
fn a_fixed_place_the_provider_never_serves_is_reported_on_the_tick_and_not_before() {
    let mut applet = applet();
    applet.watching = WatchedPlace::Coordinates {
        latitude: 54.6872,
        longitude: 25.2797,
    };
    applet.owner = true;
    applet.place = None;

    applet.refresh();
    assert!(
        applet.indicators().is_empty(),
        "the gap before the first fetch renders nothing, as it always has"
    );

    applet.note_unserved();
    applet.refresh();

    let indicators = applet.indicators();
    assert_eq!(indicators.len(), 1);
    assert_eq!(indicators[0].severity, Some(Severity::Warning));
    assert_eq!(
        indicators[0].tooltip.as_deref(),
        Some("The weather provider is not serving this place.")
    );
}

#[test]
fn a_snapshot_does_not_decide_whether_a_place_is_unserved() {
    let mut applet = applet();
    applet.unserved = true;

    applet.sync();

    assert!(
        applet.unserved,
        "sync reads the provider; only the tick judges how long a place has been missing"
    );
}

#[test]
fn here_without_a_fix_stays_silent_however_long_it_waits() {
    let mut applet = applet();
    applet.owner = true;
    applet.place = None;

    applet.note_unserved();
    applet.refresh();

    assert!(
        applet.indicators().is_empty(),
        "`here` may legitimately never resolve, so it never becomes a warning"
    );
}

#[test]
fn a_place_that_arrives_clears_the_warning() {
    let mut applet = applet();
    applet.watching = WatchedPlace::Here;
    applet.owner = true;
    applet.note_unserved();
    applet.place = Some(reading());

    applet.note_unserved();
    applet.refresh();

    let indicators = applet.indicators();
    assert_eq!(indicators.len(), 1);
    assert_eq!(indicators[0].severity, None);
}

fn reading() -> PlaceWeather {
    PlaceWeather {
        place: WatchedPlace::Here,
        coordinates: GeoCoordinates {
            latitude: 54.6872,
            longitude: 25.2797,
        },
        city: Some("Vilnius".to_owned()),
        country_code: Some("LT".to_owned()),
        utc_offset_seconds: 10_800,
        current: Some(CurrentWeather {
            observed_at: Utc.with_ymd_and_hms(2026, 9, 8, 12, 0, 0).unwrap(),
            condition: Condition::LightRain,
            is_day: true,
            temperature: 18.4,
            apparent_temperature: Some(16.2),
            humidity: Some(72),
            wind_speed: Some(11.7),
            wind_direction: Some(230),
            precipitation: Some(0.4),
        }),
        hours: Vec::new(),
        days: Vec::new(),
        alerts: Vec::new(),
    }
}

#[test]
fn a_place_with_no_current_reading_renders_no_chip() {
    let mut applet = applet();
    applet.refresh();
    assert!(applet.indicators().is_empty(), "no value yet is no chip");

    applet.place = Some(PlaceWeather {
        current: None,
        ..reading()
    });
    applet.refresh();
    assert!(
        applet.indicators().is_empty(),
        "a place the provider answered for with no current reading is the same absence"
    );

    applet.place = Some(reading());
    applet.refresh();
    assert_eq!(applet.indicators().len(), 1);
}

#[test]
fn the_chip_reads_the_one_rounding_site_and_carries_a_symbolic_icon() {
    let mut applet = applet();
    applet.place = Some(reading());
    applet.refresh();

    let [chip] = applet.indicators().try_into().expect("one chip");
    assert_eq!(chip.label.as_deref(), Some("18°"));
    assert!(chip.icon.is_some());
    assert_eq!(
        chip.tooltip.as_deref(),
        Some("Vilnius, LT\nLight rain, 18°, feels like 16°"),
        "an applet with no tooltip-format shows the default one"
    );
}

#[test]
fn the_icon_is_rebuilt_only_when_the_condition_changes() {
    let mut applet = applet();
    applet.place = Some(reading());
    applet.refresh();
    let first = applet.icon.clone().expect("an icon").1;

    applet.refresh();
    let again = applet.icon.clone().expect("an icon").1;
    assert!(
        first.equal(Some(&again)),
        "an unchanged condition reuses the icon it already built"
    );

    let mut cleared = reading();
    if let Some(current) = cleared.current.as_mut() {
        current.condition = Condition::ClearSky;
    }
    applet.place = Some(cleared);
    applet.refresh();
    let changed = applet.icon.clone().expect("an icon").1;
    assert!(!first.equal(Some(&changed)));
}

#[test]
fn an_alert_takes_the_chips_icon_and_its_color() {
    let mut applet = applet();
    applet.place = Some(reading());
    applet.refresh();

    let [calm] = applet.indicators().try_into().expect("one chip");
    assert_eq!(calm.severity, None);

    let mut warned = reading();
    warned.alerts = vec![WeatherAlert {
        severity: AlertSeverity::Severe,
        headline: "Thunderstorm warning until 21:00".to_owned(),
        description: None,
        source: None,
        starts_at: None,
        expires_at: None,
    }];
    applet.place = Some(warned);
    applet.refresh();

    let [raised] = applet.indicators().try_into().expect("one chip");
    assert_eq!(raised.severity, Some(Severity::Error));
    assert_eq!(
        applet.icon.as_ref().map(|(name, _)| name.as_str()),
        Some(render::ALERT_ICON)
    );
    assert_eq!(
        raised.label.as_deref(),
        Some("18°"),
        "the reading stays: an alert changes what the chip warns about, not what it reads"
    );
}

#[test]
fn a_configured_label_wins_over_the_coordinates_the_provider_answered_with() {
    let mut applet = applet();
    applet.place = Some(reading());
    assert_eq!(applet.label(), "Vilnius, LT");

    applet.settings.label = Some("Vilnius".to_owned());
    assert_eq!(applet.label(), "Vilnius");
}

#[test]
fn a_missing_canonical_name_falls_back_to_coordinates() {
    let mut applet = applet();
    applet.place = Some(PlaceWeather {
        city: None,
        country_code: None,
        ..reading()
    });
    assert_eq!(applet.label(), "54.69, 25.28");
}

#[test]
fn a_place_table_becomes_the_place_the_wire_takes() {
    assert_eq!(watched(&WeatherPlace::Here {}), WatchedPlace::Here);
    assert_eq!(
        watched(&WeatherPlace::Coordinates {
            latitude: 54.6872,
            longitude: 25.2797
        }),
        WatchedPlace::Coordinates {
            latitude: 54.6872,
            longitude: 25.2797
        }
    );
    assert_eq!(
        watched(&WeatherPlace::Location {
            name: "Vilnius, LT".to_owned(),
        }),
        WatchedPlace::Location {
            name: "Vilnius, LT".to_owned(),
        }
    );
}
