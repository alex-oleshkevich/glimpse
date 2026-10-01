use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::environment;

/// What language this session reads, and how it writes a time of day and a measurement. Every
/// setting here defaults to what the environment already says, so an empty table is the right
/// answer on a machine whose locale is set up.
///
/// The three keys are read by different owners: `glimpse-weather` resolves `units` and stamps the
/// answer onto every weather reading, while the panel, the lock screen and the wallpaper resolve
/// `language` and `hour-format` for themselves. Nothing else in glimpse asks the environment
/// these questions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Regional {
    /// Which translation catalog the interface is drawn in, such as `ru`. Empty follows the
    /// environment, which is `LANGUAGE`, then `LC_MESSAGES`, then `LANG`. Naming one here sets
    /// `LANGUAGE` for the process, so it applies whether or not that locale has been generated.
    ///
    /// This chooses the catalog and nothing else: weekday names, the twelve-hour question and the
    /// unit system keep following the environment, because a person reading a Russian interface in
    /// Chicago still wants their own clock and their own thermometer.
    ///
    /// A change here takes effect when the binary next starts. A GTK template resolves its text as
    /// each widget is built, so re-reading it in a running process would leave what is already on
    /// screen in the old language and everything opened afterwards in the new one.
    pub language: String,
    /// How a time of day reads wherever glimpse composes one itself — the world clock, the times
    /// inside event rows, sunrise and sunset. A `strftime` format you write yourself, such as the
    /// clock applet's `label-format`, is yours and is not affected by this.
    pub hour_format: HourFormat,
    /// Whether temperatures, wind speeds and rainfall are reported in metric or imperial units.
    /// Every reading in one weather report uses the same system, and the panel prints the symbols
    /// that go with it.
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
    /// Follow `LC_TIME`, which is what the locale's own `%X` resolves to.
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
    /// Follow `LC_MEASUREMENT`. Every locale glibc ships is metric except `en_US`, so this is
    /// metric unless the machine is set to American English.
    #[default]
    Locale,
    /// Celsius, kilometres per hour, millimetres.
    Metric,
    /// Fahrenheit, miles per hour, inches.
    Imperial,
}

#[cfg(test)]
mod tests;
