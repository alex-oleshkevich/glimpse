use super::*;

#[test]
fn positions_are_one_based_and_counted_in_characters() {
    let text = "a = 1\nb = \"ключ\"\nc = 3\n";

    assert_eq!(line_column(text, 0), (1, 1));
    assert_eq!(line_column(text, 6), (2, 1));
    assert_eq!(line_column(text, 17), (2, 9));
}

#[test]
fn an_offset_off_the_end_or_mid_character_does_not_panic() {
    let text = "x = \"é\"\n";

    assert_eq!(line_column(text, 999), (2, 1));
    assert_eq!(line_column(text, 6), (1, 6));
}

#[test]
fn a_parse_error_names_the_position_without_quoting_the_line() {
    let text = "token = ghp_thisisasecret\n";
    let Err(error) = text.parse::<toml::Table>() else {
        panic!("expected a parse failure");
    };

    let rendered = ConfigError::parse(Path::new("config.toml"), text, &error).to_string();

    assert!(rendered.starts_with("config.toml:1:9:"), "{rendered}");
    assert!(!rendered.contains("ghp_thisisasecret"), "{rendered}");
}

#[test]
fn a_type_mismatch_names_the_key_without_quoting_the_value() {
    #[derive(serde::Deserialize)]
    struct Fixture {
        #[serde(rename = "count")]
        _count: u64,
    }

    let raw = config::Config::builder()
        .add_source(config::File::from_str(
            "count = \"ghp_thisisasecret\"",
            config::FileFormat::Toml,
        ))
        .build()
        .expect("valid TOML, just the wrong type for `count`");

    let Err(error) = raw.try_deserialize::<Fixture>() else {
        panic!("expected a type mismatch");
    };

    let rendered = ConfigError::schema(error).to_string();

    assert!(rendered.contains("count"), "{rendered}");
    assert!(!rendered.contains("ghp_thisisasecret"), "{rendered}");
}
