use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a forecast is fetched. There are no places here: a place is registered by whatever wants to
/// see it, through `weather.watch`, and held for as long as it keeps asking. Nothing is fetched and
/// no location is resolved until something does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Weather {
    /// Which service a forecast comes from. `open-meteo` needs no account and no key.
    pub provider: Provider,
    /// Whether temperatures, wind speeds and rainfall are reported in metric or imperial units.
    /// Every reading in one report uses the same system, and the panel prints the symbols that go
    /// with it.
    pub units: Units,
    /// How often a forecast is fetched again, in seconds. The provider recomputes current
    /// conditions every fifteen minutes, so anything shorter asks again for data that has not
    /// moved; `weather.refresh` is how a person asks for it now. Values below 600 are raised.
    pub poll_interval: u64,
    /// How many days the daily forecast covers, today included. Clamped to 1..=10. The panel's
    /// list starts at tomorrow, so it shows one fewer day than this asks for.
    pub forecast_days: u8,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            provider: Provider::OpenMeteo,
            units: Units::Metric,
            poll_interval: 900,
            forecast_days: 7,
        }
    }
}

/// Where a forecast comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    /// Open-Meteo, which needs no account and no key.
    #[default]
    OpenMeteo,
    /// The Norwegian Meteorological Institute, which needs no key either but asks for a
    /// User-Agent naming the application. It is the only source that carries official warnings.
    MetNo,
}

/// Which units every reading is reported in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Units {
    /// Celsius, kilometres per hour, millimetres.
    #[default]
    Metric,
    /// Fahrenheit, miles per hour, inches.
    Imperial,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn an_absent_table_polls_every_fifteen_minutes() {
        let weather = load("").expect("an absent table is fine").weather;

        assert_eq!(weather.provider, Provider::OpenMeteo);
        assert_eq!(weather.units, Units::Metric);
        assert_eq!(weather.poll_interval, 900);
        assert_eq!(weather.forecast_days, 7);
    }

    #[test]
    fn every_key_reads_kebab_case() {
        let parsed =
            load("[weather]\nunits = \"imperial\"\npoll-interval = 1200\nforecast-days = 3\n")
                .expect("kebab-case keys")
                .weather;

        assert_eq!(parsed.units, Units::Imperial);
        assert_eq!(parsed.poll_interval, 1200);
        assert_eq!(parsed.forecast_days, 3);
    }

    /// A place is a lease held by whoever wants to see it, not a document the daemon reads. Writing
    /// one here has to be an error rather than a setting that is quietly ignored.
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
    fn an_unknown_unit_system_is_refused() {
        load("[weather]\nunits = \"kelvin\"\n").expect_err("`kelvin` is not a unit system");
    }
}
