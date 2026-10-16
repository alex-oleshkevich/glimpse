use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a forecast is fetched. There are no places here: a place is registered by whatever wants to
/// see it, through `WatchPlace`, and held for as long as it keeps asking. Nothing is fetched and
/// no location is resolved until something does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Weather {
    /// Your weather forecast service.
    pub provider: Provider,
    /// How often the forecast updates, in seconds, with a minimum of 600.
    pub poll_interval: u64,
    /// How many forecast days you get, including today, from 1 to 10.
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
    /// Open-Meteo.
    #[default]
    OpenMeteo,
    /// The Norwegian Meteorological Institute.
    MetNo,
}

#[cfg(test)]
mod tests;
