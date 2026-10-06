use super::*;

pub(super) fn check() {
    let clock = WorldClock::new();
    let zone = |label: &str, timezone: &str| Zone {
        label: label.to_owned(),
        timezone: timezone.to_owned(),
        note: String::new(),
        icon_name: String::new(),
    };
    clock.set_zones(&[
        zone("Berlin", "Europe/Berlin"),
        zone("Auckland", "Pacific/Auckland"),
        zone("Midway", "Pacific/Midway"),
        zone("Nowhere", "Not/AZone"),
    ]);
    clock.set_now(&glib::DateTime::from_utc(2026, 9, 1, 12, 0, 0.0).expect("instant"));

    let clock_rows: Vec<Row> = children_of(&clock);
    let time_of = |row: &Row| row.value().unwrap_or_default();
    assert_eq!(clock_rows.len(), 4);
    assert_eq!(time_of(&clock_rows[0]), "14:00");
    assert_eq!(time_of(&clock_rows[1]), "00:00");
    assert_eq!(
        time_of(&clock_rows[3]),
        "—",
        "a timezone the system cannot resolve reads as unknown rather than silently as UTC"
    );

    assert_eq!(
        clock_rows[0].subtitle(),
        None,
        "a zone on the same date says nothing, so a list of neighbours stays one line each"
    );
    assert_eq!(clock_rows[1].subtitle().as_deref(), Some("Tomorrow"));
    assert_eq!(clock_rows[2].subtitle(), None);
    assert_eq!(clock_rows[3].subtitle(), None);

    let phase_of = |row: &Row| row.icon_name();
    assert_eq!(
        phase_of(&clock_rows[0]),
        Some("weather-clear-symbolic".to_owned()),
        "14:00 in Berlin is daylight, which is the one thing a world clock is consulted for"
    );
    assert_eq!(
        phase_of(&clock_rows[1]),
        Some("weather-clear-night-symbolic".to_owned()),
        "00:00 in Auckland is not"
    );
    assert_eq!(
        phase_of(&clock_rows[3]),
        None,
        "a zone that did not resolve claims nothing about daylight"
    );

    clock.set_now(&glib::DateTime::from_utc(2026, 9, 1, 5, 0, 0.0).expect("instant"));
    assert_eq!(clock_rows[2].subtitle().as_deref(), Some("Yesterday"));
    assert_eq!(
        phase_of(&clock_rows[1]),
        Some("weather-clear-symbolic".to_owned()),
        "17:00 in Auckland is daylight, so the icon follows the clock rather than the zone"
    );
    assert_eq!(clock_rows[1].subtitle(), None);

    clock.set_twelve_hour(true);
    assert!(
        time_of(&clock_rows[0]).starts_with("7:00"),
        "twelve-hour drops the padding strftime leaves in front of a single digit"
    );
    clock.set_twelve_hour(false);
    assert_eq!(time_of(&clock_rows[0]), "07:00");

    assert_eq!(
        clock_rows[0].tooltip_text().as_deref(),
        Some("Europe/Berlin · CEST (UTC+02:00)"),
        "the label is the city a user named; the tooltip is the zone it actually resolved \
             to, with the offset that makes the time checkable"
    );
    assert_eq!(
        clock_rows[3].tooltip_text().as_deref(),
        Some("Not/AZone"),
        "a zone that does not resolve still names itself, because that is the diagnostic"
    );
    assert!(
        clock_rows[0].can_target(),
        "a clock row still takes the pointer, because that is what raises the tooltip; it \
             just does not light up, since the tooltip is all the click would have given"
    );
    assert!(
        !clock_rows[0].is_focusable(),
        "and it is not a tab stop, because there is nothing to activate once you reach it"
    );

    clock.set_now(&glib::DateTime::from_utc(2026, 9, 1, 12, 0, 0.0).expect("instant"));
    clock.set_zones(&[
        Zone {
            note: "12° · Light rain".to_owned(),
            icon_name: "weather-showers-symbolic".to_owned(),
            ..zone("Berlin", "Europe/Berlin")
        },
        Zone {
            note: "9° · Clear".to_owned(),
            ..zone("Auckland", "Pacific/Auckland")
        },
    ]);
    assert_eq!(
        clock_rows[0].subtitle().as_deref(),
        Some("12° · Light rain"),
        "a zone with something to add carries it on the second line"
    );
    assert_eq!(
        phase_of(&clock_rows[0]),
        Some("weather-showers-symbolic".to_owned()),
        "and a zone that knows its weather draws that instead of the sun, so the icon cannot \
             contradict the line under it"
    );
    assert_eq!(
        phase_of(&clock_rows[1]),
        Some("weather-clear-night-symbolic".to_owned()),
        "a zone with no icon of its own still falls back to daylight"
    );
    assert_eq!(
        clock_rows[1].subtitle().as_deref(),
        Some("Tomorrow · 9° · Clear"),
        "and shares that line with the day note rather than taking a third, because a third \
             is what makes a clock list stop being glanceable"
    );
}
