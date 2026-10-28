use super::*;

fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
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
