use glimpse_dbus::weather::GeoCoordinates;
use glimpse_dbus::weather::{PlaceWeather, UnitSystem, WatchedPlace};
use glimpsectl::commands::{
    ABSENT,
    weather::{degrees, name, row},
};

fn place(place: WatchedPlace) -> PlaceWeather {
    PlaceWeather {
        place,
        coordinates: GeoCoordinates {
            latitude: 52.52,
            longitude: 13.405,
        },
        city: None,
        country_code: None,
        utc_offset_seconds: 0,
        current: None,
        hours: Vec::new(),
        days: Vec::new(),
        alerts: Vec::new(),
    }
}

#[test]
fn a_resolved_place_reads_as_its_city_and_country() {
    let mut resolved = place(WatchedPlace::Here);
    resolved.city = Some("Berlin".to_owned());
    resolved.country_code = Some("DE".to_owned());
    assert_eq!(name(&resolved), "Berlin, DE");
}

#[test]
fn an_unresolved_place_falls_back_to_what_was_asked_for() {
    assert_eq!(name(&place(WatchedPlace::Here)), "here");
    assert_eq!(
        name(&place(WatchedPlace::Location {
            name: "Berlin, DE".to_owned()
        })),
        "Berlin, DE"
    );
    assert_eq!(
        name(&place(WatchedPlace::Coordinates {
            latitude: 52.52,
            longitude: 13.405
        })),
        "52.520, 13.405"
    );
}

#[test]
fn a_place_with_no_reading_says_so_rather_than_printing_a_zero() {
    let [_, temperature, condition, alerts] = row(&place(WatchedPlace::Here), None);
    assert_eq!(temperature, ABSENT);
    assert_eq!(condition, ABSENT);
    assert_eq!(alerts, ABSENT);
}

#[test]
fn units_decide_the_degree_symbol() {
    assert_eq!(degrees(Some(UnitSystem::Imperial)), "°F");
    assert_eq!(degrees(Some(UnitSystem::Metric)), "°C");
    assert_eq!(degrees(None), "°C");
}
