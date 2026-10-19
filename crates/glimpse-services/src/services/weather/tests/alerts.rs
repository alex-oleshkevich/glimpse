use super::*;

#[test]
fn an_unknown_condition_variant_decodes_rather_than_failing_the_payload() {
    let decoded: Condition = serde_json::from_str(r#"{"condition":"volcanic_ash"}"#)
        .expect("an unrecognized condition still decodes");

    assert_eq!(decoded, Condition::Unknown);
}

#[tokio::test]
async fn a_second_providers_alerts_go_through_the_same_gate() {
    let mut harness = harness().await;
    call(&mut harness, Asked::Watch(at(54.6872, 25.2797)))
        .await
        .expect("a place to hold a reading");

    let fetch = fetched(
        &harness.service,
        vec![Reading {
            alerts: vec![WeatherAlert {
                severity: AlertSeverity::Severe,
                headline: "Gale\u{202e}gpj.exe".to_owned(),
                description: None,
                source: None,
                starts_at: None,
                expires_at: None,
            }],
            ..a_reading()
        }],
    );
    harness.service.absorb(&fetch);

    assert_eq!(harness.service.places.len(), 1);
    assert_eq!(
        harness.service.places[0].alerts[0].headline, "Gale gpj.exe",
        "the override is stripped whichever provider sent it"
    );
}

#[test]
fn an_alert_headline_is_capped_and_stripped_of_bidi_overrides() {
    let alert = WeatherAlert {
        description: Some("  line\tone\n\nline two  ".to_owned()),
        source: Some("LHMT\u{202e}".to_owned()),
        ..an_alert("Thunderstorm\u{202e}gpj.exe warning")
    };

    let [cleaned] = sanitized(&[alert]).try_into().expect("one alert back");

    assert_eq!(cleaned.headline, "Thunderstorm gpj.exe warning");
    assert_eq!(cleaned.description.as_deref(), Some("line one line two"));
    assert_eq!(cleaned.source.as_deref(), Some("LHMT"));
    assert_eq!(
        sanitized(&[an_alert(&"e".repeat(HEADLINE + 10))])[0].headline,
        format!("{}…", "e".repeat(HEADLINE))
    );
}

#[test]
fn more_alerts_than_the_cap_are_dropped_rather_than_rendered() {
    let many: Vec<_> = (0..MOST_ALERTS + 5)
        .map(|index| an_alert(&format!("alert {index}")))
        .collect();

    assert_eq!(sanitized(&many).len(), MOST_ALERTS);
}

#[test]
fn a_payload_without_an_alerts_field_still_decodes() {
    let older = serde_json::json!({
        "place": { "at": "here" },
        "coordinates": { "latitude": 54.6872, "longitude": 25.2797 },
        "utc_offset_seconds": 7200,
        "current": null,
        "hours": [],
        "days": []
    });

    let decoded: PlaceWeather = serde_json::from_value(older).expect("an older payload decodes");

    assert!(decoded.alerts.is_empty());
}

#[test]
fn every_alert_severity_decodes_from_its_wire_name() {
    for (name, severity) in [
        ("minor", AlertSeverity::Minor),
        ("moderate", AlertSeverity::Moderate),
        ("severe", AlertSeverity::Severe),
        ("extreme", AlertSeverity::Extreme),
        ("something_new", AlertSeverity::Unknown),
    ] {
        let decoded: AlertSeverity =
            serde_json::from_value(serde_json::json!({ "severity": name }))
                .unwrap_or_else(|_| panic!("{name} decodes"));
        assert_eq!(decoded, severity, "{name}");
    }
}
