fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
}

#[test]
fn the_document_sets_the_floor() {
    let parsed = load("[brightness]\nminimum = 10\n")
        .expect("minimum is a key of this table")
        .brightness;

    assert_eq!(parsed.minimum, 10);
}

#[test]
fn ddc_can_be_turned_off() {
    let parsed = load("[brightness]\nddc = false\n")
        .expect("ddc is a key of this table")
        .brightness;

    assert!(!parsed.ddc);
}

#[test]
fn the_cadence_knobs_udev_removed_are_not_keys_of_this_table() {
    let rendered = load("[brightness]\npoll-interval = 5\n")
        .expect_err("`poll-interval` is not a key of this table")
        .to_string();

    assert!(
        rendered.contains("poll-interval") && rendered.contains("brightness"),
        "the error must name the table and `poll-interval`, got {rendered}"
    );
}

#[test]
fn a_floor_above_the_hardware_maximum_is_refused() {
    let rendered = load("[brightness]\nminimum = 200\n")
        .expect_err("200 is not a percent")
        .to_string();

    assert!(
        rendered.contains("minimum"),
        "the error must name `minimum`, got {rendered}"
    );
}
