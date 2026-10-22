use super::*;

fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
}

#[test]
fn an_absent_table_polls_every_fifteen_minutes() {
    let weather = load("").expect("an absent table is fine").weather;

    assert_eq!(weather.provider, Provider::OpenMeteo);
    assert_eq!(weather.poll_interval, 900);
    assert_eq!(weather.forecast_days, 7);
}

#[test]
fn every_key_reads_kebab_case() {
    let parsed = load("[weather]\npoll-interval = 1200\nforecast-days = 3\n")
        .expect("kebab-case keys")
        .weather;

    assert_eq!(parsed.poll_interval, 1200);
    assert_eq!(parsed.forecast_days, 3);
}

#[test]
fn a_place_is_watched_rather_than_configured() {
    let rendered = load("[weather]\nplaces = []\n")
        .expect_err("`places` is not a key of this table")
        .to_string();

    assert!(
        rendered.contains("places"),
        "the error must name `places`, got {rendered}"
    );
}

#[test]
fn both_providers_read_and_a_third_is_refused() {
    assert_eq!(
        load("[weather]\nprovider = \"met-no\"\n")
            .expect("met.no is a provider")
            .weather
            .provider,
        Provider::MetNo
    );
    load("[weather]\nprovider = \"accuweather\"\n")
        .expect_err("a provider nothing implements is a document error, not a fallback");
}

#[test]
fn units_is_not_a_key_of_this_table() {
    load("[weather]\nunits = \"imperial\"\n").expect_err("`units` belongs to `[regional]`");
}
