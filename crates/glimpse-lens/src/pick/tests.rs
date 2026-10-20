use super::*;

#[test]
fn json_carries_the_notation_the_value_and_every_channel() {
    let parsed: serde_json::Value =
        serde_json::from_str(&json(ColorFormat::Rgb, [224, 86, 63])).unwrap();

    assert_eq!(
        parsed,
        serde_json::json!({
            "format": "rgb",
            "value": "rgb(224 86 63)",
            "red": 224,
            "green": 86,
            "blue": 63,
        })
    );
}

#[test]
fn json_prints_keys_in_the_readme_order() {
    assert_eq!(
        json(ColorFormat::Hex, [224, 86, 63]),
        r##"{"blue":63,"format":"hex","green":86,"red":224,"value":"#E0563F"}"##
    );
}
