use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a forecast is fetched. There are no places here: a place is registered by whatever wants to
/// see it, through `WatchPlace`, and held for as long as it keeps asking. Nothing is fetched and
/// no location is resolved until something does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Weather {
    /// Which service a forecast comes from. `open-meteo` needs no account and no key.
    pub provider: Provider,
    /// How often a forecast is fetched again, in seconds. The provider recomputes current
    /// conditions every fifteen minutes, so anything shorter asks again for data that has not
    /// moved; the provider's `Refresh` method is how a person asks for it now. Values below 600 are raised.
    pub poll_interval: u64,
    /// How many days the daily forecast covers, today included. Clamped to 1..=10.
    pub forecast_days: u8,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            provider: Provider::OpenMeteo,
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

#[cfg(test)]
mod tests;
