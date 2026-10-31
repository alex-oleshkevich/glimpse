use super::*;
use zbus::zvariant::Str;

fn string(value: &str) -> OwnedValue {
    OwnedValue::from(Str::from(value))
}

fn invalid(values: HashMap<String, OwnedValue>, expected: &str) {
    assert!(matches!(
        parse(values),
        Err(fdo::Error::InvalidArgs(message)) if message == expected
    ));
}

#[test]
fn rejects_nothing_to_show() {
    invalid(HashMap::new(), "nothing to show");
    invalid(
        HashMap::from([("icon".into(), string("")), ("label".into(), string(""))]),
        "nothing to show",
    );
}

#[test]
fn rejects_out_of_range_and_nonfinite_values() {
    for value in [3.0, f64::NAN, -0.1, f64::INFINITY, f64::NEG_INFINITY] {
        invalid(
            HashMap::from([("value".into(), OwnedValue::from(value))]),
            "value must be between 0 and 2",
        );
    }
}

#[test]
fn accepts_value_boundaries() {
    for value in [0.0, 2.0] {
        let message =
            parse(HashMap::from([("value".into(), OwnedValue::from(value))])).expect("valid value");
        assert_eq!(message.value, Some(value));
    }
}

#[test]
fn discards_icon_paths_and_oversized_names() {
    for icon in ["/tmp/icon.svg".to_owned(), "x".repeat(129), "é".repeat(65)] {
        let message = parse(HashMap::from([
            ("icon".into(), string(&icon)),
            ("label".into(), string("Headphones")),
        ]))
        .expect("label remains visible");
        assert!(message.icon.is_empty());
        assert_eq!(message.label, "Headphones");
        invalid(
            HashMap::from([("icon".into(), string(&icon))]),
            "nothing to show",
        );
    }
}

#[test]
fn rejects_wrong_key_types() {
    for key in ["icon", "label", "value"] {
        invalid(
            HashMap::from([(key.into(), OwnedValue::from(true))]),
            &format!("{key} has the wrong type"),
        );
    }
    invalid(
        HashMap::from([("value".into(), OwnedValue::from(1_u32))]),
        "value has the wrong type",
    );
}

#[test]
fn ignores_unknown_keys() {
    let message = parse(HashMap::from([
        ("unknown".into(), OwnedValue::from(true)),
        ("label".into(), string("English (US)")),
    ]))
    .expect("known label");
    assert_eq!(message.label, "English (US)");
    assert!(message.icon.is_empty());
    assert_eq!(message.value, None);
    invalid(
        HashMap::from([("unknown".into(), OwnedValue::from(true))]),
        "nothing to show",
    );
}

#[test]
fn accepts_icon_and_value() {
    let message = parse(HashMap::from([
        ("icon".into(), string("audio-volume-high-symbolic")),
        ("value".into(), OwnedValue::from(0.45)),
    ]))
    .expect("valid icon and value");
    assert_eq!(message.icon, "audio-volume-high-symbolic");
    assert!(message.label.is_empty());
    assert_eq!(message.value, Some(0.45));
}

#[test]
fn caps_and_cleans_labels() {
    let label = format!("\u{1b}{}", "é".repeat(140));
    let message = parse(HashMap::from([("label".into(), string(&label))])).expect("valid label");
    assert_eq!(message.label, format!("{}…", "é".repeat(120)));
    let message = parse(HashMap::from([(
        "label".into(),
        string("  Headphones\u{202e}\n connected  "),
    )]))
    .expect("valid label");
    assert_eq!(message.label, "Headphones connected");
}
