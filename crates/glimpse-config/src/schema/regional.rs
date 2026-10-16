use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::environment;

/// What language this session reads, and how it writes a time of day and a measurement. Every
/// setting here defaults to what the environment already says, so an empty table is the right
/// answer on a machine whose locale is set up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Regional {
    /// Your text language, such as `ru` for Russian; leave it empty to use your system language.
    /// Changes apply after you restart glimpse.
    pub language: String,
    /// How you display times throughout glimpse. Your bar and lock screen clocks use their own
    /// formats.
    pub hour_format: HourFormat,
    /// Your weather units for temperature, wind speed and rainfall.
    pub units: Units,
}

impl Regional {
    pub fn language(&self) -> Option<&str> {
        Some(self.language.trim()).filter(|language| !language.is_empty())
    }

    pub fn twelve_hour(&self) -> bool {
        match self.hour_format {
            HourFormat::Twelve => true,
            HourFormat::TwentyFour => false,
            HourFormat::Locale => environment::locale_is_twelve_hour(),
        }
    }

    pub fn is_metric(&self) -> bool {
        match self.units {
            Units::Metric => true,
            Units::Imperial => false,
            Units::Locale => environment::locale_is_metric(),
        }
    }
}

/// How a time of day reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HourFormat {
    /// Your system's time format.
    #[default]
    Locale,
    /// `3:30 PM`.
    #[serde(rename = "12h")]
    Twelve,
    /// `15:30`.
    #[serde(rename = "24h")]
    TwentyFour,
}

/// Which units every reading is reported in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Units {
    /// Your system's measurement units.
    #[default]
    Locale,
    /// Read Celsius, kilometers per hour and millimeters.
    Metric,
    /// Fahrenheit, miles per hour, inches.
    Imperial,
}

#[cfg(test)]
mod tests;
